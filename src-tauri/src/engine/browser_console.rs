use log::{info, warn};
use serde_json::Value;

const CDP_DISCOVERY_URL: &str = "http://localhost:9222/json";
const CDP_TIMEOUT_MS: u64 = 3000;

/// Fetch recent console errors from Chrome via the DevTools Protocol.
/// Requires Chrome to be running with --remote-debugging-port=9222.
/// Returns None if CDP is unavailable (Chrome not in debug mode).
pub async fn get_console_errors() -> Option<String> {
    let ws_url = match discover_debug_ws_url().await {
        Some(url) => url,
        None => return None,
    };

    match fetch_console_errors_via_cdp(&ws_url).await {
        Ok(errors) if errors.is_empty() => None,
        Ok(errors) => Some(errors),
        Err(e) => {
            warn!("CDP console fetch failed: {}", e);
            None
        }
    }
}

/// Hit Chrome's /json endpoint to find the WebSocket debugger URL for the active page.
async fn discover_debug_ws_url() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(CDP_TIMEOUT_MS))
        .build()
        .ok()?;

    let resp = client.get(CDP_DISCOVERY_URL).send().await.ok()?;
    let targets: Vec<Value> = resp.json().await.ok()?;

    // Find the first "page" type target (skip extensions, service workers, etc.)
    for target in &targets {
        if target.get("type").and_then(|t| t.as_str()) == Some("page") {
            if let Some(ws_url) = target.get("webSocketDebuggerUrl").and_then(|u| u.as_str()) {
                return Some(ws_url.to_string());
            }
        }
    }

    None
}

/// Connect to Chrome via WebSocket, inject a console error collector,
/// then retrieve accumulated errors.
async fn fetch_console_errors_via_cdp(ws_url: &str) -> Result<String, String> {
    use tokio_tungstenite::connect_async;
    use futures::prelude::*;

    let (mut ws, _) = connect_async(ws_url)
        .await
        .map_err(|e| format!("WebSocket connect failed: {}", e))?;

    // Inject a collector script that patches console.error if not already patched,
    // then harvest whatever has been collected so far.
    let inject_and_harvest = serde_json::json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": r#"
                (function() {
                    if (!window.__heelixConsoleErrors) {
                        window.__heelixConsoleErrors = [];
                        const orig = console.error;
                        console.error = function() {
                            const msg = Array.from(arguments).map(a => {
                                if (a instanceof Error) return a.stack || a.message;
                                try { return typeof a === 'object' ? JSON.stringify(a) : String(a); }
                                catch(e) { return String(a); }
                            }).join(' ');
                            window.__heelixConsoleErrors.push(msg);
                            if (window.__heelixConsoleErrors.length > 50) {
                                window.__heelixConsoleErrors.shift();
                            }
                            orig.apply(console, arguments);
                        };

                        window.addEventListener('error', function(e) {
                            window.__heelixConsoleErrors.push(
                                'Uncaught ' + (e.error ? (e.error.stack || e.error.message) : e.message)
                                + ' at ' + e.filename + ':' + e.lineno
                            );
                        });

                        window.addEventListener('unhandledrejection', function(e) {
                            const reason = e.reason;
                            window.__heelixConsoleErrors.push(
                                'Unhandled Promise rejection: '
                                + (reason instanceof Error ? (reason.stack || reason.message) : String(reason))
                            );
                        });
                    }
                    return JSON.stringify(window.__heelixConsoleErrors);
                })()
            "#,
            "returnByValue": true
        }
    });

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        inject_and_harvest.to_string(),
    ))
    .await
    .map_err(|e| format!("WebSocket send failed: {}", e))?;

    // Read response with timeout
    let response = tokio::time::timeout(
        std::time::Duration::from_millis(CDP_TIMEOUT_MS),
        ws.next(),
    )
    .await
    .map_err(|_| "CDP response timeout".to_string())?
    .ok_or("WebSocket stream ended")?
    .map_err(|e| format!("WebSocket read error: {}", e))?;

    let _ = ws.close(None).await;

    let msg_text = response.to_text().map_err(|e| format!("Non-text response: {}", e))?;
    let parsed: Value = serde_json::from_str(msg_text)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    // Extract the result value — it's a JSON-stringified array
    let result_str = parsed
        .pointer("/result/result/value")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");

    let errors: Vec<String> = serde_json::from_str(result_str).unwrap_or_default();

    if errors.is_empty() {
        return Ok(String::new());
    }

    info!("CDP: captured {} console errors", errors.len());

    let mut output = format!("Browser console errors ({}):\n", errors.len());
    for (i, err) in errors.iter().enumerate() {
        let truncated: &str = if err.len() > 500 {
            // Safe truncate at char boundary
            let mut end = 500;
            while end > 0 && !err.is_char_boundary(end) {
                end -= 1;
            }
            &err[..end]
        } else {
            err
        };
        output.push_str(&format!("{}. {}\n", i + 1, truncated));
    }

    // Cap total output
    if output.len() > 6000 {
        output.truncate(6000);
        output.push_str("\n... (truncated)");
    }

    Ok(output)
}

/// Clear the collected console errors (call after the executor has seen them).
pub async fn clear_console_errors() -> Option<()> {
    let ws_url = discover_debug_ws_url().await?;

    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.ok()?;

    use futures::prelude::*;
    let clear_cmd = serde_json::json!({
        "id": 2,
        "method": "Runtime.evaluate",
        "params": {
            "expression": "window.__heelixConsoleErrors = []",
            "returnByValue": true
        }
    });

    ws.send(tokio_tungstenite::tungstenite::Message::Text(clear_cmd.to_string()))
        .await
        .ok()?;

    let _ = ws.close(None).await;
    Some(())
}
