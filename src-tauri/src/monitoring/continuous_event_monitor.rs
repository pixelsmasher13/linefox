use crate::entity::user_event::{UserEvent, UserEventType};
use log::{error, info};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[cfg(target_os = "macos")]
use crate::window_details_collector::macos::macos_accessibility_engine::{observe_pid_continuously, get_active_pid};

pub struct ContinuousEventMonitor {
    handle: Option<JoinHandle<()>>,
    running: Arc<Mutex<bool>>,
}

impl ContinuousEventMonitor {
    pub fn new() -> Self {
        ContinuousEventMonitor {
            handle: None,
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start(&mut self, app_handle: AppHandle) {
        let already_running = {
            let mut running = self.running.lock().await;
            let was_running = *running;
            *running = true;
            was_running
        };

        if already_running {
            info!("Continuous event monitoring already running");
            return;
        }

        info!("Starting continuous event monitoring");
        let running_clone = self.running.clone();
        
        self.handle = Some(tokio::spawn(async move {
            let monitor_interval = Duration::from_millis(100);
            
            while *running_clone.lock().await {
                #[cfg(target_os = "macos")]
                {
                    match get_active_pid() {
                        Ok(pid) => {
                            if let Err(e) = observe_pid_continuously(&pid.to_string()) {
                                error!("Error observing PID {}: {}", pid, e);
                            }
                        }
                        Err(e) => {
                            error!("Error getting active PID: {}", e);
                        }
                    }
                }
                
                #[cfg(not(target_os = "macos"))]
                {
                    info!("Continuous event monitoring is only supported on macOS");
                }
                
                tokio::time::sleep(monitor_interval).await;
            }
            
            info!("Continuous event monitoring stopped");
        }));
    }

    pub async fn stop(&mut self) {
        info!("Stopping continuous event monitoring");
        {
            let mut running = self.running.lock().await;
            *running = false;
        }
        
        if let Some(handle) = self.handle.take() {
            if let Err(e) = handle.await {
                error!("Error stopping continuous event monitoring: {}", e);
            }
        }
    }
}
