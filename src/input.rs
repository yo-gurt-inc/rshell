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

        // Print prompt and ensure cursor is at the right position
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
                            self.update_cursor_position(prompt)?;
                        }
                    }

                    // Right arrow
                    KeyEvent {
                        code: KeyCode::Right,
                        ..
                    } => {
                        if self.cursor_pos < self.buffer.len() {
                            self.cursor_pos += 1;
                            self.update_cursor_position(prompt)?;
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
                        self.update_cursor_position(prompt)?;
                    }

                    // End - move to end
                    KeyEvent {
                        code: KeyCode::End,
                        ..
                    } => {
                        self.cursor_pos = self.buffer.chars().count();
                        self.update_cursor_position(prompt)?;
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

                    // Ctrl+A - Move to start of line
                    KeyEvent {
                        code: KeyCode::Char('a'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.cursor_pos = 0;
                        self.update_cursor_position(prompt)?;
                    }

                    // Ctrl+E - Move to end of line  
                    KeyEvent {
                        code: KeyCode::Char('e'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.cursor_pos = self.buffer.chars().count();
                        self.update_cursor_position(prompt)?;
                    }

                    // Ctrl+K - Kill line from cursor to end
                    KeyEvent {
                        code: KeyCode::Char('k'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        self.buffer.truncate(self.byte_index_at_char_pos(self.cursor_pos));
                        self.redraw(prompt)?;
                    }

                    // Ctrl+U - Kill line from start to cursor
                    KeyEvent {
                        code: KeyCode::Char('u'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        let bytes_to_remove = self.byte_index_at_char_pos(self.cursor_pos);
                        self.buffer.drain(0..bytes_to_remove);
                        self.cursor_pos = 0;
                        self.redraw(prompt)?;
                    }

                    // Ctrl+W - Delete word before cursor
                    KeyEvent {
                        code: KeyCode::Char('w'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        if self.cursor_pos > 0 {
                            let mut word_end = self.cursor_pos;
                            
                            // Skip whitespace backwards
                            while word_end > 0 {
                                let prev_char = self.buffer.chars().nth(word_end - 1).unwrap();
                                if !prev_char.is_whitespace() {
                                    break;
                                }
                                word_end -= 1;
                            }
                            
                            // Find start of word
                            let mut word_start = word_end;
                            while word_start > 0 {
                                let prev_char = self.buffer.chars().nth(word_start - 1).unwrap();
                                if prev_char.is_whitespace() {
                                    break;
                                }
                                word_start -= 1;
                            }
                            
                            // Remove the word
                            let byte_start = self.byte_index_at_char_pos(word_start);
                            let byte_end = self.byte_index_at_char_pos(word_end);
                            self.buffer.drain(byte_start..byte_end);
                            self.cursor_pos = word_start;
                            self.redraw(prompt)?;
                        }
                    }

                    // Ctrl+L - Clear screen
                    KeyEvent {
                        code: KeyCode::Char('l'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        execute!(io::stdout(), terminal::Clear(ClearType::All))?;
                        self.redraw(prompt)?;
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

                    // Ctrl+Y - Paste (yank) killed text (basic implementation)
                    KeyEvent {
                        code: KeyCode::Char('y'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        // For now, just beep since we don't have kill ring implemented
                        execute!(stdout, Print("\x07"))?; // Bell character
                    }

                    // Ctrl+T - Transpose characters
                    KeyEvent {
                        code: KeyCode::Char('t'),
                        modifiers: KeyModifiers::CONTROL,
                        ..
                    } => {
                        if self.cursor_pos > 0 && self.cursor_pos < self.buffer.chars().count() {
                            let left_idx = self.byte_index_at_char_pos(self.cursor_pos - 1);
                            let right_idx = self.byte_index_at_char_pos(self.cursor_pos);
                            let left_char = self.buffer.chars().nth(self.cursor_pos - 1).unwrap();
                            let right_char = self.buffer.chars().nth(self.cursor_pos).unwrap();
                            
                            self.buffer.replace_range(left_idx..right_idx, &format!("{}{}", right_char, left_char));
                            self.cursor_pos += 1;
                            self.redraw(prompt)?;
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
        
        // Move to beginning of line and redraw everything
        execute!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::UntilNewLine),
            Print(prompt),
            Print(&self.buffer),
        )?;

        // Position cursor correctly - use visual length (without ANSI codes)
        let visual_prompt_len = Self::visual_length(prompt);
        let total_chars = visual_prompt_len + self.cursor_pos;
        execute!(stdout, cursor::MoveToColumn(total_chars as u16))?;
        stdout.flush()?;
        Ok(())
    }

    fn update_cursor_position(&self, prompt: &str) -> io::Result<()> {
        let mut stdout = io::stdout();
        let visual_prompt_len = Self::visual_length(prompt);
        let total_chars = visual_prompt_len + self.cursor_pos;
        execute!(stdout, cursor::MoveToColumn(total_chars as u16))?;
        stdout.flush()?;
        Ok(())
    }

    /// Calculate visual length of string by stripping ANSI escape codes
    fn visual_length(s: &str) -> usize {
        let mut in_escape = false;
        let mut length = 0;
        
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
                continue;
            }
            if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
                continue;
            }
            length += 1;
        }
        length
    }

    // ... (rest of your methods) ...
    fn handle_tab_completion(&mut self, prompt: &str) -> io::Result<bool> {
        Ok(false)
    }

    fn show_completions(&self, matches: &[String], prompt: &str) -> io::Result<()> {
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

// ... (rest of your helper functions) ...
