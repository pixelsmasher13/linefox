//! Remote Bridge - WebSocket server for web app <-> desktop agent communication
//! 
//! Inspired by Clawdbot's Telegram pairing pattern. Allows the Heelix web app
//! to connect to the desktop agent via WebSocket, send task commands, and receive
//! status updates.
//!
//! Protocol: JSON messages over WebSocket on port 21890.
//! Authentication: 6-character pairing code displayed in the desktop app UI.

use std::sync::{Arc, Mutex};
use std::net::SocketAddr;

use futures::stream::StreamExt;
use futures::SinkExt;
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

use crate::configuration::state::ServiceAccess;
use crate::repository::{automation_repository, automation_execution_repository};

// Default port for the WebSocket bridge
const DEFAULT_PORT: u16 = 21890;

// Characters used for pairing codes (no ambiguous chars: 0/O, 1/I)
const CODE_CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;

/// Shared bridge state
#[derive(Clone)]
pub struct BridgeState {
    inner: Arc<BridgeStateInner>,
}

struct BridgeStateInner {
    /// Current pairing code
    pairing_code: Mutex<String>,
    /// Whether the bridge server is running
    is_running: Mutex<bool>,
    /// Port the server is running on
    port: Mutex<u16>,
    /// Broadcast channel for sending events to all connected clients
    event_tx: broadcast::Sender<String>,
    /// Number of connected (authenticated) clients
    connected_clients: Mutex<u32>,
}

impl BridgeState {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        BridgeState {
            inner: Arc::new(BridgeStateInner {
                pairing_code: Mutex::new(generate_pairing_code()),
                is_running: Mutex::new(false),
                port: Mutex::new(DEFAULT_PORT),
                event_tx,
                connected_clients: Mutex::new(0),
            }),
        }
    }

    pub fn get_pairing_code(&self) -> String {
        self.inner.pairing_code.lock().unwrap().clone()
    }

    pub fn regenerate_pairing_code(&self) -> String {
        let new_code = generate_pairing_code();
        *self.inner.pairing_code.lock().unwrap() = new_code.clone();
        new_code
    }

    pub fn is_running(&self) -> bool {
        *self.inner.is_running.lock().unwrap()
    }

    pub fn get_port(&self) -> u16 {
        *self.inner.port.lock().unwrap()
    }

    pub fn get_connected_clients(&self) -> u32 {
        *self.inner.connected_clients.lock().unwrap()
    }

    /// Broadcast an event to all connected clients
    pub fn broadcast_event(&self, event: BridgeEvent) {
        if let Ok(json) = serde_json::to_string(&event) {
            let _ = self.inner.event_tx.send(json);
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<String> {
        self.inner.event_tx.subscribe()
    }

    fn increment_clients(&self) {
        *self.inner.connected_clients.lock().unwrap() += 1;
    }

    fn decrement_clients(&self) {
        let mut count = self.inner.connected_clients.lock().unwrap();
        if *count > 0 {
            *count -= 1;
        }
    }

    fn verify_code(&self, code: &str) -> bool {
        let current = self.inner.pairing_code.lock().unwrap();
        current.eq_ignore_ascii_case(code)
    }
}

/// Events sent from desktop to web clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: String,
}

/// Commands received from web clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeCommand {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub command_type: String,
    pub payload: Option<serde_json::Value>,
    pub timestamp: Option<String>,
    // Auth fields (only for auth messages)
    pub pairing_code: Option<String>,
}

impl BridgeEvent {
    pub fn new(event_type: &str, payload: serde_json::Value) -> Self {
        BridgeEvent {
            event_type: event_type.to_string(),
            payload,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Generate a random 6-character pairing code
fn generate_pairing_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..CODE_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..CODE_CHARS.len());
            CODE_CHARS[idx] as char
        })
        .collect()
}

/// Start the WebSocket bridge server
pub async fn start_bridge_server(app_handle: AppHandle, bridge_state: BridgeState) {
    let port = bridge_state.get_port();
    let addr = format!("0.0.0.0:{}", port);

    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => {
            info!("[RemoteBridge] WebSocket server listening on {}", addr);
            *bridge_state.inner.is_running.lock().unwrap() = true;
            // Emit event to frontend that bridge is ready
            let _ = app_handle.emit("remote_bridge_status", serde_json::json!({
                "running": true,
                "port": port,
                "pairing_code": bridge_state.get_pairing_code(),
                "connected_clients": bridge_state.get_connected_clients(),
            }));
            l
        }
        Err(e) => {
            error!("[RemoteBridge] Failed to bind to {}: {}", addr, e);
            let _ = app_handle.emit("remote_bridge_status", serde_json::json!({
                "running": false,
                "error": format!("Failed to bind to port {}: {}", port, e),
            }));
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!("[RemoteBridge] New connection from {}", addr);
                let app = app_handle.clone();
                let state = bridge_state.clone();
                tokio::spawn(handle_connection(stream, addr, app, state));
            }
            Err(e) => {
                warn!("[RemoteBridge] Failed to accept connection: {}", e);
            }
        }
    }
}

/// Handle a single WebSocket connection
async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    app_handle: AppHandle,
    bridge_state: BridgeState,
) {
    // Upgrade TCP to WebSocket
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(ws) => ws,
        Err(e) => {
            warn!("[RemoteBridge] WebSocket handshake failed for {}: {}", addr, e);
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut authenticated = false;
    let mut event_rx = bridge_state.subscribe();

    // Wait for auth message first
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<BridgeCommand>(&text) {
                    Ok(cmd) if cmd.command_type == "auth" => {
                        if let Some(code) = &cmd.pairing_code {
                            if bridge_state.verify_code(code) {
                                authenticated = true;
                                bridge_state.increment_clients();
                                info!("[RemoteBridge] Client {} authenticated successfully", addr);
                                
                                let response = BridgeEvent::new("auth_result", serde_json::json!({
                                    "success": true,
                                }));
                                let _ = ws_sender.send(Message::Text(
                                    serde_json::to_string(&response).unwrap()
                                )).await;

                                // Notify desktop frontend about new connection
                                let _ = app_handle.emit("remote_bridge_client_connected", serde_json::json!({
                                    "addr": addr.to_string(),
                                    "connected_clients": bridge_state.get_connected_clients(),
                                }));
                                // Also emit full status so UI stays in sync
                                let _ = app_handle.emit("remote_bridge_status", serde_json::json!({
                                    "running": true,
                                    "port": bridge_state.get_port(),
                                    "pairing_code": bridge_state.get_pairing_code(),
                                    "connected_clients": bridge_state.get_connected_clients(),
                                }));

                                break;
                            } else {
                                warn!("[RemoteBridge] Invalid pairing code from {}", addr);
                                let response = BridgeEvent::new("auth_result", serde_json::json!({
                                    "success": false,
                                    "error": "Invalid pairing code",
                                }));
                                let _ = ws_sender.send(Message::Text(
                                    serde_json::to_string(&response).unwrap()
                                )).await;
                                return;
                            }
                        }
                    }
                    _ => {
                        warn!("[RemoteBridge] Expected auth message from {}, got: {}", addr, text);
                        return;
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => {
                info!("[RemoteBridge] Connection closed before auth: {}", addr);
                return;
            }
            _ => {}
        }
    }

    if !authenticated {
        return;
    }

    // Now handle authenticated session
    // Two concurrent tasks: receive commands from web client, forward events to web client
    loop {
        tokio::select! {
            // Forward broadcast events to this client
            event = event_rx.recv() => {
                match event {
                    Ok(json) => {
                        if ws_sender.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Missed some events, continue
                    }
                    Err(_) => break,
                }
            }

            // Receive commands from web client
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<BridgeCommand>(&text) {
                            Ok(cmd) => {
                                let response = handle_command(&app_handle, &bridge_state, cmd).await;
                                if let Some(event) = response {
                                    let json = serde_json::to_string(&event).unwrap_or_default();
                                    if ws_sender.send(Message::Text(json)).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("[RemoteBridge] Invalid command from {}: {}", addr, e);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!("[RemoteBridge] Client {} disconnected", addr);
                        break;
                    }
                    Some(Err(e)) => {
                        warn!("[RemoteBridge] Error from {}: {}", addr, e);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Cleanup
    bridge_state.decrement_clients();
    let _ = app_handle.emit("remote_bridge_client_disconnected", serde_json::json!({
        "addr": addr.to_string(),
        "connected_clients": bridge_state.get_connected_clients(),
    }));
    // Also emit full status so UI stays in sync
    let _ = app_handle.emit("remote_bridge_status", serde_json::json!({
        "running": true,
        "port": bridge_state.get_port(),
        "pairing_code": bridge_state.get_pairing_code(),
        "connected_clients": bridge_state.get_connected_clients(),
    }));
    info!("[RemoteBridge] Session ended for {}", addr);
}

/// Handle an incoming command from a web client
async fn handle_command(
    app_handle: &AppHandle,
    bridge_state: &BridgeState,
    cmd: BridgeCommand,
) -> Option<BridgeEvent> {
    match cmd.command_type.as_str() {
        "ping" => {
            Some(BridgeEvent::new("pong", serde_json::json!({})))
        }

        "get_status" => {
            let is_running = crate::engine::automation_agent_engine::is_running();
            Some(BridgeEvent::new("status_update", serde_json::json!({
                "is_running": is_running,
                "status_message": if is_running { "Automation is running" } else { "Idle" },
            })))
        }

        "list_automations" => {
            match app_handle.db(|db| automation_repository::get_all_automations(db)) {
                Ok(automations) => {
                    let list: Vec<serde_json::Value> = automations.iter().map(|a| {
                        serde_json::json!({
                            "id": a.id,
                            "name": a.name,
                            "objective": a.objective,
                            "description": a.nl_description,
                            "created_at": a.created_at,
                            "updated_at": a.updated_at,
                        })
                    }).collect();
                    Some(BridgeEvent::new("automations_list", serde_json::json!({
                        "automations": list,
                    })))
                }
                Err(e) => {
                    error!("[RemoteBridge] Failed to list automations: {}", e);
                    Some(BridgeEvent::new("automations_list", serde_json::json!({
                        "automations": [],
                        "error": e.to_string(),
                    })))
                }
            }
        }

        "get_history" => {
            let limit = cmd.payload
                .as_ref()
                .and_then(|p| p.get("limit"))
                .and_then(|l| l.as_i64())
                .unwrap_or(20) as i32;

            // Get automations first for name lookup
            let automations_map: std::collections::HashMap<i64, String> = app_handle
                .db(|db| automation_repository::get_all_automations(db))
                .unwrap_or_default()
                .into_iter()
                .map(|a| (a.id, a.name))
                .collect();

            match app_handle.db(|db| automation_execution_repository::get_recent_execution_runs_with_steps(db, limit)) {
                Ok(runs) => {
                    let list: Vec<serde_json::Value> = runs.iter().map(|r| {
                        let automation_name = automations_map
                            .get(&r.run.automation_id)
                            .cloned()
                            .unwrap_or_else(|| format!("Automation #{}", r.run.automation_id));
                        serde_json::json!({
                            "id": r.run.id,
                            "automation_id": r.run.automation_id,
                            "automation_name": automation_name,
                            "status": r.run.status,
                            "started_at": r.run.started_at,
                            "completed_at": r.run.completed_at,
                            "completion_message": r.run.completion_message,
                            "error": r.run.error_message,
                        })
                    }).collect();
                    Some(BridgeEvent::new("history_list", serde_json::json!({
                        "history": list,
                    })))
                }
                Err(e) => {
                    error!("[RemoteBridge] Failed to get history: {}", e);
                    Some(BridgeEvent::new("history_list", serde_json::json!({
                        "history": [],
                        "error": e.to_string(),
                    })))
                }
            }
        }

        "execute_task" => {
            let payload = cmd.payload.unwrap_or(serde_json::json!({}));
            let task_description = payload.get("task_description")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let automation_id = payload.get("automation_id")
                .and_then(|id| id.as_i64());

            if task_description.is_empty() && automation_id.is_none() {
                return Some(BridgeEvent::new("task_failed", serde_json::json!({
                    "error": "No task description or automation ID provided",
                })));
            }

            info!("[RemoteBridge] Executing task: {:?} (automation_id: {:?})", task_description, automation_id);

            if let Some(auto_id) = automation_id {
                // Run an existing automation
                let app_clone = app_handle.clone();
                let bridge_clone = bridge_state.clone();
                let task_desc_for_closure = task_description.clone();
                let additional_instructions = if task_description.is_empty() { None } else { Some(task_description.clone()) };

                tokio::spawn(async move {
                    // Notify that task is starting
                    bridge_clone.broadcast_event(BridgeEvent::new("task_started", serde_json::json!({
                        "automation_id": auto_id,
                        "task_description": task_desc_for_closure,
                    })));

                    match crate::engine::automation_agent_engine::execute_automation(
                        &app_clone,
                        auto_id,
                        additional_instructions,
                    ).await {
                        Ok(_) => {
                            bridge_clone.broadcast_event(BridgeEvent::new("task_completed", serde_json::json!({
                                "automation_id": auto_id,
                                "status_message": "Task completed successfully",
                            })));
                        }
                        Err(e) => {
                            bridge_clone.broadcast_event(BridgeEvent::new("task_failed", serde_json::json!({
                                "automation_id": auto_id,
                                "error": e,
                                "status_message": format!("Task failed: {}", e),
                            })));
                        }
                    }
                });

                Some(BridgeEvent::new("task_started", serde_json::json!({
                    "automation_id": automation_id,
                    "task_description": task_description,
                    "status_message": "Starting automation...",
                })))
            } else {
                // Create a new automation from the task description and run it
                let app_clone = app_handle.clone();
                let bridge_clone = bridge_state.clone();
                let task_desc = task_description.clone();

                tokio::spawn(async move {
                    bridge_clone.broadcast_event(BridgeEvent::new("task_started", serde_json::json!({
                        "task_description": task_desc,
                        "status_message": "Creating and starting task...",
                    })));

                    // First, generate a synthetic automation from the prompt
                    let name = if task_desc.len() > 50 {
                        format!("{}...", &task_desc[..47])
                    } else {
                        task_desc.clone()
                    };

                    // Save as a new automation with the task as objective
                    let automation_id = app_clone.db(|db| -> Result<i64, rusqlite::Error> {
                        crate::repository::automation_repository::save_automation(
                            db,
                            &name,
                            &task_desc,
                            "", // raw_script
                            "", // generalized_script
                        )
                    });

                    match automation_id {
                        Ok(auto_id) => {
                            // Now execute the automation
                            match crate::engine::automation_agent_engine::execute_automation(
                                &app_clone,
                                auto_id,
                                Some(task_desc.clone()),
                            ).await {
                                Ok(_) => {
                                    bridge_clone.broadcast_event(BridgeEvent::new("task_completed", serde_json::json!({
                                        "automation_id": auto_id,
                                        "status_message": "Task completed successfully",
                                    })));
                                }
                                Err(e) => {
                                    bridge_clone.broadcast_event(BridgeEvent::new("task_failed", serde_json::json!({
                                        "automation_id": auto_id,
                                        "error": e,
                                        "status_message": format!("Task failed: {}", e),
                                    })));
                                }
                            }
                        }
                        _ => {
                            bridge_clone.broadcast_event(BridgeEvent::new("task_failed", serde_json::json!({
                                "error": "Failed to create automation",
                                "status_message": "Failed to create task",
                            })));
                        }
                    }
                });

                Some(BridgeEvent::new("task_started", serde_json::json!({
                    "task_description": task_description,
                    "status_message": "Creating task...",
                })))
            }
        }

        "stop_task" => {
            info!("[RemoteBridge] Stopping current task");
            match crate::engine::automation_agent_engine::stop_execution() {
                Ok(_) => {
                    crate::engine::automation_agent_engine::finalize_execution_run(
                        app_handle, "stopped", None,
                    );
                    let _ = app_handle.emit("playback_status", serde_json::json!({
                        "isPlaying": false,
                        "progress": 0.0,
                        "statusMessage": "Automation stopped (from web)"
                    }));
                    bridge_state.broadcast_event(BridgeEvent::new("task_stopped", serde_json::json!({
                        "status_message": "Task stopped by remote command",
                    })));
                    Some(BridgeEvent::new("task_stopped", serde_json::json!({
                        "status_message": "Task stopped",
                    })))
                }
                Err(e) => {
                    Some(BridgeEvent::new("task_failed", serde_json::json!({
                        "error": e,
                        "status_message": format!("Failed to stop: {}", e),
                    })))
                }
            }
        }

        _ => {
            warn!("[RemoteBridge] Unknown command type: {}", cmd.command_type);
            None
        }
    }
}

// ===== Tauri Commands for the desktop frontend =====

/// Get the current bridge status
#[tauri::command]
pub fn get_remote_bridge_status(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let state = get_bridge_state(&app_handle);
    Ok(serde_json::json!({
        "running": state.is_running(),
        "port": state.get_port(),
        "pairing_code": state.get_pairing_code(),
        "connected_clients": state.get_connected_clients(),
    }))
}

/// Regenerate the pairing code
#[tauri::command]
pub fn regenerate_pairing_code(app_handle: AppHandle) -> Result<String, String> {
    let state = get_bridge_state(&app_handle);
    let new_code = state.regenerate_pairing_code();
    info!("[RemoteBridge] Pairing code regenerated");
    Ok(new_code)
}

/// Start the remote bridge server
#[tauri::command]
pub fn start_remote_bridge(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    let state = get_bridge_state(&app_handle);
    
    if state.is_running() {
        return Ok(serde_json::json!({
            "running": true,
            "port": state.get_port(),
            "pairing_code": state.get_pairing_code(),
            "message": "Bridge is already running",
        }));
    }

    let app_clone = app_handle.clone();
    let state_clone = state.clone();
    
    tauri::async_runtime::spawn(async move {
        start_bridge_server(app_clone, state_clone).await;
    });

    Ok(serde_json::json!({
        "running": true,
        "port": state.get_port(),
        "pairing_code": state.get_pairing_code(),
        "message": "Bridge server starting",
    }))
}

lazy_static::lazy_static! {
    /// Global singleton bridge state
    static ref GLOBAL_BRIDGE_STATE: BridgeState = BridgeState::new();
}

// Helper to get the global bridge state
fn get_bridge_state(_app_handle: &AppHandle) -> BridgeState {
    GLOBAL_BRIDGE_STATE.clone()
}

/// Get the global bridge state (for use outside of Tauri commands)
pub fn global_bridge_state() -> BridgeState {
    GLOBAL_BRIDGE_STATE.clone()
}

/// Hook into the automation engine events to broadcast status updates.
/// Call this from the playback_status event handler in main.rs
pub fn broadcast_playback_status(is_playing: bool, progress: f64, status_message: &str) {
    if GLOBAL_BRIDGE_STATE.get_connected_clients() > 0 {
        let event_type = if is_playing { "task_progress" } else { "task_completed" };
        GLOBAL_BRIDGE_STATE.broadcast_event(BridgeEvent::new(event_type, serde_json::json!({
            "is_running": is_playing,
            "progress": progress,
            "status_message": status_message,
        })));
    }
}

// ===== Supabase-relay Tauri Commands =====
// These are called by the React RemoteBridgeListener when commands come in via Supabase.

/// Get all automations as JSON (for remote relay)
#[tauri::command]
pub fn get_all_automations_command(app_handle: AppHandle) -> Result<serde_json::Value, String> {
    match app_handle.db(|db| automation_repository::get_all_automations(db)) {
        Ok(automations) => {
            let list: Vec<serde_json::Value> = automations.iter().map(|a| {
                serde_json::json!({
                    "id": a.id,
                    "name": a.name,
                    "objective": a.objective,
                    "description": a.nl_description,
                    "created_at": a.created_at,
                    "updated_at": a.updated_at,
                })
            }).collect();
            Ok(serde_json::json!(list))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Get execution history as JSON (for remote relay)
#[tauri::command]
pub fn get_execution_history_command(app_handle: AppHandle, limit: i32) -> Result<serde_json::Value, String> {
    // Get automations for name lookup
    let automations_map: std::collections::HashMap<i64, String> = app_handle
        .db(|db| automation_repository::get_all_automations(db))
        .unwrap_or_default()
        .into_iter()
        .map(|a| (a.id, a.name))
        .collect();

    match app_handle.db(|db| automation_execution_repository::get_recent_execution_runs_with_steps(db, limit)) {
        Ok(runs) => {
            let list: Vec<serde_json::Value> = runs.iter().map(|r| {
                let automation_name = automations_map
                    .get(&r.run.automation_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Automation #{}", r.run.automation_id));
                serde_json::json!({
                    "id": r.run.id,
                    "automation_id": r.run.automation_id,
                    "automation_name": automation_name,
                    "status": r.run.status,
                    "started_at": r.run.started_at,
                    "completed_at": r.run.completed_at,
                    "completion_message": r.run.completion_message,
                    "error": r.run.error_message,
                })
            }).collect();
            Ok(serde_json::json!(list))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Execute a task from the remote web app. Creates a new automation if no ID provided, then runs it.
/// For new tasks (no automation_id), this goes through the same `generate_synthetic_automation`
/// pipeline as the desktop UI — producing a proper name, structured plan, and NL description
/// via the LLM before executing.
#[tauri::command]
pub async fn execute_remote_task(
    app_handle: AppHandle,
    task_description: String,
    automation_id: Option<i64>,
) -> Result<String, String> {
    info!("[RemoteBridge] execute_remote_task: {:?} (automation_id: {:?})", task_description, automation_id);

    if let Some(auto_id) = automation_id {
        // Run an existing automation
        let additional = if task_description.is_empty() { None } else { Some(task_description) };
        crate::engine::automation_agent_engine::execute_automation(
            &app_handle,
            auto_id,
            additional,
        ).await.map_err(|e| e.to_string())?;
        Ok("Task completed".to_string())
    } else if !task_description.is_empty() {
        // Generate a proper automation through the same pipeline the desktop UI uses.
        // This produces an LLM-generated name, structured plan/script, and NL description.
        let initial_name = task_description
            .split_whitespace()
            .take(5)
            .collect::<Vec<_>>()
            .join(" ");

        let request = crate::engine::synthetic_automation::SyntheticGenerationRequest {
            prompt: task_description.clone(),
            name: initial_name,
            api_key: None,    // Will be resolved from settings inside generate_synthetic_automation
            api_choice: None, // Will be resolved from settings inside generate_synthetic_automation
        };

        let gen_result = crate::engine::synthetic_automation::generate_synthetic_automation(
            &app_handle,
            request,
        ).await?;

        if !gen_result.success {
            let err_msg = gen_result.error.unwrap_or_else(|| "Unknown error generating automation".to_string());
            error!("[RemoteBridge] Synthetic generation failed: {}", err_msg);
            return Err(err_msg);
        }

        let auto_id = gen_result.automation_id
            .ok_or_else(|| "Synthetic generation succeeded but no automation_id returned".to_string())?;

        info!("[RemoteBridge] Synthetic automation created: ID={}, name={:?}",
              auto_id, gen_result.generated_name);

        // Now execute the properly planned automation
        crate::engine::automation_agent_engine::execute_automation(
            &app_handle,
            auto_id,
            None, // No additional instructions needed — the plan is in the generalized_script
        ).await.map_err(|e| e.to_string())?;

        Ok("Task completed".to_string())
    } else {
        Err("No task description or automation ID provided".to_string())
    }
}

/// Stop the current running task (for remote relay)
#[tauri::command]
pub fn stop_remote_task(app_handle: AppHandle) -> Result<String, String> {
    info!("[RemoteBridge] stop_remote_task");
    crate::engine::automation_agent_engine::stop_execution()?;
    crate::engine::automation_agent_engine::finalize_execution_run(&app_handle, "stopped", None);
    Ok("Task stopped".to_string())
}

/// Get full execution details (run info + steps) for a single execution run
#[tauri::command]
pub fn get_execution_details_command(app_handle: AppHandle, execution_run_id: i64) -> Result<serde_json::Value, String> {
    info!("[RemoteBridge] get_execution_details_command for run {}", execution_run_id);

    // Get the steps
    let steps = app_handle
        .db(|db| automation_execution_repository::get_execution_steps_by_run(db, execution_run_id))
        .map_err(|e| e.to_string())?;

    // Get the run itself (from the recent runs -- find the matching one)
    let runs = app_handle
        .db(|db| automation_execution_repository::get_recent_execution_runs_with_steps(db, 100))
        .map_err(|e| e.to_string())?;

    let run = runs.iter().find(|r| r.run.id == execution_run_id);

    let run_info = if let Some(r) = run {
        // Look up automation name
        let automation_name = app_handle
            .db(|db| automation_repository::get_automation_by_id(db, r.run.automation_id))
            .ok()
            .flatten()
            .map(|a| a.name)
            .unwrap_or_else(|| format!("Automation #{}", r.run.automation_id));

        serde_json::json!({
            "id": r.run.id,
            "automation_id": r.run.automation_id,
            "automation_name": automation_name,
            "status": r.run.status,
            "started_at": r.run.started_at,
            "completed_at": r.run.completed_at,
            "completion_message": r.run.completion_message,
            "additional_instructions": r.run.additional_instructions,
            "error": r.run.error_message,
        })
    } else {
        return Err(format!("Execution run {} not found", execution_run_id));
    };

    let steps_json: Vec<serde_json::Value> = steps.iter().map(|s| {
        serde_json::json!({
            "id": s.id,
            "step_number": s.step_number,
            "timestamp": s.timestamp,
            "explanation": s.explanation,
            "next_step": s.next_step,
            "status": s.status,
            "error_message": s.error_message,
            "step_type": s.step_type,
        })
    }).collect();

    Ok(serde_json::json!({
        "run": run_info,
        "steps": steps_json,
    }))
}
