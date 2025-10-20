use crate::jobs::JobManager;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};
use std::io::{Read, Write};

#[derive(Debug)]
pub enum Command {
    Cd(Option<String>),
    Pwd,
    Echo(Vec<String>),
    Exit,
    Help,
    Ls(Option<String>),
    Cat(String),
    Mkdir(String),
    Rm(String),
    Touch(String),
    Clear,
    History,
    Jobs,
    Fg(u32),
    Bg(u32),
    External {
        program: String,
        args: Vec<String>,
        background: bool,
    },
}

impl Command {
    /// Parse command string with support for quotes and parentheses
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        // Handle subshells/parentheses first
        let input = match Self::expand_subshells(input) {
            Ok(expanded) => expanded,
            Err(e) => {
                eprintln!("Error: {}", e);
                return None;
            }
        };

        let background = input.ends_with('&');
        let input = if background {
            input[..input.len() - 1].trim()
        } else {
            input.as_str()
        };

        // Parse with quote handling
        let parts = Self::parse_args(input);
        
        // Empty command (e.g., just quotes: "" or '')
        if parts.is_empty() {
            return None;
        }
        
        // Handle lone backslash or other edge cases
        if parts.len() == 1 && (parts[0] == "\\" || parts[0].is_empty()) {
            return None;
        }

        let cmd = &parts[0];
        let args: Vec<String> = parts[1..].to_vec();

        match cmd.as_str() {
            "cd" => Some(Command::Cd(args.first().cloned())),
            "pwd" => Some(Command::Pwd),
            "echo" => Some(Command::Echo(args)),
            "exit" => Some(Command::Exit),
            "help" => Some(Command::Help),
            "ls" => Some(Command::Ls(args.first().cloned())),
            "cat" => {
                if args.is_empty() {
                    eprintln!("cat: missing file operand");
                    None
                } else {
                    Some(Command::Cat(args[0].clone()))
                }
            }
            "mkdir" => {
                if args.is_empty() {
                    eprintln!("mkdir: missing operand");
                    None
                } else {
                    Some(Command::Mkdir(args[0].clone()))
                }
            }
            "rm" => {
                if args.is_empty() {
                    eprintln!("rm: missing operand");
                    None
                } else {
                    Some(Command::Rm(args[0].clone()))
                }
            }
            "touch" => {
                if args.is_empty() {
                    eprintln!("touch: missing file operand");
                    None
                } else {
                    Some(Command::Touch(args[0].clone()))
                }
            }
            "clear" => Some(Command::Clear),
            "history" => Some(Command::History),
            "jobs" => Some(Command::Jobs),
            "fg" => {
                let job_id = args.first().and_then(|s| s.parse().ok()).unwrap_or(1);
                Some(Command::Fg(job_id))
            }
            "bg" => {
                let job_id = args.first().and_then(|s| s.parse().ok()).unwrap_or(1);
                Some(Command::Bg(job_id))
            }
            _ => Some(Command::External {
                program: cmd.clone(),
                args,
                background,
            }),
        }
    }

    /// Parse arguments with quote handling
    /// Returns (args, needs_continuation)
    pub fn parse_args_with_state(input: &str) -> (Vec<String>, bool) {
        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut in_quotes = false;
        let mut quote_char = ' ';
        let mut chars = input.chars().peekable();
        let mut escape_next = false;

        while let Some(c) = chars.next() {
            if escape_next {
                // Handle escaped character
                current_arg.push(match c {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    '\'' => '\'',
                    _ => c,
                });
                escape_next = false;
                continue;
            }

            match c {
                // Escape character
                '\\' if in_quotes || chars.peek().is_some() => {
                    escape_next = true;
                }
                // Opening quote
                '"' | '\'' if !in_quotes => {
                    in_quotes = true;
                    quote_char = c;
                }
                // Closing quote
                '"' | '\'' if in_quotes && c == quote_char => {
                    in_quotes = false;
                    quote_char = ' ';
                }
                // Newline inside quotes - keep it
                '\n' if in_quotes => {
                    current_arg.push(c);
                }
                // Space outside quotes = separator
                ' ' if !in_quotes => {
                    if !current_arg.is_empty() {
                        args.push(current_arg.clone());
                        current_arg.clear();
                    }
                }
                // Regular character
                _ => current_arg.push(c),
            }
        }

        // Push last argument
        if !current_arg.is_empty() {
            args.push(current_arg);
        }

        // Return whether we need continuation (unclosed quotes)
        (args, in_quotes)
    }

    /// Parse arguments with quote handling (original function)
    fn parse_args(input: &str) -> Vec<String> {
        Self::parse_args_with_state(input).0
    }

    /// Check if input needs line continuation (only for unclosed quotes)
    pub fn needs_line_continuation(input: &str) -> bool {
        // Check for unclosed quotes only
        let (_, in_quotes) = Self::parse_args_with_state(input);
        in_quotes
    }

    /// Expand subshells (commands in parentheses)
    fn expand_subshells(input: &str) -> Result<String, String> {
        let mut result = String::new();
        let mut chars = input.chars().peekable();
        let mut depth = 0;
        let mut subshell = String::new();

        while let Some(c) = chars.next() {
            match c {
                '(' => {
                    depth += 1;
                    if depth > 1 {
                        subshell.push(c);
                    }
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        // Execute subshell and capture output
                        let output = Self::execute_subshell(&subshell)?;
                        result.push_str(&output);
                        subshell.clear();
                    } else if depth > 0 {
                        subshell.push(c);
                    } else {
                        return Err("Unmatched closing parenthesis".to_string());
                    }
                }
                _ => {
                    if depth > 0 {
                        subshell.push(c);
                    } else {
                        result.push(c);
                    }
                }
            }
        }

        if depth > 0 {
            return Err("Unmatched opening parenthesis".to_string());
        }

        Ok(result)
    }

    /// Execute a subshell command and return its output
    fn execute_subshell(cmd: &str) -> Result<String, String> {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return Ok(String::new());
        }

        // Parse the subshell command
        let args = Self::parse_args(cmd);
        if args.is_empty() {
            return Ok(String::new());
        }

        let program = &args[0];
        let cmd_args = &args[1..];

        // Execute command and capture output
        let output = ProcessCommand::new(program)
            .args(cmd_args)
            .output()
            .map_err(|e| format!("Failed to execute '{}': {}", program, e))?;

        if !output.status.success() {
            return Err(format!(
                "'{}' failed with exit code {}",
                program,
                output.status.code().unwrap_or(-1)
            ));
        }

        // Return stdout, trimming trailing newline
        let mut result = String::from_utf8_lossy(&output.stdout).to_string();
        if result.ends_with('\n') {
            result.pop();
        }

        Ok(result)
    }

    pub fn execute(&self, job_manager: &mut JobManager) -> bool {
        match self {
            Command::Cd(path) => {
                let home = env::var("HOME").unwrap_or_else(|_| "/".to_string());
                let target = path.as_deref().unwrap_or(&home);

                if let Err(e) = env::set_current_dir(target) {
                    eprintln!("cd: {}", e);
                }
            }

            Command::Pwd => {
                if let Ok(path) = env::current_dir() {
                    println!("{}", path.display());
                }
            }

            Command::Echo(args) => {
                println!("{}", args.join(" "));
            }

            Command::Exit => {
                return false;
            }

            Command::Help => {
                println!("Available commands:");
                println!("  cd [path]       - Change directory");
                println!("  pwd             - Print working directory");
                println!("  ls [path]       - List directory contents");
                println!("  cat <file>      - Display file contents");
                println!("  mkdir <dir>     - Create directory");
                println!("  rm <file>       - Remove file");
                println!("  touch <file>    - Create empty file");
                println!("  echo [args...]  - Print arguments");
                println!("  clear           - Clear screen");
                println!("  history         - Show command history");
                println!("  jobs            - List background jobs");
                println!("  fg [job_id]     - Bring job to foreground");
                println!("  bg [job_id]     - Resume job in background");
                println!("  exit            - Exit shell");
                println!("\nFeatures:");
                println!("  - Quotes: echo \"hello world\" or echo 'single quotes'");
                println!("  - Subshells: echo $(pwd) or echo $(ls)");
                println!("  - Background: command &");
                println!("  - Pipes: command1 | command2");
            }

            Command::Ls(path) => {
                let target = path.as_deref().unwrap_or(".");
                match fs::read_dir(target) {
                    Ok(entries) => {
                        let mut items: Vec<_> = entries
                            .flatten()
                            .map(|entry| {
                                let name = entry.file_name().to_string_lossy().to_string();
                                let is_dir = entry.path().is_dir();
                                (name, is_dir)
                            })
                            .collect();

                        items.sort_by(|a, b| a.0.cmp(&b.0));

                        for (i, (name, is_dir)) in items.iter().enumerate() {
                            if *is_dir {
                                print!("\x1b[34m{:<20}\x1b[0m", name);
                            } else {
                                print!("{:<20}", name);
                            }

                            if (i + 1) % 4 == 0 {
                                println!();
                            }
                        }
                        println!();
                    }
                    Err(e) => eprintln!("ls: {}", e),
                }
            }

            Command::Cat(file) => match fs::read_to_string(file) {
                Ok(contents) => print!("{}", contents),
                Err(e) => eprintln!("cat: {}: {}", file, e),
            },

            Command::Mkdir(dir) => {
                if let Err(e) = fs::create_dir(dir) {
                    eprintln!("mkdir: {}", e);
                }
            }

            Command::Rm(file) => {
                let path = PathBuf::from(file);
                let result = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
                if let Err(e) = result {
                    eprintln!("rm: {}", e);
                }
            }

            Command::Touch(file) => {
                if let Err(e) = fs::File::create(file) {
                    eprintln!("touch: {}", e);
                }
            }

            Command::Clear => {
                print!("\x1b[2J\x1b[H");
            }

            Command::External {
                program,
                args,
                background,
            } => {
                let mut cmd = ProcessCommand::new(program);
                cmd.args(args);

                if *background {
                    cmd.stdin(Stdio::null())
                        .stdout(Stdio::inherit())
                        .stderr(Stdio::inherit());

                    match cmd.spawn() {
                        Ok(child) => {
                            let pid = child.id();
                            let command_str = format!("{} {}", program, args.join(" "));
                            let job_id = job_manager.add_job(pid, command_str, child);
                            println!("[{}] {}", job_id, pid);
                        }
                        Err(e) => {
                            eprintln!("{}: {}", program, e);
                        }
                    }
                } else {
                    match cmd.status() {
                        Ok(status) => {
                            if !status.success() {
                                if let Some(code) = status.code() {
                                    eprintln!("{}: exited with code {}", program, code);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("{}: {}", program, e);
                        }
                    }
                }
            }

            Command::History | Command::Jobs | Command::Fg(_) | Command::Bg(_) => {
                // These are handled in shell.rs
            }
        }
        true
    }
}
