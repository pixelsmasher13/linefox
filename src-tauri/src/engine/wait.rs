use log::{info, debug, warn};
use std::time::{Duration, Instant};

use crate::engine::types::{AgentAction, ActionType, AppState};

#[allow(dead_code)]
pub const MAX_PAGE_STABILITY_WAIT_SECONDS: u64 = 1;
const MIN_ELEMENTS_WEB_BROWSER: usize = 10;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum WaitCondition {
    DomStable { threshold_ms: u64 },
    ElementCountChanged { from: usize },
    ElementCountDelta { from: usize, min_delta: usize },
    ElementPresent { selector: String },
    ElementNotPresent { selector: String },
    NetworkIdle { threshold_ms: u64 },
    MinimumWait { duration: Duration },
}

pub struct WaitConfig {
    pub max_wait: Duration,
    pub check_interval: Duration,
}

impl Default for WaitConfig {
    fn default() -> Self {
        Self {
            max_wait: Duration::from_secs(2),
            check_interval: Duration::from_millis(450),
        }
    }
}

pub async fn wait_for_conditions<F>(
    conditions: &[WaitCondition],
    config: WaitConfig,
    update_app_state: F,
) -> Result<AppState, String>
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AppState, String>> + Send>> + Send,
{
    let start = Instant::now();
    let mut last_element_count = 0;
    let mut last_change = Instant::now();
    let mut conditions_met = vec![false; conditions.len()];
    let mut dom_stable_failures: u8 = 0;
    let mut skip_updates_for_large_dom = false;
    let mut cached_app_state: Option<AppState> = None;
    let mut final_app_state: Option<AppState> = None;
    
    info!("Waiting for {} conditions with max wait {:?}", conditions.len(), config.max_wait);
    
    while start.elapsed() < config.max_wait {
        // When we have a cached state, use quick element count to detect changes
        // without doing an expensive full scan
        let (app_state, current_element_count) = if skip_updates_for_large_dom && cached_app_state.is_some() {
            let cached = cached_app_state.as_ref().unwrap();
            let cached_count = cached.accessible_elements.len();

            // Get quick count for stability tracking (cheap operation)
            #[cfg(target_os = "macos")]
            let quick_count = if let Some(pid) = &cached.current_pid {
                crate::window_details_collector::macos::macos_action_detector_engine::get_quick_element_count(pid)
            } else {
                cached_count
            };

            #[cfg(not(target_os = "macos"))]
            let quick_count = cached_count;

            info!("Using cached state - quick count: {}, cached full count: {}", quick_count, cached_count);

            // If quick count changed significantly, do a full rescan
            let delta = (quick_count as i32 - cached_count as i32).abs() as usize;
            if delta > 20 {
                info!("Quick count changed significantly (delta {}), triggering full rescan", delta);
                let fresh_state = update_app_state().await?;
                let fresh_count = fresh_state.accessible_elements.len();
                cached_app_state = Some(fresh_state.clone());
                (fresh_state, fresh_count)
            } else {
                // Use quick count for stability tracking, but keep cached state
                (cached.clone(), quick_count)
            }
        } else {
            let state = update_app_state().await?;
            let element_count = state.accessible_elements.len();

            if element_count > 20 && !skip_updates_for_large_dom {
                info!("Large element tree detected ({} elements) - will use quick counts for future checks", element_count);
                skip_updates_for_large_dom = true;
                cached_app_state = Some(state.clone());
            }

            (state, element_count)
        };
        
        final_app_state = Some(app_state.clone());
        
        for (i, condition) in conditions.iter().enumerate() {
            if conditions_met[i] {
                continue;
            }
            
            conditions_met[i] = match condition {
                WaitCondition::DomStable { threshold_ms } => {
                    let is_web_browser = app_state.current_app.as_deref()
                        .map(|a| a.contains("Chrome") || a.contains("Safari") || a.contains("Firefox") || a.contains("Edge"))
                        .unwrap_or(false);

                    let delta = if last_element_count > 0 {
                        (current_element_count as i32 - last_element_count as i32).abs() as usize
                    } else {
                        0
                    };

                    if delta > 2 {
                        last_change = Instant::now();
                        last_element_count = current_element_count;
                        debug!("DOM changed (delta {}): {} elements", delta, current_element_count);
                    }

                    let stable_for = last_change.elapsed();

                    let effective_threshold = if is_web_browser && current_element_count >= MIN_ELEMENTS_WEB_BROWSER {
                        Duration::from_millis(200)
                    } else {
                        Duration::from_millis(*threshold_ms)
                    };

                    let mut is_stable = delta <= 2 && stable_for >= effective_threshold;

                    if !is_stable {
                        dom_stable_failures += 1;
                        if dom_stable_failures >= 3 {
                            warn!("DomStable condition failed 3× – proceeding anyway");
                            is_stable = true;
                        }
                    } else {
                        dom_stable_failures = 0;
                    }

                    if is_stable {
                        debug!("DOM considered stable (delta {}, stable_for {:?}, threshold {:?})", delta, stable_for, effective_threshold);
                    }

                    is_stable
                },
                WaitCondition::ElementCountChanged { from } => {
                    let changed = current_element_count != *from && current_element_count > 0;
                    if changed {
                        debug!("Element count changed from {} to {}", from, current_element_count);
                    }
                    changed
                },
                WaitCondition::ElementCountDelta { from, min_delta } => {
                    let delta = if current_element_count > *from {
                        current_element_count - *from
                    } else {
                        *from - current_element_count
                    };
                    let changed = delta >= *min_delta && current_element_count > 0;
                    if changed {
                        debug!("Element count changed by {} (min required {})", delta, min_delta);
                    }
                    changed
                },
                WaitCondition::ElementPresent { selector } => {
                    let found = app_state.accessible_elements.iter().any(|e| 
                        e.to_lowercase().contains(&selector.to_lowercase())
                    );
                    if found {
                        debug!("Found element matching '{}'", selector);
                    }
                    found
                },
                WaitCondition::ElementNotPresent { selector } => {
                    let not_found = !app_state.accessible_elements.iter().any(|e| 
                        e.to_lowercase().contains(&selector.to_lowercase())
                    );
                    if not_found {
                        debug!("Element matching '{}' is no longer present", selector);
                    }
                    not_found
                },
                WaitCondition::NetworkIdle { threshold_ms } => {
                    let idle_for = last_change.elapsed();
                    let is_idle = idle_for > Duration::from_millis(*threshold_ms);
                    if is_idle {
                        debug!("Network assumed idle for {:?}", idle_for);
                    }
                    is_idle
                },
                WaitCondition::MinimumWait { duration } => {
                    let elapsed = start.elapsed();
                    let met = elapsed >= *duration;
                    if met {
                        debug!("Minimum wait {:?} satisfied", duration);
                    }
                    met
                },
            };
        }
        
        if conditions_met.iter().all(|&met| met) {
            info!("All wait conditions met after {:?}", start.elapsed());
            return final_app_state.ok_or_else(|| "No app state available".to_string());
        }
        
        tokio::time::sleep(config.check_interval).await;
    }
    
    for (i, met) in conditions_met.iter().enumerate() {
        if !*met {
            warn!("Condition not met after timeout: {:?}", conditions[i]);
        }
    }
    
    info!("Wait timed out after {:?}, proceeding anyway", config.max_wait);
    final_app_state.ok_or_else(|| "No app state available after timeout".to_string())
}

#[allow(dead_code)]
pub async fn wait_for_page_stability<F>(
    app_state: &AppState,
    max_wait_seconds: u64,
    update_app_state: F,
) -> Result<(), String>
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AppState, String>> + Send>> + Send,
{
    if app_state.current_pid.is_some() {
        let config = WaitConfig {
            max_wait: Duration::from_secs(std::cmp::min(max_wait_seconds, MAX_PAGE_STABILITY_WAIT_SECONDS)),
            check_interval: Duration::from_millis(500),
        };
        
        info!("Waiting for page stability using smart conditions (max {} seconds)", config.max_wait.as_secs());
        
        let _ = wait_for_conditions(&[
            WaitCondition::DomStable { threshold_ms: 500 },
            WaitCondition::MinimumWait { duration: Duration::from_millis(300) },
        ], config, update_app_state).await?;
    }
    
    Ok(())
}

pub async fn smart_wait_after_action<F>(
    action: &AgentAction,
    app_state: &AppState,
    update_app_state: F,
) -> Result<Option<AppState>, String>
where
    F: Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<AppState, String>> + Send>> + Send + Copy,
{
    // On Windows with NVDA, DOM stability checks don't make sense
    // NVDA only gives snapshots when requested, not continuous monitoring
    #[cfg(target_os = "windows")]
    {
        // Use simple fixed delays based on action type
        match action.action_type {
            ActionType::ClickElement => {
                info!("Windows/NVDA: Fixed wait after click (1000ms)");
                tokio::time::sleep(Duration::from_millis(1400)).await;
                // Get fresh state after the wait
                let final_state = update_app_state().await.ok();
                return Ok(final_state);
            },
            ActionType::TypeText => {
                let is_navigation = if let Some(params) = &action.parameters {
                    if let Some(text) = params.get("text") {
                        (text.contains("http") || text.contains("www")) && !text.contains("@")
                    } else {
                        false
                    }
                } else {
                    false
                };
                
                if is_navigation {
                    info!("Windows/NVDA: Fixed wait after URL navigation (1000ms)");
                    tokio::time::sleep(Duration::from_millis(1400)).await;
                    let final_state = update_app_state().await.ok();
                    return Ok(final_state);
                } else {
                    info!("Windows/NVDA: Fixed wait after typing (800ms)");
                    tokio::time::sleep(Duration::from_millis(800)).await;
                    return Ok(None);
                }
            },
            ActionType::LaunchApp => {
                info!("Windows/NVDA: Fixed wait after app launch (800ms)");
                tokio::time::sleep(Duration::from_millis(1200)).await;
                let final_state = update_app_state().await.ok();
                return Ok(final_state);
            },
            ActionType::PressKey => {
                let is_navigation_key = if let Some(params) = &action.parameters {
                    if let Some(key) = params.get("key") {
                        key == "enter" || key == "return" || key == "tab"
                    } else {
                        false
                    }
                } else {
                    false
                };
                
                if is_navigation_key {
                    info!("Windows/NVDA: Fixed wait after navigation key (700ms)");
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    let final_state = update_app_state().await.ok();
                    return Ok(final_state);
                } else {
                    info!("Windows/NVDA: Fixed wait after key press (200ms)");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    return Ok(None);
                }
            },
            ActionType::NavigateURL | ActionType::GoogleSearch => {
                info!("Windows/NVDA: Fixed wait after URL navigation (2000ms for page load)");
                // Wait longer for page navigation to complete
                tokio::time::sleep(Duration::from_millis(2000)).await;
                // Get fresh state after navigation
                let final_state = update_app_state().await.ok();
                return Ok(final_state);
            },
            _ => {
                info!("Windows/NVDA: Minimal wait for action type {:?}", action.action_type);
                tokio::time::sleep(Duration::from_millis(100)).await;
                return Ok(None);
            }
        }
    }
    
    // Original implementation for macOS and other platforms
    #[cfg(not(target_os = "windows"))]
    let element_count_before = app_state.accessible_elements.len();
    
    #[cfg(not(target_os = "windows"))]
    let config = WaitConfig {
        max_wait: Duration::from_secs(2),
        check_interval: Duration::from_millis(500),
    };
    
    #[cfg(not(target_os = "windows"))]
    match action.action_type {
        ActionType::ClickElement => {
            info!("Smart wait after click: waiting for DOM stability or element change");

            // Use consistent wait for all apps
            let min_wait = Duration::from_millis(2000);
            info!("Click wait time: {}ms for app: {:?}", min_wait.as_millis(), app_state.current_app);

            tokio::time::sleep(min_wait).await;

            let stability_config = WaitConfig {
                max_wait: Duration::from_millis(1500),
                check_interval: Duration::from_millis(800),
            };

            let final_state = wait_for_conditions(&[
                WaitCondition::DomStable { threshold_ms: 800 },
            ], stability_config, update_app_state).await.ok();
            return Ok(final_state);
        },
        ActionType::TypeText => {
            info!("Smart wait after typing: minimal wait");
            tokio::time::sleep(Duration::from_millis(300)).await;
            return Ok(None);
        },
        ActionType::LaunchApp => {
            info!("Smart wait after app launch: waiting for elements to appear");
            
            let is_browser = action.parameters.as_ref()
                .and_then(|p| p.get("app"))
                .map(|app| app.contains("Chrome") || app.contains("Safari") || 
                           app.contains("Firefox") || app.contains("Edge"))
                .unwrap_or(false);
            
            let max_wait_time = if is_browser {
                Duration::from_millis(750)
            } else {
                Duration::from_millis(2000)
            };
            
            let launch_config = WaitConfig {
                max_wait: max_wait_time,
                check_interval: Duration::from_millis(100),
            };
            let final_state = wait_for_conditions(&[
                WaitCondition::ElementCountChanged { from: 0 },
                WaitCondition::DomStable { threshold_ms: 300 },
                WaitCondition::MinimumWait { duration: Duration::from_millis(300) },
            ], launch_config, update_app_state).await?;
            return Ok(Some(final_state))
        },
        ActionType::PressKey => {
            let is_navigation_key = if let Some(params) = &action.parameters {
                if let Some(key) = params.get("key") {
                    key == "enter" || key == "return" || key == "tab"
                } else {
                    false
                }
            } else {
                false
            };
            
            let is_save_key = if let Some(params) = &action.parameters {
                if let Some(key) = params.get("key") {
                    key.contains("cmd+enter") || key.contains("cmd+s")
                } else {
                    false
                }
            } else {
                false
            };
            
            if is_save_key {
                info!("Smart wait after save key: waiting for dialog to process");
                let final_state = wait_for_conditions(&[
                    WaitCondition::MinimumWait { duration: Duration::from_millis(600) },
                ], config, update_app_state).await?;
                return Ok(Some(final_state))
            } else if is_navigation_key {
                // Enter/Return in a browser triggers a full page navigation (e.g. submitting
                // a Google search). Wait comparable to NavigateURL so the page has time to load.
                let is_browser = app_state.current_app.as_deref()
                    .map(|a| a.contains("Chrome") || a.contains("Safari") || a.contains("Firefox") || a.contains("Edge") || a.contains("Arc"))
                    .unwrap_or(false);

                if is_browser {
                    info!("Smart wait after navigation key in browser: waiting for page load (2000ms + stability)");
                    tokio::time::sleep(Duration::from_millis(2000)).await;
                    let nav_config = WaitConfig {
                        max_wait: Duration::from_secs(1),
                        check_interval: Duration::from_millis(500),
                    };
                    let final_state = wait_for_conditions(&[
                        WaitCondition::ElementCountDelta { from: element_count_before, min_delta: 10 },
                        WaitCondition::DomStable { threshold_ms: 500 },
                        WaitCondition::MinimumWait { duration: Duration::from_millis(500) },
                    ], nav_config, update_app_state).await?;
                    return Ok(Some(final_state));
                }

                info!("Smart wait after navigation key: waiting for changes");
                let final_state = wait_for_conditions(&[
                    WaitCondition::ElementCountChanged { from: element_count_before },
                    WaitCondition::DomStable { threshold_ms: 500 },
                    WaitCondition::MinimumWait { duration: Duration::from_millis(500) },
                ], config, update_app_state).await?;
                return Ok(Some(final_state));
            } else {
                info!("Smart wait after key press: waiting for DOM stability");
                tokio::time::sleep(Duration::from_millis(1400)).await;

                let stability_config = WaitConfig {
                    max_wait: Duration::from_millis(1500),
                    check_interval: Duration::from_millis(800),
                };

                let final_state = wait_for_conditions(&[
                    WaitCondition::DomStable { threshold_ms: 800 },
                ], stability_config, update_app_state).await.ok();
                return Ok(final_state);
            }
        },
        ActionType::WaitTime => {
            if let Some(params) = &action.parameters {
                if let Some(wait_time) = params.get("wait_time") {
                    let seconds = wait_time.parse::<u64>().unwrap_or(1);
                    info!("Waiting for {} seconds", seconds);
                    tokio::time::sleep(Duration::from_secs(seconds)).await;
                } else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            return Ok(None);
        },
        ActionType::RequestTakeover => {
            info!("No additional wait after user takeover");
            return Ok(None);
        },
        ActionType::AskClarification => {
            info!("No additional wait after user clarification");
            return Ok(None);
        },
        ActionType::MemorySave | ActionType::FetchPages => {
            info!("No wait after {:?} - UI unchanged", action.action_type);
            return Ok(None);
        },
        ActionType::NavigateURL | ActionType::GoogleSearch => {
            info!("Smart wait after URL navigation: waiting for page load");

            // Initial wait to let navigation start
            // GoogleSearch needs extra time since it includes the full launch+navigate sequence
            let min_wait = if action.action_type == ActionType::GoogleSearch {
                Duration::from_millis(3000)
            } else {
                Duration::from_millis(2000)
            };
            info!("Initial navigation wait: {}ms", min_wait.as_millis());
            tokio::time::sleep(min_wait).await;

            let nav_config = WaitConfig {
                max_wait: Duration::from_secs(1),
                check_interval: Duration::from_millis(500),
            };
            let final_state = wait_for_conditions(&[
                WaitCondition::ElementCountDelta { from: element_count_before, min_delta: 10 },
                WaitCondition::DomStable { threshold_ms: 500 },
                WaitCondition::MinimumWait { duration: Duration::from_millis(500) },
            ], nav_config, update_app_state).await?;
            return Ok(Some(final_state));
        },
        _ => {
            info!("Smart wait default: minimal wait");
            tokio::time::sleep(Duration::from_millis(300)).await;
            return Ok(None);
        }
    }
}