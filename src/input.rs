use crate::history::History;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::Print,
    terminal::{self, ClearType},
};
use std::io::{self, Write};
use std::sync::Once;
use std::fs;
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
    history_index: Option<usize>,
}

impl LineEditor {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_pos: 0,
            history_index: None,
        }
    }

    pub fn read_line(&mut self, prompt: &str, history: &mut History) -> io::Result<String> {
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_index = None;

        let mut stdout = io::stdout();
        let _guard = RawModeGuard::enter()?;

        execute!(stdout, Print(prompt))?;
        stdout.flush()?;

        loop {
            if let Event::Key(key_event) = event::read()? {
                match key_event {
                    // Enter - submit line
                    KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    } => {
                        execute!(stdout, Print("\r\n"))?;
                        break;
                    }

                    // Backspace
                    KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    } => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            self.buffer.remove(self.cursor_pos);
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

                    // Left arrow
                    KeyEvent {
                        code: KeyCode::Left,
                        ..
                    } => {
                        if self.cursor_pos > 0 {
                            self.cursor_pos -= 1;
                            execute!(stdout, cursor::MoveLeft(1))?;
                        }
                    }

                    // Right arrow
                    KeyEvent {
                        code: KeyCode::Right,
                        ..
                    } => {
                        if self.cursor_pos < self.buffer.len() {
                            self.cursor_pos += 1;
                            execute!(stdout, cursor::MoveRight(1))?;
                        }
                    }

                    // Up arrow - previous history
                    KeyEvent {
                        code: KeyCode::Up,
                        ..
                    } => {
                        if let Some(entry) = history.previous() {
                            self.buffer = entry.clone();
                            self.cursor_pos = self.buffer.chars().count();
                            self.redraw(prompt)?;
                        }
                    }

                    // Down arrow - next history
                    KeyEvent {
                        code: KeyCode::Down,
                        ..
                    } => {
                        if let Some(entry) = history.next() {
                            self.buffer = entry.clone();
                            self.cursor_pos = self.buffer.chars().count();
                            self.redraw(prompt)?;
                        } else {
                            self.buffer.clear();
                            self.cursor_pos = 0;
                            self.history_index = None;
                            self.redraw(prompt)?;
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
                        self.cursor_pos = self.buffer.chars().count();
                        self.redraw(prompt)?;
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

                    // Ctrl+C - cancel current line
                    KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        // Clear the current line and start fresh
                        self.buffer.clear();
                        self.cursor_pos = 0;
                        execute!(stdout, Print("\r\n"))?;
                        break;
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

                    // Regular character input
                    KeyEvent {
                        code: KeyCode::Char(c),
                        modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                        ..
                    } => {
                        self.buffer.insert(
                            self.buffer
                                .char_indices()
                                .nth(self.cursor_pos)
                                .map(|(i, _)| i)
                                .unwrap_or(self.buffer.len()),
                            c,
                        );
                        self.cursor_pos += 1;
                        self.redraw(prompt)?;
                    }

                    _ => {}
                }
            }
        }

        Ok(self.buffer.clone())
    }

    fn redraw(&self, prompt: &str) -> io::Result<()> {
        let mut stdout = io::stdout();
        execute!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine),
            Print(prompt),
            Print(&self.buffer),
        )?;

        let prompt_len = prompt.chars().count();
        let target_col = (prompt_len + self.cursor_pos) as u16;
        execute!(stdout, cursor::MoveToColumn(target_col))?;
        stdout.flush()?;
        Ok(())
    }

    // --- completion helpers ------------------------------------------------

    fn handle_tab_completion(&mut self, prompt: &str) -> io::Result<bool> {
        // find token start (last whitespace before cursor)
        let token_start = self.buffer[..self.byte_index_at_char_pos(self.cursor_pos)]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        
        let token_start_char = self.buffer[..token_start].chars().count();
        let token = &self.buffer[token_start..self.byte_index_at_char_pos(self.cursor_pos)];

        if token.is_empty() {
            return Ok(false);
        }

        // if token looks like a path (contains '/'), do path completion
        if token.contains('/') {
            if let Some((dir, prefix)) = split_dir_prefix(token) {
                let matches = list_dir_matches(&dir, &prefix)?;
                if matches.is_empty() {
                    return Ok(false);
                }
                
                // Show all matches if multiple
                if matches.len() > 1 {
                    self.show_completions(&matches, prompt)?;
                }
                
                let common = common_prefix(&matches);
                
                // Replace with common prefix
                if common.len() > prefix.len() {
                    self.buffer.drain(token_start..self.byte_index_at_char_pos(self.cursor_pos));
                    let full_completion = if dir == "." {
                        common.clone()
                    } else {
                        format!("{}/{}", dir, common)
                    };
                    self.buffer.insert_str(token_start, &full_completion);
                    self.cursor_pos = token_start_char + full_completion.chars().count();
                    return Ok(true);
                }
                
                // If only one match, complete it fully
                if matches.len() == 1 {
                    let first = &matches[0];
                    self.buffer.drain(token_start..self.byte_index_at_char_pos(self.cursor_pos));
                    let full_completion = if dir == "." {
                        first.clone()
                    } else {
                        format!("{}/{}", dir, first)
                    };
                    self.buffer.insert_str(token_start, &full_completion);
                    self.cursor_pos = token_start_char + full_completion.chars().count();
                    return Ok(true);
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
                
                if matches.len() > 1 {
                    self.show_completions(&matches, prompt)?;
                }
                
                let common = common_prefix(&matches);
                if common.len() > token.len() {
                    self.buffer.drain(token_start..self.byte_index_at_char_pos(self.cursor_pos));
                    self.buffer.insert_str(token_start, &common);
                    self.cursor_pos = token_start_char + common.chars().count();
                    return Ok(true);
                }
                
                if matches.len() == 1 {
                    let first = &matches[0];
                    self.buffer.drain(token_start..self.byte_index_at_char_pos(self.cursor_pos));
                    self.buffer.insert_str(token_start, first);
                    self.cursor_pos = token_start_char + first.chars().count();
                    return Ok(true);
                }
                
                return Ok(false);
            } else {
                // not first token: filename completion in CWD by default
                let matches = list_dir_matches(".", token)?;
                if matches.is_empty() {
                    return Ok(false);
                }
                
                if matches.len() > 1 {
                    self.show_completions(&matches, prompt)?;
                }
                
                let common = common_prefix(&matches);
                if common.len() > token.len() {
                    self.buffer.drain(token_start..self.byte_index_at_char_pos(self.cursor_pos));
                    self.buffer.insert_str(token_start, &common);
                    self.cursor_pos = token_start_char + common.chars().count();
                    return Ok(true);
                }
                
                if matches.len() == 1 {
                    let first = &matches[0];
                    self.buffer.drain(token_start..self.byte_index_at_char_pos(self.cursor_pos));
                    self.buffer.insert_str(token_start, first);
                    self.cursor_pos = token_start_char + first.chars().count();
                    return Ok(true);
                }
                
                return Ok(false);
            }
        }
        Ok(false)
    }

    fn show_completions(&self, matches: &[String], prompt: &str) -> io::Result<()> {
    let mut stdout = io::stdout();
    
    // Clear current input line
    execute!(stdout, cursor::MoveToColumn(0))?;
    execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;

    // Print completions followed by a blank line for separation
    if !matches.is_empty() {
        let output = matches.join("    ");
        println!("{}", output);
    }

    // Carriage return + newline to ensure we're at start of next line
    print!("\r\n{}{}", prompt, &self.buffer);
    
    // Move cursor to correct position
    let total_pos = prompt.chars().count() + self.cursor_pos;
    execute!(stdout, cursor::MoveToColumn(total_pos as u16))?;
    
    stdout.flush()?;
    Ok(())
}

    fn byte_index_at_char_pos(&self, char_pos: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }
}

// --- helper functions ------------------------------------------------------

fn split_dir_prefix(path: &str) -> Option<(String, String)> {
    if let Some(idx) = path.rfind('/') {
        let dir = if idx == 0 {
            "/".to_string()
        } else {
            path[..idx].to_string()
        };
        let prefix = path[idx + 1..].to_string();
        Some((dir, prefix))
    } else {
        None
    }
}

fn list_dir_matches(dir: &str, prefix: &str) -> io::Result<Vec<String>> {
    let mut matches = Vec::new();
    let entries = fs::read_dir(dir)?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(prefix) {
            if entry.path().is_dir() {
                matches.push(format!("{}/", name));
            } else {
                matches.push(name);
            }
        }
    }
    matches.sort();
    Ok(matches)
}

fn list_path_commands(prefix: &str) -> io::Result<Vec<String>> {
    let mut matches = Vec::new();
    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(prefix) {
                        #[cfg(unix)]
                        {
                            if let Ok(meta) = entry.metadata() {
                                if meta.permissions().mode() & 0o111 != 0 {
                                    matches.push(name);
                                }
                            }
                        }
                        #[cfg(not(unix))]
                        {
                            matches.push(name);
                        }
                    }
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
    let first = &strings[0];
    let mut prefix_len = first.len();
    for s in &strings[1..] {
        prefix_len = prefix_len.min(
            first
                .chars()
                .zip(s.chars())
                .take_while(|(a, b)| a == b)
                .count(),
        );
    }
    first.chars().take(prefix_len).collect()
}
