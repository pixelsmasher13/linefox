//! Telegram Bot integration for Linefox
//!
//! Long-polls the Telegram Bot API using reqwest (already a project dependency — no new crates).
//! Runs as a background Tokio task alongside the existing async runtime.
//!
//! Flow:
//!   User sends message → classify intent → ack reply → execute automation → send completion message
//!
//!   Clarification flow:
//!   While an automation is running the engine may pause and ask a question (CLARIFICATION action).
//!   The bot forwards that question to Telegram. The user's next message in the same chat is treated
//!   as the clarification answer (not as a new task) and is injected back into the engine via
//!   submit_clarification(). The engine then continues.
//!
//! Settings keys (stored in the `settings` SQLite table):
//!   `telegram_bot_token`    — BotFather token, e.g. "123456:ABC-DEF..."
//!   `telegram_allowed_users` — comma-separated Telegram user IDs allowed to trigger automations.
//!                              Leave empty to allow anyone who messages the bot.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use log::{info, warn, error};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::configuration::state::ServiceAccess;
use crate::repository::{
    settings_repository,
    automation_repository,
    automation_execution_repository,
};

// ===== Clarification state =====
//
// When the automation engine needs a clarification it emits `clarification_required`.
// We store the chat_id + token that triggered the current run so the event listener
// can forward the question to the right Telegram chat.
// The user's next reply in that chat is then routed to submit_clarification() instead
// of being treated as a new task.

lazy_static::lazy_static! {
    /// (chat_id, bot_token) of the run that is currently paused waiting for mid-task clarification.
    static ref PENDING_CLARIFICATION: Arc<Mutex<Option<(i64, String)>>> =
        Arc::new(Mutex::new(None));
    /// (chat_id, original_vague_prompt) when bot asked the user to clarify before starting a task.
    static ref PENDING_PRE_TASK_CLARIFICATION: Arc<Mutex<Option<(i64, String)>>> =
        Arc::new(Mutex::new(None));
    /// Per-chat conversation history: chat_id → recent (role, content) pairs.
    static ref CHAT_HISTORY: Mutex<HashMap<i64, VecDeque<(String, String)>>> =
        Mutex::new(HashMap::new());
}

const MAX_CHAT_HISTORY: usize = 8; // 4 exchanges

fn record_message(chat_id: i64, role: &str, content: &str) {
    let mut history = CHAT_HISTORY.lock().unwrap();
    let msgs = history.entry(chat_id).or_insert_with(VecDeque::new);
    msgs.push_back((role.to_string(), content.to_string()));
    if msgs.len() > MAX_CHAT_HISTORY {
        msgs.pop_front();
    }
}

fn get_chat_history(chat_id: i64) -> String {
    let history = CHAT_HISTORY.lock().unwrap();
    match history.get(&chat_id) {
        Some(msgs) if !msgs.is_empty() => msgs
            .iter()
            .map(|(role, content)| format!("{}: {}", role, content))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => "(No prior conversation in this session)".to_string(),
    }
}

// ===== Settings keys =====

pub const SETTING_BOT_TOKEN: &str = "telegram_bot_token";
pub const SETTING_ALLOWED_USERS: &str = "telegram_allowed_users";

// ===== Global state =====

static BOT_RUNNING: AtomicBool = AtomicBool::new(false);

// ===== Telegram API types =====

#[derive(Deserialize, Debug)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
}

#[derive(Deserialize, Debug, Clone)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
}

#[derive(Deserialize, Debug, Clone)]
struct TgMessage {
    chat: TgChat,
    from: Option<TgUser>,
    text: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct TgChat {
    id: i64,
}

#[derive(Deserialize, Debug, Clone)]
struct TgUser {
    id: i64,
}

// ===== Public API =====

/// Start the Telegram bot polling loop. Safe to call multiple times — second calls are no-ops.
pub fn start_telegram_bot(app_handle: AppHandle) {
    if BOT_RUNNING.swap(true, Ordering::SeqCst) {
        info!("[TelegramBot] Already running — skipping duplicate start");
        return;
    }
    info!("[TelegramBot] Starting polling loop");

    // Register a one-time listener for clarification_required events.
    // When the automation engine pauses mid-task to ask the user a question,
    // this forwards that question to whichever Telegram chat triggered the run.
    app_handle.listen("clarification_required", |event| {
        let payload: serde_json::Value =
            serde_json::from_str(event.payload()).unwrap_or_default();
        let question = payload["question"]
            .as_str()
            .unwrap_or("Can you provide more information to continue?")
            .to_string();
        let reasoning = payload["reasoning"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let pending = PENDING_CLARIFICATION.lock().unwrap().clone();
        if let Some((chat_id, token)) = pending {
            tauri::async_runtime::spawn(async move {
                let mut msg = format!("Question: {}", question);
                if !reasoning.is_empty() {
                    msg.push_str(&format!("\n\n({})", reasoning));
                }
                msg.push_str("\n\nReply to this message to continue the task.");
                let _ = tg_send(&token, chat_id, &msg).await;
            });
        }
    });

    tauri::async_runtime::spawn(async move {
        poll_loop(app_handle).await;
    });
}

/// Stop the polling loop on the next iteration.
pub fn stop_telegram_bot() {
    BOT_RUNNING.store(false, Ordering::SeqCst);
    info!("[TelegramBot] Stop requested");
}

/// Restart the bot (e.g. after the token is saved in settings).
pub fn restart_telegram_bot(app_handle: AppHandle) {
    stop_telegram_bot();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        BOT_RUNNING.store(false, Ordering::SeqCst);
        start_telegram_bot(app_handle);
    });
}

pub fn is_running() -> bool {
    BOT_RUNNING.load(Ordering::SeqCst)
}

// ===== Per-chat sequential message queue =====
//
// Each chat_id gets its own unbounded mpsc channel. Messages for the same chat
// are processed strictly in order (no concurrent task launches per chat).
// Different chats can still run in parallel.
//
// CHAT_BUSY tracks whether the worker for a given chat is currently processing a message.
// If a new message arrives while the worker is busy, we immediately send a queued acknowledgment
// so the user isn't left wondering if their message was received.

type ChatSender = mpsc::UnboundedSender<(AppHandle, String, TgMessage)>;

lazy_static::lazy_static! {
    static ref CHAT_QUEUES: Mutex<HashMap<i64, ChatSender>> = Mutex::new(HashMap::new());
    static ref CHAT_BUSY: Mutex<HashMap<i64, bool>> = Mutex::new(HashMap::new());
}

fn is_chat_busy(chat_id: i64) -> bool {
    *CHAT_BUSY.lock().unwrap().get(&chat_id).unwrap_or(&false)
}

fn set_chat_busy(chat_id: i64, busy: bool) {
    CHAT_BUSY.lock().unwrap().insert(chat_id, busy);
}

fn get_or_create_chat_queue(chat_id: i64) -> ChatSender {
    let mut queues = CHAT_QUEUES.lock().unwrap();
    if let Some(tx) = queues.get(&chat_id) {
        if !tx.is_closed() {
            return tx.clone();
        }
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<(AppHandle, String, TgMessage)>();
    tokio::spawn(async move {
        while let Some((app, tok, msg)) = rx.recv().await {
            set_chat_busy(chat_id, true);
            process_message(app, tok, msg).await;
            set_chat_busy(chat_id, false);
        }
    });
    queues.insert(chat_id, tx.clone());
    tx
}

// ===== Polling loop =====

async fn poll_loop(app_handle: AppHandle) {
    let mut offset: i64 = 0;

    loop {
        if !BOT_RUNNING.load(Ordering::SeqCst) {
            info!("[TelegramBot] Stopped");
            break;
        }

        let token = match get_bot_token(&app_handle) {
            Some(t) => t,
            None => {
                // Token not yet configured — check every 15 s
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                continue;
            }
        };

        match fetch_updates(&token, offset).await {
            Ok(updates) => {
                for update in updates {
                    offset = update.update_id + 1;
                    if let Some(msg) = update.message {
                        let chat_id = msg.chat.id;
                        let busy = is_chat_busy(chat_id);
                        let tx = get_or_create_chat_queue(chat_id);
                        if busy {
                            // Worker is processing another message — ack immediately so the user
                            // knows their message was received and will be handled next.
                            let tok2 = token.clone();
                            tokio::spawn(async move {
                                let _ = tg_send(&tok2, chat_id, "⏳ I'm still working on your previous request. I'll get to this as soon as it's done.").await;
                            });
                        }
                        let _ = tx.send((app_handle.clone(), token.clone(), msg));
                    }
                }
            }
            Err(e) => {
                warn!("[TelegramBot] getUpdates error: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

// ===== Message handling =====

async fn process_message(app_handle: AppHandle, token: String, msg: TgMessage) {
    let chat_id = msg.chat.id;
    let text = match msg.text.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => return, // ignore non-text (photos, stickers, etc.)
    };

    // Allowlist check
    let user_id = msg.from.as_ref().map(|u| u.id).unwrap_or(0);
    if !is_authorized(&app_handle, user_id) {
        info!("[TelegramBot] Rejected message from unauthorized user {}", user_id);
        return;
    }

    info!("[TelegramBot] Message from chat {} user {}: {:?}", chat_id, user_id, text);

    // Pre-task clarification: user answered a "need more details" question before any task ran.
    {
        let pending = PENDING_PRE_TASK_CLARIFICATION.lock().unwrap().clone();
        if let Some((pending_chat_id, original_prompt)) = pending {
            if pending_chat_id == chat_id {
                *PENDING_PRE_TASK_CLARIFICATION.lock().unwrap() = None;
                info!("[TelegramBot] Got pre-task clarification from chat {}", chat_id);
                let combined = format!("{} — {}", original_prompt, text);
                let _ = tg_send(&token, chat_id, "Got it — starting now...").await;
                run_new_task_and_reply(app_handle, token, chat_id, combined).await;
                return;
            }
        }
    }

    // Mid-task clarification: engine paused waiting for user input.
    {
        let pending = PENDING_CLARIFICATION.lock().unwrap().clone();
        if let Some((pending_chat_id, _)) = pending {
            if pending_chat_id == chat_id {
                info!("[TelegramBot] Routing message as mid-task clarification for chat {}", chat_id);
                match crate::engine::automation_agent_engine::submit_clarification(text) {
                    Ok(_) => {
                        let _ = tg_send(&token, chat_id, "Got it — continuing the task...").await;
                    }
                    Err(e) => {
                        warn!("[TelegramBot] submit_clarification failed: {}", e);
                    }
                }
                return;
            }
        }
    }

    record_message(chat_id, "user", &text);

    match classify(&app_handle, &text, chat_id).await {
        Intent::Help => {
            let reply = "Linefox commands:\n\
                /list — show saved tasks\n\
                /status — check if a task is running\n\
                /stop — stop the current task\n\n\
                Or just describe a task in plain language and I'll run it.";
            record_message(chat_id, "assistant", reply);
            let _ = tg_send(&token, chat_id, reply).await;
        }

        Intent::List => {
            send_automation_list(&app_handle, &token, chat_id).await;
        }

        Intent::Status => {
            let running = crate::engine::automation_agent_engine::is_running();
            let reply = if running {
                "A task is currently running."
            } else {
                "Linefox is idle — no task running."
            };
            record_message(chat_id, "assistant", reply);
            let _ = tg_send(&token, chat_id, reply).await;
        }

        Intent::Stop => {
            let _ = crate::engine::automation_agent_engine::stop_execution();
            crate::engine::automation_agent_engine::finalize_execution_run(
                &app_handle, "stopped", None,
            );
            record_message(chat_id, "assistant", "Task stopped.");
            let _ = tg_send(&token, chat_id, "Task stopped.").await;
        }

        Intent::ContinueTask(auto_id, auto_name) => {
            let ack = format!("Got it — continuing \"{}\"...", auto_name);
            record_message(chat_id, "assistant", &ack);
            let _ = tg_send(&token, chat_id, &ack).await;
            run_saved_with_instructions_and_reply(app_handle, token, chat_id, auto_id, text).await;
        }

        Intent::QuickReply(reply) => {
            record_message(chat_id, "assistant", &reply);
            let _ = tg_send(&token, chat_id, &reply).await;
        }

        Intent::NeedsClarification(question) => {
            record_message(chat_id, "assistant", &question);
            *PENDING_PRE_TASK_CLARIFICATION.lock().unwrap() = Some((chat_id, text));
            let _ = tg_send(&token, chat_id, &question).await;
        }

        Intent::NewTask => {
            record_message(chat_id, "assistant", "Got it — starting now...");
            let _ = tg_send(&token, chat_id, "Got it — starting now...").await;
            run_new_task_and_reply(app_handle, token, chat_id, text).await;
        }

        Intent::ScheduledTask(info) => {
            let ack = format!("Got it — running it now and scheduling: {}.", info.label);
            record_message(chat_id, "assistant", &ack);
            let _ = tg_send(&token, chat_id, &ack).await;
            run_and_schedule_task_and_reply(app_handle, token, chat_id, text, info).await;
        }
    }
}

// ===== Intent classification =====

#[derive(Debug)]
struct ScheduleInfo {
    recurrence_type: String,   // "daily" | "weekdays" | "weekly"
    execution_hour: i32,       // 0-23
    recurrence_days: Option<Vec<i32>>, // only for "weekly": [0=Sun..6=Sat]
    label: String,             // human-readable, e.g. "Daily at 9am"
}

#[derive(Debug)]
enum Intent {
    Help,
    List,
    Status,
    Stop,
    ContinueTask(i64, String),      // LLM: follow-up on the most-recent automation
    QuickReply(String),             // LLM: conversational message, no execution
    NeedsClarification(String),     // LLM: task too vague — question to ask first
    NewTask,
    ScheduledTask(ScheduleInfo),    // LLM: new task with a recurring schedule
}

async fn classify(app_handle: &AppHandle, text: &str, chat_id: i64) -> Intent {
    let lower = text.to_lowercase();
    let trimmed = lower.trim();

    // Fast-path: explicit bot commands (no LLM cost)
    if matches!(trimmed, "/start" | "/help" | "help") {
        return Intent::Help;
    }
    if matches!(trimmed, "/list" | "list") {
        return Intent::List;
    }
    if matches!(trimmed, "/status" | "status") {
        return Intent::Status;
    }
    if matches!(trimmed, "/stop" | "stop") {
        return Intent::Stop;
    }

    // LLM-based classification for everything else
    llm_classify(app_handle, text, chat_id).await
}

async fn llm_classify(app_handle: &AppHandle, text: &str, chat_id: i64) -> Intent {
    let api_choice = app_handle
        .db(|db| settings_repository::get_setting(db, "api_choice"))
        .map(|s| s.setting_value)
        .unwrap_or_default();

    info!("[TelegramBot] classify: api_choice={}", api_choice);

    let api_key = match get_classify_api_key(app_handle, &api_choice) {
        Some(k) if !k.is_empty() => k,
        _ => {
            warn!("[TelegramBot] classify: no API key for '{}' — classification disabled, treating as new_task. \
                   Set an API key in Linefox settings.", api_choice);
            return Intent::NewTask;
        }
    };

    let task_context = build_task_context(app_handle);
    let chat_history = get_chat_history(chat_id);

    let system_prompt = "You are a routing classifier for a desktop automation assistant. \
        Given the conversation history, recent automation tasks, and the new message, decide what to do.\n\n\
        RESPOND WITH ONLY valid JSON, no markdown fences.\n\n\
        Four possible intents:\n\n\
        1. \"quick_reply\" — Use for anything that does NOT require touching the user's computer:\n\
           - Greetings, thanks, farewells\n\
           - Jokes, riddles, trivia, general knowledge, follow-up chat (\"another one\", \"tell me more\")\n\
           - Questions about the assistant or what it can do\n\
           - Anything answerable from memory without a desktop action\n\
           Use the conversation history to recognise conversational follow-ups.\n\
           Format: {\"intent\":\"quick_reply\",\"reply\":\"<your response>\",\"confidence\":0.9}\n\n\
        2. \"new_task\" — The user wants the agent to DO something on their computer:\n\
           open apps, navigate websites, send emails, create files, run scripts, etc.\n\
           Basic format: {\"intent\":\"new_task\",\"confidence\":0.9}\n\
           SCHEDULE DETECTION: If the message includes recurring language (\"every day\", \"every morning\",\n\
           \"daily\", \"every weekday\", \"every Monday\", \"each week\", \"weekly\", \"every hour\", etc.),\n\
           add a \"schedule\" field:\n\
           {\"intent\":\"new_task\",\"schedule\":{\"recurrence_type\":\"daily\",\"execution_hour\":9,\"label\":\"Daily at 9am\"},\"confidence\":0.9}\n\
           recurrence_type must be one of: \"daily\" | \"weekdays\" | \"weekly\"\n\
           execution_hour: 0-23 (infer from context, default 9 if not specified)\n\
           For \"weekly\" only, add \"recurrence_days\":[1,3,5] (0=Sun,1=Mon,...,6=Sat)\n\
           label: short human-readable description of the schedule (e.g. \"Weekdays at 8am\", \"Mon/Wed/Fri at 10am\")\n\n\
        3. \"continue_task\" — The user wants to RE-RUN or MODIFY a previous DESKTOP AUTOMATION task.\n\
           ONLY use when a recent automation task exists AND the user clearly wants to repeat/tweak it.\n\
           NEVER use when the prior conversation was chat/jokes/conversation.\n\
           Format: {\"intent\":\"continue_task\",\"confidence\":0.9}\n\n\
        4. \"needs_clarification\" — It is clearly a desktop task but a critical detail is missing.\n\
           Format: {\"intent\":\"needs_clarification\",\"question\":\"<specific question>\",\"confidence\":0.9}\n\n\
        Rules:\n\
        - If unsure whether it needs the desktop, prefer quick_reply over new_task.\n\
        - needs_clarification ONLY if a genuinely critical detail is missing (who, what, where).\n\
        - continue_task ONLY for desktop task repeats/modifications, never for chat continuations.\n\
        - Use the conversation history first — if the last exchange was chat, treat follow-ups as chat.\n\
        - When schedule is detected, ALWAYS include the schedule field — do not strip it.";

    let user_msg = format!(
        "Conversation history:\n{}\n\nRecent automation tasks:\n{}\n\nNew message: \"{}\"",
        chat_history, task_context, text
    );

    let result = match api_choice.as_str() {
        "claude"   => crate::engine::llm_providers::claude::call_llm_api(&api_key, user_msg, system_prompt, 300).await,
        "openai"   => crate::engine::llm_providers::openai::call_llm_api(&api_key, user_msg, system_prompt, 300).await,
        "grok"     => crate::engine::llm_providers::grok::call_llm_api(&api_key, user_msg, system_prompt, 300).await,
        "gemini"   => crate::engine::llm_providers::gemini::call_llm_api(&api_key, user_msg, system_prompt, 300).await,
        "deepseek" => crate::engine::llm_providers::deepseek::call_llm_api(&api_key, user_msg, system_prompt, 300).await,
        _          => return Intent::NewTask,
    };

    match result {
        Ok((response, _, _)) => {
            info!("[TelegramBot] classify raw response: {}", &response[..response.len().min(200)]);
            parse_llm_intent(app_handle, &response, text)
        }
        Err(e) => {
            warn!("[TelegramBot] LLM classify call failed ({}): {} — defaulting to new_task", api_choice, e);
            Intent::NewTask
        }
    }
}

fn get_classify_api_key(app_handle: &AppHandle, api_choice: &str) -> Option<String> {
    let key_name = match api_choice {
        "claude"   => "api_key_claude",
        "openai"   => "api_key_open_ai",
        "grok"     => "api_key_grok",
        "gemini"   => "api_key_gemini",
        "deepseek" => "api_key_deepseek",
        _ => return None,
    };
    app_handle
        .db(|db| settings_repository::get_setting(db, key_name))
        .ok()
        .map(|s| s.setting_value)
        .filter(|s| !s.is_empty())
}

fn build_task_context(app_handle: &AppHandle) -> String {
    let runs = app_handle
        .db(|db| automation_execution_repository::get_recent_execution_runs_with_steps(db, 3))
        .unwrap_or_default();

    if runs.is_empty() {
        return "(No previous tasks)".to_string();
    }

    let automations: std::collections::HashMap<i64, String> = app_handle
        .db(|db| automation_repository::get_all_automations(db))
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.id, a.name))
        .collect();

    runs.iter().map(|r| {
        let name = automations
            .get(&r.run.automation_id)
            .cloned()
            .unwrap_or_else(|| format!("Task #{}", r.run.automation_id));
        let completion = r.run.completion_message
            .as_deref()
            .map(|m| format!(" — Result: \"{}\"", &m[..m.len().min(120)]))
            .unwrap_or_default();
        format!("- \"{}\" ({}){}",  name, r.run.status, completion)
    }).collect::<Vec<_>>().join("\n")
}

fn parse_llm_intent(app_handle: &AppHandle, response: &str, original_text: &str) -> Intent {
    let cleaned = response.replace("```json", "").replace("```", "");
    let trimmed = cleaned.trim();
    let start = trimmed.find('{').unwrap_or(0);
    let end = trimmed.rfind('}').map(|i| i + 1).unwrap_or(trimmed.len());
    let json_str = &trimmed[start..end];

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Intent::NewTask,
    };

    match parsed["intent"].as_str() {
        Some("quick_reply") => {
            let reply = parsed["reply"]
                .as_str()
                .unwrap_or("Hey! What would you like me to do?")
                .to_string();
            Intent::QuickReply(reply)
        }
        Some("needs_clarification") => {
            let question = parsed["question"]
                .as_str()
                .unwrap_or("Could you provide more details?")
                .to_string();
            Intent::NeedsClarification(question)
        }
        Some("continue_task") => {
            if let Ok(runs) = app_handle.db(|db| automation_execution_repository::get_recent_execution_runs_with_steps(db, 1)) {
                if let Some(run) = runs.first() {
                    let auto_id = run.run.automation_id;
                    let auto_name = app_handle
                        .db(|db| automation_repository::get_automation_by_id(db, auto_id))
                        .ok()
                        .flatten()
                        .map(|a| a.name)
                        .unwrap_or_else(|| format!("Task #{}", auto_id));
                    return Intent::ContinueTask(auto_id, auto_name);
                }
            }
            Intent::NewTask
        }
        Some("new_task") => {
            // Check if the LLM detected a recurring schedule
            if let Some(sched) = parsed.get("schedule").filter(|s| s.is_object()) {
                let recurrence_type = sched["recurrence_type"]
                    .as_str()
                    .unwrap_or("daily")
                    .to_string();
                let execution_hour = sched["execution_hour"]
                    .as_i64()
                    .unwrap_or(9)
                    .clamp(0, 23) as i32;
                let recurrence_days = sched["recurrence_days"].as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_i64().map(|d| d.clamp(0, 6) as i32))
                        .collect::<Vec<i32>>()
                });
                let label = sched["label"]
                    .as_str()
                    .unwrap_or(&recurrence_type)
                    .to_string();
                return Intent::ScheduledTask(ScheduleInfo {
                    recurrence_type,
                    execution_hour,
                    recurrence_days,
                    label,
                });
            }
            Intent::NewTask
        }
        _ => Intent::NewTask,
    }
}


// ===== Execution helpers =====


async fn run_saved_with_instructions_and_reply(
    app_handle: AppHandle,
    token: String,
    chat_id: i64,
    auto_id: i64,
    instructions: String,
) {
    *PENDING_CLARIFICATION.lock().unwrap() = Some((chat_id, token.clone()));
    let result = crate::engine::automation_agent_engine::execute_automation(
        &app_handle, auto_id, Some(instructions),
    ).await;
    *PENDING_CLARIFICATION.lock().unwrap() = None;

    match result {
        Ok(_) => {
            let msg = build_completion_reply(&app_handle, auto_id).await;
            let _ = tg_send(&token, chat_id, &msg).await;
        }
        Err(e) => {
            error!("[TelegramBot] ContinueTask {} failed: {}", auto_id, e);
            let _ = tg_send(&token, chat_id, &format!("Task failed: {}", e)).await;
        }
    }
}

async fn run_new_task_and_reply(app_handle: AppHandle, token: String, chat_id: i64, task: String) {
    let initial_name = task
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join(" ");

    let request = crate::engine::synthetic_automation::SyntheticGenerationRequest {
        prompt: task.clone(),
        name: initial_name,
        api_key: None,
        api_choice: None,
    };

    match crate::engine::synthetic_automation::generate_synthetic_automation(&app_handle, request).await {
        Ok(gen) if gen.success => {
            match gen.automation_id {
                Some(auto_id) => {
                    // Register this chat as the clarification target for the duration of the run
                    *PENDING_CLARIFICATION.lock().unwrap() = Some((chat_id, token.clone()));

                    let result = crate::engine::automation_agent_engine::execute_automation(
                        &app_handle, auto_id, None,
                    ).await;

                    // Always clear clarification context when done
                    *PENDING_CLARIFICATION.lock().unwrap() = None;

                    match result {
                        Ok(_) => {
                            let msg = build_completion_reply(&app_handle, auto_id).await;
                            let _ = tg_send(&token, chat_id, &msg).await;
                        }
                        Err(e) => {
                            error!("[TelegramBot] New task execution failed: {}", e);
                            let _ = tg_send(&token, chat_id, &format!("Task failed: {}", e)).await;
                        }
                    }
                }
                None => {
                    let _ = tg_send(&token, chat_id, "Task was created but couldn't be started.").await;
                }
            }
        }
        Ok(gen) => {
            let err = gen.error.unwrap_or_else(|| "Unknown error".to_string());
            warn!("[TelegramBot] Synthetic generation failed: {}", err);
            let _ = tg_send(&token, chat_id, &format!("Couldn't create task: {}", err)).await;
        }
        Err(e) => {
            error!("[TelegramBot] generate_synthetic_automation error: {}", e);
            let _ = tg_send(&token, chat_id, &format!("Task failed: {}", e)).await;
        }
    }
}

async fn run_and_schedule_task_and_reply(
    app_handle: AppHandle,
    token: String,
    chat_id: i64,
    task: String,
    schedule: ScheduleInfo,
) {
    let initial_name = task
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join(" ");

    let request = crate::engine::synthetic_automation::SyntheticGenerationRequest {
        prompt: task.clone(),
        name: initial_name,
        api_key: None,
        api_choice: None,
    };

    match crate::engine::synthetic_automation::generate_synthetic_automation(&app_handle, request).await {
        Ok(gen) if gen.success => {
            match gen.automation_id {
                Some(auto_id) => {
                    // Create the recurring schedule before running
                    let schedule_result = app_handle.db(|db| {
                        crate::repository::schedule_repository::create_schedule(
                            db,
                            auto_id,
                            &schedule.recurrence_type,
                            schedule.recurrence_days.clone(),
                            schedule.execution_hour,
                            0,    // execution_minute
                            "Local", // timezone — matches local_scheduler
                            None, // persistent_run_id
                            None, // continuation_prompt
                        )
                    });
                    match schedule_result {
                        Ok(schedule_id) => {
                            info!("[TelegramBot] Created schedule {} for automation {} ({})", schedule_id, auto_id, schedule.label);
                            // Notify the frontend so the schedule filter refreshes immediately
                            let _ = app_handle.emit("schedule_changed", serde_json::json!({ "action": "created" }));
                        }
                        Err(e) => {
                            warn!("[TelegramBot] Schedule creation failed for automation {}: {}", auto_id, e);
                            let _ = tg_send(&token, chat_id, "⚠️ Task created but scheduling failed — it will run once now.").await;
                        }
                    }

                    // Run immediately
                    *PENDING_CLARIFICATION.lock().unwrap() = Some((chat_id, token.clone()));

                    let result = crate::engine::automation_agent_engine::execute_automation(
                        &app_handle, auto_id, None,
                    ).await;

                    *PENDING_CLARIFICATION.lock().unwrap() = None;

                    match result {
                        Ok(_) => {
                            let completion = build_completion_reply(&app_handle, auto_id).await;
                            let msg = format!("{}\n\n✅ Scheduled: {}", completion, schedule.label);
                            let _ = tg_send(&token, chat_id, &msg).await;
                        }
                        Err(e) => {
                            error!("[TelegramBot] Scheduled task execution failed: {}", e);
                            let _ = tg_send(&token, chat_id, &format!("Task failed: {}\n\nSchedule is still active: {}", e, schedule.label)).await;
                        }
                    }
                }
                None => {
                    let _ = tg_send(&token, chat_id, "Task was created but couldn't be started.").await;
                }
            }
        }
        Ok(gen) => {
            let err = gen.error.unwrap_or_else(|| "Unknown error".to_string());
            warn!("[TelegramBot] Scheduled task generation failed: {}", err);
            let _ = tg_send(&token, chat_id, &format!("Couldn't create task: {}", err)).await;
        }
        Err(e) => {
            error!("[TelegramBot] generate_synthetic_automation error: {}", e);
            let _ = tg_send(&token, chat_id, &format!("Task failed: {}", e)).await;
        }
    }
}

/// Strip common markdown formatting for plain-text Telegram messages.
fn strip_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        // Strip heading markers: "# Heading" → "Heading"
        let line = if line.starts_with('#') {
            line.trim_start_matches('#').trim_start()
        } else {
            line
        };
        out.push_str(line);
        out.push('\n');
    }
    // Bold: **text** or __text__
    let out = out.replace("**", "");
    let out = out.replace("__", "");
    // Inline code
    let out = out.replace('`', "");
    // Links: [text](url) → text (url)
    let mut result = String::with_capacity(out.len());
    let mut chars = out.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '[' {
            let mut link_text = String::new();
            let mut found_close = false;
            for c in chars.by_ref() {
                if c == ']' {
                    found_close = true;
                    break;
                }
                link_text.push(c);
            }
            if found_close && chars.peek() == Some(&'(') {
                chars.next(); // consume '('
                let mut url = String::new();
                for c in chars.by_ref() {
                    if c == ')' { break; }
                    url.push(c);
                }
                result.push_str(&link_text);
                result.push_str(" (");
                result.push_str(&url);
                result.push(')');
            } else {
                result.push('[');
                result.push_str(&link_text);
                if found_close { result.push(']'); }
            }
        } else {
            result.push(ch);
        }
    }
    result.trim_end().to_string()
}

/// Query the DB for a specific automation's most recent run completion message.
async fn build_completion_reply(app_handle: &AppHandle, automation_id: i64) -> String {
    match app_handle.db(|db| automation_execution_repository::get_execution_runs_by_automation(db, automation_id)) {
        Ok(runs) if !runs.is_empty() => {
            let run = &runs[0]; // most recent by started_at DESC
            match run.status.as_str() {
                "completed" => {
                    if let Some(ref msg) = run.completion_message {
                        if !msg.is_empty() {
                            return format!("Done. {}", strip_markdown(msg));
                        }
                    }
                    "Done.".to_string()
                }
                "failed" => {
                    let err = run.error_message.as_deref().unwrap_or("unknown error");
                    format!("Failed: {}", err)
                }
                "stopped" => "Stopped.".to_string(),
                _ => "Done.".to_string(),
            }
        }
        _ => "Done.".to_string(),
    }
}

async fn send_automation_list(app_handle: &AppHandle, token: &str, chat_id: i64) {
    match app_handle.db(|db| automation_repository::get_all_automations(db)) {
        Ok(automations) if !automations.is_empty() => {
            let lines: Vec<String> = automations
                .iter()
                .enumerate()
                .map(|(i, a)| format!("{}. {}", i + 1, a.name))
                .collect();
            let msg = format!(
                "Saved tasks:\n\n{}\n\nSend the name or describe a new task.",
                lines.join("\n")
            );
            let _ = tg_send(token, chat_id, &msg).await;
        }
        _ => {
            let _ = tg_send(token, chat_id, "No saved tasks yet.").await;
        }
    }
}

// ===== Authorization =====

fn is_authorized(app_handle: &AppHandle, user_id: i64) -> bool {
    let allowed_str = app_handle
        .db(|db| settings_repository::get_setting(db, SETTING_ALLOWED_USERS))
        .ok()
        .map(|s| s.setting_value)
        .unwrap_or_default();

    // Empty allowlist = unrestricted (user should lock this down in settings)
    if allowed_str.is_empty() {
        return true;
    }

    allowed_str
        .split(',')
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .any(|id| id == user_id)
}

// ===== Settings helpers =====

fn get_bot_token(app_handle: &AppHandle) -> Option<String> {
    app_handle
        .db(|db| settings_repository::get_setting(db, SETTING_BOT_TOKEN))
        .ok()
        .map(|s| s.setting_value)
        .filter(|s| !s.is_empty())
}

// ===== Telegram HTTP helpers =====

const TG_API_BASE: &str = "https://api.telegram.org/bot";

async fn fetch_updates(token: &str, offset: i64) -> Result<Vec<TgUpdate>, String> {
    let url = format!(
        "{}{}/getUpdates?offset={}&timeout=30",
        TG_API_BASE, token, offset
    );

    let client = reqwest::Client::builder()
        // Must exceed the long-poll timeout (30 s) by a margin
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    let parsed: TgResponse<Vec<TgUpdate>> = resp
        .json()
        .await
        .map_err(|e| format!("JSON parse error: {}", e))?;

    if parsed.ok {
        Ok(parsed.result.unwrap_or_default())
    } else {
        Err("Telegram API returned ok=false — check your bot token".to_string())
    }
}

async fn tg_send(token: &str, chat_id: i64, text: &str) -> Result<(), String> {
    let url = format!("{}{}/sendMessage", TG_API_BASE, token);

    // Telegram has a 4096-char message limit
    let safe_text = if text.len() > 4000 {
        &text[..4000]
    } else {
        text
    };

    let client = reqwest::Client::new();
    client
        .post(&url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": safe_text,
        }))
        .send()
        .await
        .map_err(|e| format!("sendMessage failed: {}", e))?;

    Ok(())
}

// ===== Tauri commands (called from the Settings UI) =====

/// Save the bot token and restart the polling loop.
#[tauri::command]
pub fn set_telegram_bot_token(app_handle: AppHandle, token: String) -> Result<(), String> {
    use crate::entity::setting::Setting;
    app_handle
        .db(|db| settings_repository::insert_or_update_setting(
            db,
            Setting { setting_key: SETTING_BOT_TOKEN.to_string(), setting_value: token },
        ))
        .map_err(|e| e.to_string())?;

    info!("[TelegramBot] Token updated — restarting bot");
    restart_telegram_bot(app_handle);
    Ok(())
}

/// Save the comma-separated allowlist of Telegram user IDs.
#[tauri::command]
pub fn set_telegram_allowed_users(app_handle: AppHandle, user_ids: String) -> Result<(), String> {
    use crate::entity::setting::Setting;
    app_handle
        .db(|db| settings_repository::insert_or_update_setting(
            db,
            Setting {
                setting_key: SETTING_ALLOWED_USERS.to_string(),
                setting_value: user_ids,
            },
        ))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Disconnect the Telegram bot: clear the token and stop the loop.
#[tauri::command]
pub fn disconnect_telegram_bot(app_handle: AppHandle) -> Result<(), String> {
    use crate::entity::setting::Setting;
    app_handle
        .db(|db| settings_repository::insert_or_update_setting(
            db,
            Setting { setting_key: SETTING_BOT_TOKEN.to_string(), setting_value: String::new() },
        ))
        .map_err(|e| e.to_string())?;
    stop_telegram_bot();
    info!("[TelegramBot] Disconnected — token cleared");
    Ok(())
}

/// Get the current Telegram bot configuration (token is masked for security).
#[tauri::command]
pub fn get_telegram_config(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let token = app_handle
        .db(|db| settings_repository::get_setting(db, SETTING_BOT_TOKEN))
        .map(|s| s.setting_value)
        .unwrap_or_default();

    let allowed = app_handle
        .db(|db| settings_repository::get_setting(db, SETTING_ALLOWED_USERS))
        .map(|s| s.setting_value)
        .unwrap_or_default();

    let token_configured = !token.is_empty();
    let token_masked = if token_configured {
        // Show only last 4 chars for verification
        let visible = &token[token.len().saturating_sub(4)..];
        format!("...{}", visible)
    } else {
        String::new()
    };

    Ok(serde_json::json!({
        "token_configured": token_configured,
        "token_masked": token_masked,
        "allowed_users": allowed,
        "bot_running": is_running(),
    }))
}
