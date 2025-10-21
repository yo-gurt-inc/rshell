use std::io;
use std::sync::Once;
use crossterm::terminal;

static SET_PANIC_HOOK: Once = Once::new();

pub struct RawModeGuard;

impl RawModeGuard {
    pub fn enter() -> io::Result<Self> {
        // install panic hook once to restore terminal on panic
        SET_PANIC_HOOK.call_once(|| {
            let prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = terminal::disable_raw_mode();
                prev(info);
            }));
        });

        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
