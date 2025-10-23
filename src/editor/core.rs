use crate::history::History;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::Print,
    terminal::{self, ClearType},
};
use std::io::{self, Write};
use super::completion::*;
use super::raw_mode::RawModeGuard;

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
        loop {
            self.buffer.clear();
            self.cursor_pos = 0;
            self.history_index = None;

            let mut stdout = io::stdout();
            let _guard = RawModeGuard::enter()?;

            execute!(stdout, Print(prompt))?;
            stdout.flush()?;

            let mut ctrl_c_pressed = false;

            loop {
                if let Event::Key(key_event) = event::read()? {
                    match key_event {
                        KeyEvent {
                            code: KeyCode::Enter,
                            ..
                        } => {
                            execute!(stdout, Print("\r\n"))?;
                            return Ok(self.buffer.clone());
                        }

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

                        KeyEvent {
                            code: KeyCode::Delete,
                            ..
                        } => {
                            if self.cursor_pos < self.buffer.len() {
                                self.buffer.remove(self.cursor_pos);
                                self.redraw(prompt)?;
                            }
                        }

                        KeyEvent {
                            code: KeyCode::Left,
                            ..
                        } => {
                            if self.cursor_pos > 0 {
                                self.cursor_pos -= 1;
                                self.update_cursor_position(prompt)?;
                            }
                        }

                        KeyEvent {
                            code: KeyCode::Right,
                            ..
                        } => {
                            if self.cursor_pos < self.buffer.len() {
                                self.cursor_pos += 1;
                                self.update_cursor_position(prompt)?;
                            }
                        }

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

                        KeyEvent {
                            code: KeyCode::Home,
                            ..
                        } => {
                            self.cursor_pos = 0;
                            self.update_cursor_position(prompt)?;
                        }

                        KeyEvent {
                            code: KeyCode::End,
                            ..
                        } => {
                            self.cursor_pos = self.buffer.chars().count();
                            self.update_cursor_position(prompt)?;
                        }

                        KeyEvent {
                            code: KeyCode::Tab,
                            ..
                        } => {
                            if self.handle_tab_completion(prompt)? {
                                self.redraw(prompt)?;
                            }
                        }

                        KeyEvent {
                            code: KeyCode::Char('a'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            self.cursor_pos = 0;
                            self.update_cursor_position(prompt)?;
                        }

                        KeyEvent {
                            code: KeyCode::Char('e'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            self.cursor_pos = self.buffer.chars().count();
                            self.update_cursor_position(prompt)?;
                        }

                        KeyEvent {
                            code: KeyCode::Char('k'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            self.buffer.truncate(self.byte_index_at_char_pos(self.cursor_pos));
                            self.redraw(prompt)?;
                        }

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

                        KeyEvent {
                            code: KeyCode::Char('w'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            if self.cursor_pos > 0 {
                                let mut word_end = self.cursor_pos;

                                while word_end > 0 {
                                    let prev_char = self.buffer.chars().nth(word_end - 1).unwrap();
                                    if !prev_char.is_whitespace() {
                                        break;
                                    }
                                    word_end -= 1;
                                }

                                let mut word_start = word_end;
                                while word_start > 0 {
                                    let prev_char = self.buffer.chars().nth(word_start - 1).unwrap();
                                    if prev_char.is_whitespace() {
                                        break;
                                    }
                                    word_start -= 1;
                                }

                                let byte_start = self.byte_index_at_char_pos(word_start);
                                let byte_end = self.byte_index_at_char_pos(word_end);
                                self.buffer.drain(byte_start..byte_end);
                                self.cursor_pos = word_start;
                                self.redraw(prompt)?;
                            }
                        }

                        KeyEvent {
                            code: KeyCode::Char('l'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            execute!(io::stdout(), terminal::Clear(ClearType::All))?;
                            self.redraw(prompt)?;
                        }

                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            self.buffer.clear();
                            self.cursor_pos = 0;
                            execute!(stdout, Print("\r\n"))?;
                            ctrl_c_pressed = true;
                            break;
                        }

                        KeyEvent {
                            code: KeyCode::Char('d'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            if self.buffer.is_empty() {
                                drop(_guard);
                                println!();
                                std::process::exit(0);
                            }
                        }

                        KeyEvent {
                            code: KeyCode::Char('y'),
                            modifiers: KeyModifiers::CONTROL,
                            ..
                        } => {
                            execute!(stdout, Print("\x07"))?;
                        }

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

            if ctrl_c_pressed {
                continue;
            }
        }
    }

    fn redraw(&self, prompt: &str) -> io::Result<()> {
        let mut stdout = io::stdout();

        execute!(
            stdout,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::UntilNewLine),
            Print(prompt),
            Print(&self.buffer),
        )?;

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

    fn handle_tab_completion(&mut self, prompt: &str) -> io::Result<bool> {
        let token_start = self.buffer[..self.byte_index_at_char_pos(self.cursor_pos)]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        let token_start_char = self.buffer[..token_start].chars().count();
        let token = &self.buffer[token_start..self.byte_index_at_char_pos(self.cursor_pos)];

        if token.is_empty() {
            return Ok(false);
        }

        if token.contains('/') {
            if let Some((dir, prefix)) = split_dir_prefix(token) {
                let matches = list_dir_matches(&dir, &prefix)?;
                if matches.is_empty() {
                    return Ok(false);
                }

                if matches.len() > 1 {
                    self.show_completions(&matches, prompt)?;
                }

                let common = common_prefix(&matches);

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

        execute!(stdout, cursor::MoveToColumn(0))?;
        execute!(stdout, terminal::Clear(terminal::ClearType::CurrentLine))?;

        if !matches.is_empty() {
            let output = matches.join("    ");
            println!("{}", output);
        }

        print!("\r\n{}{}", prompt, &self.buffer);

        let visual_prompt_len = Self::visual_length(prompt);
        let total_chars = visual_prompt_len + self.cursor_pos;
        execute!(stdout, cursor::MoveToColumn(total_chars as u16))?;

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
