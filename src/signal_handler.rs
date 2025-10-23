use std::sync::{Arc, Mutex};

pub struct SignalHandler {
    current_child: Arc<Mutex<Option<u32>>>,
}

impl SignalHandler {
    pub fn new() -> Self {
        SignalHandler {
            current_child: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_child(&self, pid: Option<u32>) {
        if let Ok(mut child) = self.current_child.lock() {
            *child = pid;
        }
    }

    pub fn get_child(&self) -> Option<u32> {
        if let Ok(child) = self.current_child.lock() {
            *child
        } else {
            None
        }
    }

    pub fn setup_handler(&self) {
        let current_child = Arc::clone(&self.current_child);
        
        #[cfg(unix)]
        {
            use std::sync::atomic::{AtomicBool, Ordering};
            static HANDLER_SET: AtomicBool = AtomicBool::new(false);
            
            if HANDLER_SET.swap(true, Ordering::SeqCst) {
                return;
            }

            std::thread::spawn(move || {
                use nix::sys::signal::{self, Signal};
                use nix::unistd::Pid;
                
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    
                    if let Ok(child_lock) = current_child.lock() {
                        if let Some(pid) = *child_lock {
                            let _ = signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT);
                        }
                    }
                }
            });
        }
    }
}

pub fn send_sigint_to_pid(pid: u32) -> Result<(), String> {
    #[cfg(unix)]
    {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;
        
        signal::kill(Pid::from_raw(pid as i32), Signal::SIGINT)
            .map_err(|e| format!("Failed to send signal: {}", e))
    }
    
    #[cfg(not(unix))]
    {
        Err("Signal handling not supported on this platform".to_string())
    }
}
