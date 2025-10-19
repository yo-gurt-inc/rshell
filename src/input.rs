use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use std::io::{self, Write};
use std::sync::Once;
use std::fs;
use std::path::Path;
use std::env;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

static SET_PANIC_HOOK: Once = Once::new();

struct RawModeGuard;

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

pub struct LineEditor {
    buffer: String,
    cursor_pos: usize,
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
        }
    }

    pub fn read_line(&mut self, prompt: &str, history: &mut crate::history::History) -> io::Result<String> {
        // enter raw mode and ensure it will be restored
        let raw = RawModeGuard::enter()?; // keep ownership to drop explicitly later
        let mut stdout = io::stdout();

        self.buffer.clear();
        self.cursor_pos = 0;

        // ensure line is cleared and prompt is printed at column 0
        stdout.execute(cursor::MoveToColumn(0))?;
        stdout.execute(terminal::Clear(ClearType::CurrentLine))?;
        write!(stdout, "{}", prompt)?;
        stdout.flush()?;

        loop {
            if let Event::Key(key_event) = event::read()? {
                match key_event {
                    // Enter - submit (don't print newline while still in raw mode)
                    KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    } => {
                        break;
                    }

                    // Tab - attempt completion
                    KeyEvent {
                        code: KeyCode::Tab,
                        ..
                    } => {
                        if self.handle_tab_completion(prompt)? {
                            // buffer changed, redraw
                            self.redraw(prompt)?;
                        }
                    }


                    // Ctrl+C - cancel
                    KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        // process::exit bypasses Drop, so restore first
                        let _ = terminal::disable_raw_mode();
                        std::process::exit(0);
                    }

                    // Ctrl+D - EOF / exit if buffer empty
                    KeyEvent {
                        code: KeyCode::Char('d'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        if self.buffer.is_empty() {
                            // restore then exit
                            let _ = terminal::disable_raw_mode();
                            println!();
                            std::process::exit(0);
                        }
                    }

                    // Ctrl+A - move to start
                    KeyEvent {
                        code: KeyCode::Char('a'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.cursor_pos = 0;
                        self.redraw(prompt)?;
                    }

                    // Ctrl+E - move to end
                    KeyEvent {
                        code: KeyCode::Char('e'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.cursor_pos = self.buffer.len();
                        self.redraw(prompt)?;
                    }

                    // Ctrl+K - delete from cursor to end
                    KeyEvent {
                        code: KeyCode::Char('k'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.buffer.truncate(self.cursor_pos);
                        self.redraw(prompt)?;
                    }

                    // Ctrl+U - delete from cursor to start
                    KeyEvent {
                        code: KeyCode::Char('u'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.buffer.drain(..self.cursor_pos);
                        self.cursor_pos = 0;
                        self.redraw(prompt)?;
                    }

                    // Ctrl+W - delete word backward
                    KeyEvent {
                        code: KeyCode::Char('w'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        if self.cursor_pos > 0 {
                            let start = self.buffer[..self.cursor_pos]
                                .rfind(|c: char| c.is_whitespace())
                                .map(|i| i + 1)
                                .unwrap_or(0);
                            self.buffer.drain(start..self.cursor_pos);
                            self.cursor_pos = start;
                            self.redraw(prompt)?;
                        }
                    }

                    // Arrow Up - previous history
                    KeyEvent {
                        code: KeyCode::Up,
                        ..
                    } => {
                        if let Some(cmd) = history.previous() {
                            self.buffer = cmd.clone();
                            self.cursor_pos = self.buffer.len();
                            self.redraw(prompt)?;
                        }
                    }

                    // Arrow Down - next history
                    KeyEvent {
                        code: KeyCode::Down,
                        ..
                    } => {
                        if let Some(cmd) = history.next() {
                            self.buffer = cmd.clone();
                            self.cursor_pos = self.buffer.len();
                            self.redraw(prompt)?;
                        } else {
                            self.buffer.clear();
                            self.cursor_pos = 0;
                            self.redraw(prompt)?;
                        }
                    }

                    // Arrow Left
                    KeyEvent {
                        code: KeyCode::Left,
                        ..
                    } => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            stdout.execute(cursor::MoveLeft(1))?;
                            stdout.flush()?;
                        }
                    }

                    // Arrow Right
                    KeyEvent {
                        code: KeyCode::Right,
                        ..
                    } => {
                        if self.cursor_pos < self.buffer.len() {
                            self.cursor_pos += 1;
                            stdout.execute(cursor::MoveRight(1))?;
                            stdout.flush()?;
                        }
                    }

                    // Home - move to start
                    KeyEvent {
                        code: KeyCode::Home,
                        ..
                    } => {
                        self.cursor_pos = 0;
                        self.redraw(prompt)?;
                    }

                    // End - move to end
                    KeyEvent {
                        code: KeyCode::End,
                        ..
                    } => {
                        self.cursor_pos = self.buffer.len();
                        self.redraw(prompt)?;
                    }

                    // Backspace
                    KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    } => {
                        if self.cursor_pos > 0 {
                            self.buffer.remove(self.cursor_pos - 1);
                            self.cursor_pos -= 1;
                            self.redraw(prompt)?;
                        }
                    }

                    // Delete
                    KeyEvent {
                        code: KeyCode::Delete,
                        ..
                    } => {
                        if self.cursor_pos < self.buffer.len() {
                            self.buffer.remove(self.cursor_pos);
                            self.redraw(prompt)?;
                        }
                            }

                    // Regular character
                    KeyEvent {
                        code: KeyCode::Char(c),
                        ..
                    } => {
                        self.buffer.insert(self.cursor_pos, c);
                        self.cursor_pos += 1;
                        self.redraw(prompt)?;
                    }

                    _ => {}
                }
            }
        }

        // drop raw mode before printing the final newline / returning
        drop(raw);
        println!();
        Ok(self.buffer.clone())
    }

    fn redraw(&self, prompt: &str) -> io::Result<()> {
        let mut stdout = io::stdout();

        // Move to start of line and clear
        stdout.execute(cursor::MoveToColumn(0))?;
        stdout.execute(terminal::Clear(ClearType::CurrentLine))?;

        // Print prompt and buffer
        write!(stdout, "{}{}", prompt, self.buffer)?;

        // Move cursor to correct position
        let prompt_len = prompt.chars().count() as u16;
        stdout.execute(cursor::MoveToColumn(prompt_len + self.cursor_pos as u16))?;
        stdout.flush()?;

        Ok(())
    }

    // --- completion helpers ------------------------------------------------

    fn handle_tab_completion(&mut self, _prompt: &str) -> io::Result<bool> {
        // find token start (last whitespace before cursor)
        let token_start = self.buffer[..self.cursor_pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let token = &self.buffer[token_start..self.cursor_pos];

        if token.is_empty() {
            return Ok(false);
        }

        // if token looks like a path (contains '/'), do path completion
        if token.contains('/') {
            if let Some((dir, prefix)) = split_dir_prefix(token) {
                let matches = list_dir_matches(&dir, prefix)?;
                if matches.is_empty() {
                    return Ok(false);
                }
                let common = common_prefix(&matches);
                // insert the remaining part of the common prefix
                if common.len() > prefix.len() {
                    let to_insert = &common[prefix.len()..];
                    self.buffer.insert_str(self.cursor_pos, to_insert);
                    self.cursor_pos += to_insert.chars().count();
                    return self.redraw_and_return(true);
                }
                // otherwise insert first match remainder
                let first = &matches[0];
                if first.len() > prefix.len() {
                    let to_insert = &first[prefix.len()..];
                    self.buffer.insert_str(self.cursor_pos, to_insert);
                    self.cursor_pos += to_insert.chars().count();
                    return self.redraw_and_return(true);
                }
                return Ok(false);
            }
        } else {
            // first token -> complete executables in PATH
            let is_first = token_start == 0;
            if is_first {
                let matches = list_path_commands(token)?;
                if matches.is_empty() {
                    return Ok(false);
                }
                let common = common_prefix(&matches);
                if common.len() > token.len() {
                    let to_insert = &common[token.len()..];
                    self.buffer.insert_str(self.cursor_pos, to_insert);
                    self.cursor_pos += to_insert.chars().count();
                    return self.redraw_and_return(true);
                }
                let first = &matches[0];
                if first.len() > token.len() {
                    let to_insert = &first[token.len()..];
                    self.buffer.insert_str(self.cursor_pos, to_insert);
                    self.cursor_pos += to_insert.chars().count();
                    return self.redraw_and_return(true);
                }
                return Ok(false);
            } else {
                // not first token: filename completion in CWD by default
                let matches = list_dir_matches(".", token)?;
                if matches.is_empty() {
                    return Ok(false);
                }
                let common = common_prefix(&matches);
                if common.len() > token.len() {
                    let to_insert = &common[token.len()..];
                    self.buffer.insert_str(self.cursor_pos, to_insert);
                    self.cursor_pos += to_insert.chars().count();
                    return self.redraw_and_return(true);
                }
                let first = &matches[0];
                if first.len() > token.len() {
                    let to_insert = &first[token.len()..];
                    self.buffer.insert_str(self.cursor_pos, to_insert);
                    self.cursor_pos += to_insert.chars().count();
                    return self.redraw_and_return(true);
                }
                return Ok(false);
            }
        }
        Ok(false)
    }

    fn redraw_and_return(&self, changed: bool) -> io::Result<bool> {
        // redraw using an empty prompt here because caller already has prompt;
        // caller will typically call redraw(prompt) after this, but we can redraw now by returning Ok(true)
        // to indicate buffer changed; the caller's event loop will redraw next iteration.
        // For immediate redraw, we do nothing here and let caller call redraw.
        Ok(changed)
    }
}

// --- filesystem / PATH helpers --------------------------------------------

fn split_dir_prefix(token: &str) -> Option<(String, &str)> {
    // token includes at least one '/'
    if let Some(pos) = token.rfind('/') {
        let (dir, _pref) = token.split_at(pos + 1);
        // dir is e.g. "src/" or "/usr/bi"
        // pref is the part after the last '/'
        Some((dir.to_string(), &token[pos + 1..]))
    } else {
        None
    }
}

fn list_dir_matches(dir_part: &str, prefix: &str) -> io::Result<Vec<String>> {
    let path = Path::new(dir_part);
    let dir_to_read = if path.is_absolute() || dir_part.starts_with("./") || dir_part.starts_with("../") {
        Path::new(dir_part).parent().unwrap_or(Path::new(dir_part)).to_path_buf()
    } else {
        // dir_part may be "foo/" relative to cwd
        Path::new(dir_part).to_path_buf()
    };

    let read_dir = if dir_to_read.exists() && dir_to_read.is_dir() {
        fs::read_dir(&dir_to_read)
    } else {
        // try to interpret full token as a directory (e.g. "foo/" where foo doesn't exist yet)
        fs::read_dir(".")
    };

    let mut matches = Vec::new();
    if let Ok(entries) = read_dir {
        for entry in entries.flatten() {
            if let Ok(name_os) = entry.file_name().into_string() {
                if name_os.starts_with(prefix) {
                    let mut candidate = format!("{}{}", dir_part, name_os);
                    // add trailing slash if directory
                    if entry.path().is_dir() {
                        candidate.push('/');
                    }
                    matches.push(candidate);
                }
            }
        }
    }
    matches.sort();
    matches.dedup();
    Ok(matches)
}

fn common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let mut prefix = strings[0].clone();
    for s in strings.iter().skip(1) {
        let mut new_pref = String::new();
        for (a, b) in prefix.chars().zip(s.chars()) {
            if a == b {
                new_pref.push(a);
            } else {
                break;
            }
        }
        prefix = new_pref;
        if prefix.is_empty() {
            break;
        }
    }
    prefix
}

fn list_path_commands(prefix: &str) -> io::Result<Vec<String>> {
    let mut cmds = Vec::new();
    if let Ok(pathvar) = env::var("PATH") {
        for p in env::split_paths(&pathvar) {
            if let Ok(entries) = fs::read_dir(p) {
                for e in entries.flatten() {
                    if let Ok(name) = e.file_name().into_string() {
                        if name.starts_with(prefix) {
                            #[cfg(unix)]
                            {
                                if let Ok(meta) = e.metadata() {
                                    if meta.is_file() && (meta.permissions().mode() & 0o111 != 0) {
                                        cmds.push(name.clone());
                                    }
                                }
                            }
                            #[cfg(not(unix))]
                            {
                                // on non-unix just include the name
                                cmds.push(name.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    cmds.sort();
    cmds.dedup();
    Ok(cmds)
}