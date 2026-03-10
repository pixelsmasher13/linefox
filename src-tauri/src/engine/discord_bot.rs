//! Discord Bot integration for Linefox
//!
//! Connects to the Discord Gateway via WebSocket (using tokio-tungstenite, already a dependency)
//! and sends replies via the Discord REST API (using reqwest, already a dependency).
//!
//! Flow mirrors telegram_bot.rs:
//!   User sends message → classify intent → ack reply → execute automation → send completion message
//!
//!   Clarification flow:
//!   While an automation is running the engine may pause and ask a question (CLARIFICATION action).
//!   The bot forwards that question to Discord. The user's next message in the same channel is treated
//!   as the clarification answer (not as a new task) and is injected back into the engine via
//!   submit_clarification(). The engine then continues.
//!
//! Settings keys (stored in the `settings` SQLite table):
//!   `discord_bot_token`     — Bot token from Discord Developer Portal
//!   `discord_allowed_users` — comma-separated Discord user IDs allowed to trigger automations.
//!                             Leave empty to allow anyone who messages the bot.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use log::{info, warn, error, debug};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Listener, Manager};

use crate::configuration::state::ServiceAccess;
use crate::repository::{
    settings_repository,
    automation_repository,
    automation_execution_repository,
};

// ===== Clarification state =====

lazy_static::lazy_static! {
    /// (channel_id, bot_token) of the run that is currently paused waiting for mid-task clarification.
    static ref PENDING_CLARIFICATION: Arc<Mutex<Option<(String, String)>>> =
        Arc::new(Mutex::new(None));
    /// (channel_id, original_vague_prompt) when bot asked the user to clarify before starting a task.
    static ref PENDING_PRE_TASK_CLARIFICATION: Arc<Mutex<Option<(String, String)>>> =
        Arc::new(Mutex::new(None));
    /// Per-channel conversation history: channel_id → recent (role, content) pairs.
    static ref CHAT_HISTORY: Mutex<HashMap<String, VecDeque<(String, String)>>> =
        Mutex::new(HashMap::new());
}

const MAX_CHAT_HISTORY: usize = 8; // 4 exchanges

fn record_message(channel_id: &str, role: &str, content: &str) {
    let mut history = CHAT_HISTORY.lock().unwrap();
    let msgs = history.entry(channel_id.to_string()).or_insert_with(VecDeque::new);
    msgs.push_back((role.to_string(), content.to_string()));
    if msgs.len() > MAX_CHAT_HISTORY {
        msgs.pop_front();
    }
}

fn get_chat_history(channel_id: &str) -> String {
    let history = CHAT_HISTORY.lock().unwrap();
    match history.get(channel_id) {
        Some(msgs) if !msgs.is_empty() => msgs
            .iter()
            .map(|(role, content)| format!("{}: {}", role, content))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => "(No prior conversation in this session)".to_string(),
    }
}

// ===== Settings keys =====

pub const SETTING_BOT_TOKEN: &str = "discord_bot_token";
pub const SETTING_ALLOWED_USERS: &str = "discord_allowed_users";

// ===== Global state =====

static BOT_RUNNING: AtomicBool = AtomicBool::new(false);

// ===== Discord API types =====

#[derive(Deserialize, Debug)]
struct GatewayInfo {
    url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct GatewayPayload {
    op: u8,
    d: Option<serde_json::Value>,
    s: Option<u64>,
    t: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct DiscordMessage {
    id: String,
    channel_id: String,
    content: String,
    author: DiscordUser,
}

#[derive(Deserialize, Debug, Clone)]
struct DiscordUser {
    id: String,
    username: String,
    bot: Option<bool>,
}

// ===== Public API =====

/// Start the Discord bot gateway loop. Safe to call multiple times — second calls are no-ops.
pub fn start_discord_bot(app_handle: AppHandle) {
    if BOT_RUNNING.swap(true, Ordering::SeqCst) {
        info!("[DiscordBot] Already running — skipping duplicate start");
        return;
    }
    info!("[DiscordBot] Starting gateway loop");

    // Register a one-time listener for clarification_required events.
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
        if let Some((channel_id, token)) = pending {
            tauri::async_runtime::spawn(async move {
                let mut msg = format!("**Question:** {}", question);
                if !reasoning.is_empty() {
                    msg.push_str(&format!("\n\n_{}_", reasoning));
                }
                msg.push_str("\n\nReply to this message to continue the task.");
                let _ = discord_send(&token, &channel_id, &msg).await;
            });
        }
    });

    tauri::async_runtime::spawn(async move {
        gateway_loop(app_handle).await;
    });
}

/// Stop the gateway loop on the next iteration.
pub fn stop_discord_bot() {
    BOT_RUNNING.store(false, Ordering::SeqCst);
    info!("[DiscordBot] Stop requested");
}

/// Restart the bot (e.g. after the token is saved in settings).
pub fn restart_discord_bot(app_handle: AppHandle) {
    stop_discord_bot();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        BOT_RUNNING.store(false, Ordering::SeqCst);
        start_discord_bot(app_handle);
    });
}

pub fn is_running() -> bool {
    BOT_RUNNING.load(Ordering::SeqCst)
}

// ===== Per-channel sequential message queue =====

type ChannelSender = mpsc::UnboundedSender<(AppHandle, String, DiscordMessage)>;

lazy_static::lazy_static! {
    static ref CHANNEL_QUEUES: Mutex<HashMap<String, ChannelSender>> = Mutex::new(HashMap::new());
    static ref CHANNEL_BUSY: Mutex<HashMap<String, bool>> = Mutex::new(HashMap::new());
}

fn is_channel_busy(channel_id: &str) -> bool {
    *CHANNEL_BUSY.lock().unwrap().get(channel_id).unwrap_or(&false)
}

fn set_channel_busy(channel_id: &str, busy: bool) {
    CHANNEL_BUSY.lock().unwrap().insert(channel_id.to_string(), busy);
}

fn get_or_create_channel_queue(channel_id: &str) -> ChannelSender {
    let mut queues = CHANNEL_QUEUES.lock().unwrap();
    if let Some(tx) = queues.get(channel_id) {
        if !tx.is_closed() {
            return tx.clone();
        }
    }
    let ch_id = channel_id.to_string();
    let (tx, mut rx) = mpsc::unbounded_channel::<(AppHandle, String, DiscordMessage)>();
    tokio::spawn(async move {
        while let Some((app, tok, msg)) = rx.recv().await {
            set_channel_busy(&ch_id, true);
            process_message(app, tok, msg).await;
            set_channel_busy(&ch_id, false);
        }
    });
    queues.insert(channel_id.to_string(), tx.clone());
    tx
}

// ===== Gateway loop =====

async fn gateway_loop(app_handle: AppHandle) {
    loop {
        if !BOT_RUNNING.load(Ordering::SeqCst) {
            info!("[DiscordBot] Stopped");
            break;
        }

        let token = match get_bot_token(&app_handle) {
            Some(t) => t,
            None => {
                tokio::time::sleep(std::time::Duration::from_secs(15)).await;
                continue;
            }
        };

        // Get gateway URL
        let gateway_url = match get_gateway_url(&token).await {
            Ok(url) => url,
            Err(e) => {
                warn!("[DiscordBot] Failed to get gateway URL: {} — retrying in 30s", e);
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
        };

        info!("[DiscordBot] Connecting to gateway: {}", gateway_url);

        let ws_url = format!("{}/?v=10&encoding=json", gateway_url);
        let connect_result = tokio_tungstenite::connect_async(&ws_url).await;

        let (ws_stream, _) = match connect_result {
            Ok(pair) => pair,
            Err(e) => {
                warn!("[DiscordBot] WebSocket connect failed: {} — retrying in 10s", e);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            }
        };

        info!("[DiscordBot] WebSocket connected");

        use futures::stream::StreamExt;
        use futures::SinkExt;
        use tokio_tungstenite::tungstenite::Message as WsMessage;

        let (mut ws_write, mut ws_read) = ws_stream.split();

        // Read Hello to get heartbeat interval
        let hello = match ws_read.next().await {
            Some(Ok(WsMessage::Text(text))) => {
                match serde_json::from_str::<GatewayPayload>(&text) {
                    Ok(payload) if payload.op == 10 => payload,
                    Ok(payload) => {
                        warn!("[DiscordBot] Expected Hello (op 10), got op {}", payload.op);
                        continue;
                    }
                    Err(e) => {
                        warn!("[DiscordBot] Failed to parse Hello: {}", e);
                        continue;
                    }
                }
            }
            other => {
                warn!("[DiscordBot] Unexpected first message: {:?}", other);
                continue;
            }
        };

        let heartbeat_interval = hello.d
            .as_ref()
            .and_then(|d| d.get("heartbeat_interval"))
            .and_then(|v| v.as_u64())
            .unwrap_or(41250);

        debug!("[DiscordBot] Heartbeat interval: {}ms", heartbeat_interval);

        // Send Identify
        let identify = serde_json::json!({
            "op": 2,
            "d": {
                "token": token,
                "intents": 33281, // GUILDS (1) | GUILD_MESSAGES (512) | MESSAGE_CONTENT (32768) | DIRECT_MESSAGES (4096)
                "properties": {
                    "os": std::env::consts::OS,
                    "browser": "linefox",
                    "device": "linefox"
                }
            }
        });

        if let Err(e) = ws_write.send(WsMessage::Text(identify.to_string().into())).await {
            warn!("[DiscordBot] Failed to send Identify: {}", e);
            continue;
        }

        // Start heartbeat task
        let last_sequence: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
        let hb_seq = last_sequence.clone();
        let hb_running = Arc::new(AtomicBool::new(true));
        let hb_running_clone = hb_running.clone();

        // We need a shared write half for heartbeating
        let ws_write = Arc::new(tokio::sync::Mutex::new(ws_write));
        let hb_write = ws_write.clone();

        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(heartbeat_interval));
            // Skip the first immediate tick
            interval.tick().await;
            loop {
                interval.tick().await;
                if !hb_running_clone.load(Ordering::SeqCst) {
                    break;
                }
                let seq = *hb_seq.lock().unwrap();
                let hb = serde_json::json!({ "op": 1, "d": seq });
                let mut writer = hb_write.lock().await;
                if let Err(e) = writer.send(WsMessage::Text(hb.to_string().into())).await {
                    warn!("[DiscordBot] Heartbeat send failed: {}", e);
                    break;
                }
                debug!("[DiscordBot] Heartbeat sent (seq: {:?})", seq);
            }
        });

        let mut ready = false;

        // Main event loop
        loop {
            if !BOT_RUNNING.load(Ordering::SeqCst) {
                break;
            }

            let msg = tokio::select! {
                msg = ws_read.next() => msg,
                _ = tokio::time::sleep(std::time::Duration::from_secs(90)) => {
                    warn!("[DiscordBot] No message in 90s — reconnecting");
                    break;
                }
            };

            match msg {
                Some(Ok(WsMessage::Text(text))) => {
                    let payload: GatewayPayload = match serde_json::from_str(&text) {
                        Ok(p) => p,
                        Err(e) => {
                            debug!("[DiscordBot] Failed to parse payload: {}", e);
                            continue;
                        }
                    };

                    // Update sequence number
                    if let Some(s) = payload.s {
                        *last_sequence.lock().unwrap() = Some(s);
                    }

                    match payload.op {
                        0 => {
                            // Dispatch event
                            if let Some(ref event_name) = payload.t {
                                match event_name.as_str() {
                                    "READY" => {
                                        ready = true;
                                        info!("[DiscordBot] Gateway READY — bot is online");
                                    }
                                    "MESSAGE_CREATE" => {
                                        if !ready { continue; }
                                        if let Some(ref d) = payload.d {
                                            match serde_json::from_value::<DiscordMessage>(d.clone()) {
                                                Ok(discord_msg) => {
                                                    // Ignore messages from bots (including self)
                                                    if discord_msg.author.bot.unwrap_or(false) {
                                                        continue;
                                                    }
                                                    if discord_msg.content.is_empty() {
                                                        continue;
                                                    }

                                                    let channel_id = discord_msg.channel_id.clone();
                                                    let busy = is_channel_busy(&channel_id);
                                                    let tx = get_or_create_channel_queue(&channel_id);
                                                    if busy {
                                                        let tok2 = token.clone();
                                                        let ch2 = channel_id.clone();
                                                        tokio::spawn(async move {
                                                            let _ = discord_send(&tok2, &ch2, "I'm still working on your previous request. I'll get to this as soon as it's done.").await;
                                                        });
                                                    }
                                                    let _ = tx.send((app_handle.clone(), token.clone(), discord_msg));
                                                }
                                                Err(e) => {
                                                    debug!("[DiscordBot] Failed to parse MESSAGE_CREATE: {}", e);
                                                }
                                            }
                                        }
                                    }
                                    _ => {
                                        debug!("[DiscordBot] Ignoring event: {}", event_name);
                                    }
                                }
                            }
                        }
                        1 => {
                            // Heartbeat request from server — send heartbeat immediately
                            let seq = *last_sequence.lock().unwrap();
                            let hb = serde_json::json!({ "op": 1, "d": seq });
                            let mut writer = ws_write.lock().await;
                            let _ = writer.send(WsMessage::Text(hb.to_string().into())).await;
                        }
                        7 => {
                            // Reconnect requested
                            info!("[DiscordBot] Server requested reconnect");
                            break;
                        }
                        9 => {
                            // Invalid session
                            warn!("[DiscordBot] Invalid session — reconnecting in 5s");
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            break;
                        }
                        11 => {
                            // Heartbeat ACK
                            debug!("[DiscordBot] Heartbeat ACK received");
                        }
                        _ => {
                            debug!("[DiscordBot] Unknown op: {}", payload.op);
                        }
                    }
                }
                Some(Ok(WsMessage::Close(_))) => {
                    info!("[DiscordBot] WebSocket closed — reconnecting");
                    break;
                }
                Some(Err(e)) => {
                    warn!("[DiscordBot] WebSocket error: {} — reconnecting", e);
                    break;
                }
                None => {
                    info!("[DiscordBot] WebSocket stream ended — reconnecting");
                    break;
                }
                _ => {}
            }
        }

        // Cleanup heartbeat
        hb_running.store(false, Ordering::SeqCst);
        heartbeat_handle.abort();

        if BOT_RUNNING.load(Ordering::SeqCst) {
            info!("[DiscordBot] Reconnecting in 5s...");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}

async fn get_gateway_url(token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://discord.com/api/v10/gateway/bot")
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("HTTP error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Discord API returned {} — check your bot token. Body: {}", status, &body[..body.len().min(200)]));
    }

    let info: GatewayInfo = resp.json().await.map_err(|e| format!("JSON parse error: {}", e))?;
    info.url.ok_or_else(|| "No gateway URL in response".to_string())
}

// ===== Message handling =====

async fn process_message(app_handle: AppHandle, token: String, msg: DiscordMessage) {
    let channel_id = msg.channel_id.clone();
    let text = msg.content.clone();
    let user_id = msg.author.id.clone();

    // Allowlist check
    if !is_authorized(&app_handle, &user_id) {
        info!("[DiscordBot] Rejected message from unauthorized user {} ({})", msg.author.username, user_id);
        return;
    }

    info!("[DiscordBot] Message from {} in channel {}: {:?}", msg.author.username, channel_id, &text[..text.len().min(100)]);

    // Strip bot mention prefix if present (e.g. "<@BOT_ID> do something" → "do something")
    let text = strip_bot_mention(&text);

    // Pre-task clarification
    {
        let pending = PENDING_PRE_TASK_CLARIFICATION.lock().unwrap().clone();
        if let Some((pending_channel, original_prompt)) = pending {
            if pending_channel == channel_id {
                *PENDING_PRE_TASK_CLARIFICATION.lock().unwrap() = None;
                info!("[DiscordBot] Got pre-task clarification from channel {}", channel_id);
                let combined = format!("{} — {}", original_prompt, text);
                let _ = discord_send(&token, &channel_id, "Got it — starting now...").await;
                run_new_task_and_reply(app_handle, token, channel_id, combined).await;
                return;
            }
        }
    }

    // Mid-task clarification
    {
        let pending = PENDING_CLARIFICATION.lock().unwrap().clone();
        if let Some((pending_channel, _)) = pending {
            if pending_channel == channel_id {
                info!("[DiscordBot] Routing message as mid-task clarification for channel {}", channel_id);
                match crate::engine::automation_agent_engine::submit_clarification(text.to_string()) {
                    Ok(_) => {
                        let _ = discord_send(&token, &channel_id, "Got it — continuing the task...").await;
                    }
                    Err(e) => {
                        warn!("[DiscordBot] submit_clarification failed: {}", e);
                    }
                }
                return;
            }
        }
    }

    record_message(&channel_id, "user", &text);

    match classify(&app_handle, &text, &channel_id).await {
        Intent::Help => {
            let reply = "**Linefox commands:**\n\
                `/list` — show saved tasks\n\
                `/status` — check if a task is running\n\
                `/stop` — stop the current task\n\n\
                Or just describe a task in plain language and I'll run it.";
            record_message(&channel_id, "assistant", reply);
            let _ = discord_send(&token, &channel_id, reply).await;
        }

        Intent::List => {
            send_automation_list(&app_handle, &token, &channel_id).await;
        }

        Intent::Status => {
            let running = crate::engine::automation_agent_engine::is_running();
            let reply = if running {
                "A task is currently running."
            } else {
                "Linefox is idle — no task running."
            };
            record_message(&channel_id, "assistant", reply);
            let _ = discord_send(&token, &channel_id, reply).await;
        }

        Intent::Stop => {
            let _ = crate::engine::automation_agent_engine::stop_execution();
            crate::engine::automation_agent_engine::finalize_execution_run(
                &app_handle, "stopped", None,
            );
            record_message(&channel_id, "assistant", "Task stopped.");
            let _ = discord_send(&token, &channel_id, "Task stopped.").await;
        }

        Intent::ContinueTask(auto_id, auto_name) => {
            let ack = format!("Got it — continuing \"{}\"...", auto_name);
            record_message(&channel_id, "assistant", &ack);
            let _ = discord_send(&token, &channel_id, &ack).await;
            run_saved_with_instructions_and_reply(app_handle, token, channel_id, auto_id, text.to_string()).await;
        }

        Intent::QuickReply(reply) => {
            record_message(&channel_id, "assistant", &reply);
            let _ = discord_send(&token, &channel_id, &reply).await;
        }

        Intent::NeedsClarification(question) => {
            record_message(&channel_id, "assistant", &question);
            *PENDING_PRE_TASK_CLARIFICATION.lock().unwrap() = Some((channel_id.clone(), text.to_string()));
            let _ = discord_send(&token, &channel_id, &question).await;
        }

        Intent::NewTask => {
            record_message(&channel_id, "assistant", "Got it — starting now...");
            let _ = discord_send(&token, &channel_id, "Got it — starting now...").await;
            run_new_task_and_reply(app_handle, token, channel_id, text.to_string()).await;
        }

        Intent::ScheduledTask(info) => {
            let ack = format!("Got it — running it now and scheduling: {}.", info.label);
            record_message(&channel_id, "assistant", &ack);
            let _ = discord_send(&token, &channel_id, &ack).await;
            run_and_schedule_task_and_reply(app_handle, token, channel_id, text.to_string(), info).await;
        }
    }
}

/// Strip a leading bot mention like "<@1234567890> " from the message
fn strip_bot_mention(text: &str) -> &str {
    let trimmed = text.trim();
    if trimmed.starts_with("<@") {
        if let Some(end) = trimmed.find('>') {
            return trimmed[end + 1..].trim_start();
        }
    }
    trimmed
}

// ===== Intent classification =====

#[derive(Debug)]
struct ScheduleInfo {
    recurrence_type: String,
    execution_hour: i32,
    recurrence_days: Option<Vec<i32>>,
    label: String,
}

#[derive(Debug)]
enum Intent {
    Help,
    List,
    Status,
    Stop,
    ContinueTask(i64, String),
    QuickReply(String),
    NeedsClarification(String),
    NewTask,
    ScheduledTask(ScheduleInfo),
}

async fn classify(app_handle: &AppHandle, text: &str, channel_id: &str) -> Intent {
    let lower = text.to_lowercase();
    let trimmed = lower.trim();

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

    llm_classify(app_handle, text, channel_id).await
}

async fn llm_classify(app_handle: &AppHandle, text: &str, channel_id: &str) -> Intent {
    let api_choice = app_handle
        .db(|db| settings_repository::get_setting(db, "api_choice"))
        .map(|s| s.setting_value)
        .unwrap_or_default();

    let api_key = match get_classify_api_key(app_handle, &api_choice) {
        Some(k) if !k.is_empty() => k,
        _ => {
            warn!("[DiscordBot] classify: no API key for '{}' — treating as new_task", api_choice);
            return Intent::NewTask;
        }
    };

    let task_context = build_task_context(app_handle);
    let chat_history = get_chat_history(channel_id);

    // Same classification prompt as Telegram
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
            info!("[DiscordBot] classify raw response: {}", &response[..response.len().min(200)]);
            parse_llm_intent(app_handle, &response, text)
        }
        Err(e) => {
            warn!("[DiscordBot] LLM classify call failed ({}): {} — defaulting to new_task", api_choice, e);
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

    let automations: HashMap<i64, String> = app_handle
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
        format!("- \"{}\" ({}){}", name, r.run.status, completion)
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
    channel_id: String,
    auto_id: i64,
    instructions: String,
) {
    *PENDING_CLARIFICATION.lock().unwrap() = Some((channel_id.clone(), token.clone()));
    let result = crate::engine::automation_agent_engine::execute_automation(
        &app_handle, auto_id, Some(instructions),
    ).await;
    *PENDING_CLARIFICATION.lock().unwrap() = None;

    match result {
        Ok(_) => {
            let msg = build_completion_reply(&app_handle, auto_id).await;
            let _ = discord_send(&token, &channel_id, &msg).await;
        }
        Err(e) => {
            error!("[DiscordBot] ContinueTask {} failed: {}", auto_id, e);
            let _ = discord_send(&token, &channel_id, &format!("Task failed: {}", e)).await;
        }
    }
}

async fn run_new_task_and_reply(app_handle: AppHandle, token: String, channel_id: String, task: String) {
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
                    *PENDING_CLARIFICATION.lock().unwrap() = Some((channel_id.clone(), token.clone()));

                    let result = crate::engine::automation_agent_engine::execute_automation(
                        &app_handle, auto_id, None,
                    ).await;

                    *PENDING_CLARIFICATION.lock().unwrap() = None;

                    match result {
                        Ok(_) => {
                            let msg = build_completion_reply(&app_handle, auto_id).await;
                            let _ = discord_send(&token, &channel_id, &msg).await;
                        }
                        Err(e) => {
                            error!("[DiscordBot] New task execution failed: {}", e);
                            let _ = discord_send(&token, &channel_id, &format!("Task failed: {}", e)).await;
                        }
                    }
                }
                None => {
                    let _ = discord_send(&token, &channel_id, "Task was created but couldn't be started.").await;
                }
            }
        }
        Ok(gen) => {
            let err = gen.error.unwrap_or_else(|| "Unknown error".to_string());
            warn!("[DiscordBot] Synthetic generation failed: {}", err);
            let _ = discord_send(&token, &channel_id, &format!("Couldn't create task: {}", err)).await;
        }
        Err(e) => {
            error!("[DiscordBot] generate_synthetic_automation error: {}", e);
            let _ = discord_send(&token, &channel_id, &format!("Task failed: {}", e)).await;
        }
    }
}

async fn run_and_schedule_task_and_reply(
    app_handle: AppHandle,
    token: String,
    channel_id: String,
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
                    let schedule_result = app_handle.db(|db| {
                        crate::repository::schedule_repository::create_schedule(
                            db,
                            auto_id,
                            &schedule.recurrence_type,
                            schedule.recurrence_days.clone(),
                            schedule.execution_hour,
                            0,
                            "Local",
                            None,
                            None,
                        )
                    });
                    match schedule_result {
                        Ok(schedule_id) => {
                            info!("[DiscordBot] Created schedule {} for automation {} ({})", schedule_id, auto_id, schedule.label);
                            let _ = app_handle.emit("schedule_changed", serde_json::json!({ "action": "created" }));
                        }
                        Err(e) => {
                            warn!("[DiscordBot] Schedule creation failed for automation {}: {}", auto_id, e);
                            let _ = discord_send(&token, &channel_id, "Task created but scheduling failed — it will run once now.").await;
                        }
                    }

                    *PENDING_CLARIFICATION.lock().unwrap() = Some((channel_id.clone(), token.clone()));

                    let result = crate::engine::automation_agent_engine::execute_automation(
                        &app_handle, auto_id, None,
                    ).await;

                    *PENDING_CLARIFICATION.lock().unwrap() = None;

                    match result {
                        Ok(_) => {
                            let completion = build_completion_reply(&app_handle, auto_id).await;
                            let msg = format!("{}\n\nScheduled: {}", completion, schedule.label);
                            let _ = discord_send(&token, &channel_id, &msg).await;
                        }
                        Err(e) => {
                            error!("[DiscordBot] Scheduled task execution failed: {}", e);
                            let _ = discord_send(&token, &channel_id, &format!("Task failed: {}\n\nSchedule is still active: {}", e, schedule.label)).await;
                        }
                    }
                }
                None => {
                    let _ = discord_send(&token, &channel_id, "Task was created but couldn't be started.").await;
                }
            }
        }
        Ok(gen) => {
            let err = gen.error.unwrap_or_else(|| "Unknown error".to_string());
            warn!("[DiscordBot] Scheduled task generation failed: {}", err);
            let _ = discord_send(&token, &channel_id, &format!("Couldn't create task: {}", err)).await;
        }
        Err(e) => {
            error!("[DiscordBot] generate_synthetic_automation error: {}", e);
            let _ = discord_send(&token, &channel_id, &format!("Task failed: {}", e)).await;
        }
    }
}

/// Query the DB for a specific automation's most recent run completion message.
async fn build_completion_reply(app_handle: &AppHandle, automation_id: i64) -> String {
    match app_handle.db(|db| automation_execution_repository::get_execution_runs_by_automation(db, automation_id)) {
        Ok(runs) if !runs.is_empty() => {
            let run = &runs[0];
            match run.status.as_str() {
                "completed" => {
                    if let Some(ref msg) = run.completion_message {
                        if !msg.is_empty() {
                            return format!("Done. {}", msg);
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

async fn send_automation_list(app_handle: &AppHandle, token: &str, channel_id: &str) {
    match app_handle.db(|db| automation_repository::get_all_automations(db)) {
        Ok(automations) if !automations.is_empty() => {
            let lines: Vec<String> = automations
                .iter()
                .enumerate()
                .map(|(i, a)| format!("{}. {}", i + 1, a.name))
                .collect();
            let msg = format!(
                "**Saved tasks:**\n\n{}\n\nSend the name or describe a new task.",
                lines.join("\n")
            );
            let _ = discord_send(token, channel_id, &msg).await;
        }
        _ => {
            let _ = discord_send(token, channel_id, "No saved tasks yet.").await;
        }
    }
}

// ===== Authorization =====

fn is_authorized(app_handle: &AppHandle, user_id: &str) -> bool {
    let allowed_str = app_handle
        .db(|db| settings_repository::get_setting(db, SETTING_ALLOWED_USERS))
        .ok()
        .map(|s| s.setting_value)
        .unwrap_or_default();

    if allowed_str.is_empty() {
        return true;
    }

    allowed_str
        .split(',')
        .any(|s| s.trim() == user_id)
}

// ===== Settings helpers =====

fn get_bot_token(app_handle: &AppHandle) -> Option<String> {
    app_handle
        .db(|db| settings_repository::get_setting(db, SETTING_BOT_TOKEN))
        .ok()
        .map(|s| s.setting_value)
        .filter(|s| !s.is_empty())
}

// ===== Discord REST API helpers =====

const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

async fn discord_send(token: &str, channel_id: &str, text: &str) -> Result<(), String> {
    let url = format!("{}/channels/{}/messages", DISCORD_API_BASE, channel_id);

    // Discord has a 2000-char message limit
    let safe_text = if text.len() > 1900 {
        &text[..1900]
    } else {
        text
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bot {}", token))
        .json(&serde_json::json!({
            "content": safe_text,
        }))
        .send()
        .await
        .map_err(|e| format!("sendMessage failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!("[DiscordBot] Send message failed ({}): {}", status, &body[..body.len().min(200)]);
    }

    Ok(())
}

// ===== Tauri commands (called from the Settings UI) =====

/// Save the bot token and restart the gateway loop.
#[tauri::command]
pub fn set_discord_bot_token(app_handle: AppHandle, token: String) -> Result<(), String> {
    use crate::entity::setting::Setting;
    app_handle
        .db(|db| settings_repository::insert_or_update_setting(
            db,
            Setting { setting_key: SETTING_BOT_TOKEN.to_string(), setting_value: token },
        ))
        .map_err(|e| e.to_string())?;

    info!("[DiscordBot] Token updated — restarting bot");
    restart_discord_bot(app_handle);
    Ok(())
}

/// Save the comma-separated allowlist of Discord user IDs.
#[tauri::command]
pub fn set_discord_allowed_users(app_handle: AppHandle, user_ids: String) -> Result<(), String> {
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

/// Disconnect the Discord bot: clear the token and stop the gateway loop.
#[tauri::command]
pub fn disconnect_discord_bot(app_handle: AppHandle) -> Result<(), String> {
    use crate::entity::setting::Setting;
    app_handle
        .db(|db| settings_repository::insert_or_update_setting(
            db,
            Setting { setting_key: SETTING_BOT_TOKEN.to_string(), setting_value: String::new() },
        ))
        .map_err(|e| e.to_string())?;
    stop_discord_bot();
    info!("[DiscordBot] Disconnected — token cleared");
    Ok(())
}

/// Get the current Discord bot configuration (token is masked for security).
#[tauri::command]
pub fn get_discord_config(app_handle: AppHandle) -> Result<serde_json::Value, String> {
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
