use std::fs::{File, OpenOptions};
use std::io;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub enum RedirectType {
    StdinFrom(String),
    StdoutTo(String),
    StdoutAppend(String),
    StderrTo(String),
    StderrAppend(String),
    BothTo(String),
}

#[derive(Debug)]
pub struct ParsedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub redirects: Vec<RedirectType>,
}

impl ParsedCommand {
    pub fn parse(input: &str) -> Self {
        let mut tokens = tokenize_with_redirects(input);
        let mut redirects = Vec::new();
        let mut cmd_parts = Vec::new();

        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].as_str() {
                "<" => {
                    if i + 1 < tokens.len() {
                        redirects.push(RedirectType::StdinFrom(tokens[i + 1].clone()));
                        i += 2;
                    } else {
                        eprintln!("Error: expected filename after '<'");
                        i += 1;
                    }
                }
                ">" => {
                    if i + 1 < tokens.len() {
                        redirects.push(RedirectType::StdoutTo(tokens[i + 1].clone()));
                        i += 2;
                    } else {
                        eprintln!("Error: expected filename after '>'");
                        i += 1;
                    }
                }
                ">>" => {
                    if i + 1 < tokens.len() {
                        redirects.push(RedirectType::StdoutAppend(tokens[i + 1].clone()));
                        i += 2;
                    } else {
                        eprintln!("Error: expected filename after '>>'");
                        i += 1;
                    }
                }
                "2>" => {
                    if i + 1 < tokens.len() {
                        redirects.push(RedirectType::StderrTo(tokens[i + 1].clone()));
                        i += 2;
                    } else {
                        eprintln!("Error: expected filename after '2>'");
                        i += 1;
                    }
                }
                "2>>" => {
                    if i + 1 < tokens.len() {
                        redirects.push(RedirectType::StderrAppend(tokens[i + 1].clone()));
                        i += 2;
                    } else {
                        eprintln!("Error: expected filename after '2>>'");
                        i += 1;
                    }
                }
                "&>" => {
                    if i + 1 < tokens.len() {
                        redirects.push(RedirectType::BothTo(tokens[i + 1].clone()));
                        i += 2;
                    } else {
                        eprintln!("Error: expected filename after '&>'");
                        i += 1;
                    }
                }
                _ => {
                    cmd_parts.push(tokens[i].clone());
                    i += 1;
                }
            }
        }

        let program = cmd_parts.first().cloned().unwrap_or_default();
        let args = if cmd_parts.len() > 1 {
            cmd_parts[1..].to_vec()
        } else {
            Vec::new()
        };

        ParsedCommand {
            program,
            args,
            redirects,
        }
    }

    pub fn execute(&self) -> io::Result<()> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);

        for redirect in &self.redirects {
            match redirect {
                RedirectType::StdinFrom(file) => {
                    let f = File::open(file)?;
                    cmd.stdin(Stdio::from(f));
                }
                RedirectType::StdoutTo(file) => {
                    let f = File::create(file)?;
                    cmd.stdout(Stdio::from(f));
                }
                RedirectType::StdoutAppend(file) => {
                    let f = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file)?;
                    cmd.stdout(Stdio::from(f));
                }
                RedirectType::StderrTo(file) => {
                    let f = File::create(file)?;
                    cmd.stderr(Stdio::from(f));
                }
                RedirectType::StderrAppend(file) => {
                    let f = OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(file)?;
                    cmd.stderr(Stdio::from(f));
                }
                RedirectType::BothTo(file) => {
                    let f = File::create(file)?;
                    let f2 = f.try_clone()?;
                    cmd.stdout(Stdio::from(f));
                    cmd.stderr(Stdio::from(f2));
                }
            }
        }

        let status = cmd.status()?;
        if !status.success() {
            if let Some(code) = status.code() {
                eprintln!("{}: exited with code {}", self.program, code);
            }
        }

        Ok(())
    }
}

fn tokenize_with_redirects(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = ' ';
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            '"' | '\'' if in_quotes && c == quote_char => {
                in_quotes = false;
            }
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            '>' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push(">>".to_string());
                } else {
                    tokens.push(">".to_string());
                }
            }
            '<' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                tokens.push("<".to_string());
            }
            '2' if !in_quotes && chars.peek() == Some(&'>') => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                chars.next();
                
                if chars.peek() == Some(&'>') {
                    chars.next();
                    tokens.push("2>>".to_string());
                } else {
                    tokens.push("2>".to_string());
                }
            }
            '&' if !in_quotes && chars.peek() == Some(&'>') => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
                chars.next();
                tokens.push("&>".to_string());
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}
