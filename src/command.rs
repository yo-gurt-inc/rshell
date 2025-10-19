use std::process::Command as ProcessCommand;
use std::env;
use std::fs;
use crate::variables;
use crate::jobs::JobManager;

pub enum Command {
    Cd(Option<String>),
    Pwd,
    Ls(Option<String>),
    Exit,
    Help,
    Echo(Vec<String>),
    Export(String, String),
    History,
    Jobs,
    Fg(u32),
    Bg(u32),
    Chain(Vec<ChainedCommand>),
    External(String, Vec<String>, bool),
}

#[derive(Debug, Clone)]
pub enum ChainType {
    And,        // &&
    Or,         // ||
    Semicolon,  // ;
}

#[derive(Debug, Clone)]
pub struct ChainedCommand {
    pub command: String,
    pub chain_type: Option<ChainType>,
}

impl Command {
    pub fn parse(input: &str) -> Option<Self> {
        let expanded = variables::expand_variables(input);

        // Check for command chaining
        if expanded.contains("&&") || expanded.contains("||") || expanded.contains(';') {
            return Some(Command::Chain(Self::parse_chain(&expanded)));
        }

        // Check if command should run in background
        let (expanded, background) = if expanded.trim().ends_with('&') {
            (expanded.trim_end_matches('&').trim().to_string(), true)
        } else {
            (expanded, false)
        };

        let mut parts = expanded.trim().split_whitespace();
        let cmd = parts.next()?;
        let args: Vec<String> = parts.map(|s| s.to_string()).collect();

        match cmd {
            "cd" => Some(Command::Cd(args.first().cloned())),
            "pwd" => Some(Command::Pwd),
            "ls" => Some(Command::Ls(args.first().cloned())),
            "exit" | "quit" => Some(Command::Exit),
            "help" => Some(Command::Help),
            "echo" => Some(Command::Echo(args)),
            "history" => Some(Command::History),
            "jobs" => Some(Command::Jobs),
            "fg" => {
                if let Some(id_str) = args.first() {
                    if let Ok(id) = id_str.parse::<u32>() {
                        return Some(Command::Fg(id));
                    }
                }
                eprintln!("fg: usage: fg <job_id>");
                None
            }
            "bg" => {
                if let Some(id_str) = args.first() {
                    if let Ok(id) = id_str.parse::<u32>() {
                        return Some(Command::Bg(id));
                    }
                }
                eprintln!("bg: usage: bg <job_id>");
                None
            }
            "export" => {
                if let Some(assignment) = args.first() {
                    if let Some((key, value)) = assignment.split_once('=') {
                        return Some(Command::Export(key.to_string(), value.to_string()));
                    }
                }
                None
            }
            _ => Some(Command::External(cmd.to_string(), args, background)),
        }
    }

    fn parse_chain(input: &str) -> Vec<ChainedCommand> {
        let mut commands = Vec::new();
        let mut current = String::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '&' if chars.peek() == Some(&'&') => {
                    chars.next(); // consume second &
                    if !current.trim().is_empty() {
                        commands.push(ChainedCommand {
                            command: current.trim().to_string(),
                            chain_type: Some(ChainType::And),
                        });
                        current.clear();
                    }
                }
                '|' if chars.peek() == Some(&'|') => {
                    chars.next(); // consume second |
                    if !current.trim().is_empty() {
                        commands.push(ChainedCommand {
                            command: current.trim().to_string(),
                            chain_type: Some(ChainType::Or),
                        });
                        current.clear();
                    }
                }
                ';' => {
                    if !current.trim().is_empty() {
                        commands.push(ChainedCommand {
                            command: current.trim().to_string(),
                            chain_type: Some(ChainType::Semicolon),
                        });
                        current.clear();
                    }
                }
                _ => current.push(c),
            }
        }

        // Add the last command
        if !current.trim().is_empty() {
            commands.push(ChainedCommand {
                command: current.trim().to_string(),
                chain_type: None,
            });
        }

        commands
    }

    pub fn execute(&self, job_manager: &mut JobManager) -> bool {
        match self {
            Command::Cd(path) => {
                let target = if let Some(p) = path {
                    p.clone()
                } else {
                    env::var("HOME").unwrap_or_else(|_| "/".to_string())
                };

                if let Err(e) = env::set_current_dir(&target) {
                    eprintln!("cd: {}", e);
                }
            }
            Command::Pwd => {
                match env::current_dir() {
                    Ok(path) => println!("{}", path.display()),
                    Err(e) => eprintln!("pwd: {}", e),
                }
            }
            Command::Ls(path) => {
                let target = path.as_deref().unwrap_or(".");
                match fs::read_dir(target) {
                    Ok(entries) => {
                        for entry in entries.flatten() {
                            println!("{}", entry.file_name().to_string_lossy());
                        }
                    }
                    Err(e) => eprintln!("ls: {}", e),
                }
            }
            Command::Exit => {
                println!("Goodbye!");
                return false;
            }
            Command::Help => {
                println!("Built-in commands:");
                println!("  cd [path]        - Change directory");
                println!("  pwd              - Print working directory");
                println!("  ls [path]        - List directory contents");
                println!("  echo <text>      - Print text (supports $VAR)");
                println!("  export VAR=value - Set environment variable");
                println!("  history          - Show command history");
                println!("  jobs             - List background jobs");
                println!("  fg <job_id>      - Bring job to foreground");
                println!("  bg <job_id>      - Resume job in background");
                println!("  help             - Show this help");
                println!("  exit/quit        - Exit the shell");
                println!("\nCommand chaining:");
                println!("  cmd1 && cmd2     - Run cmd2 only if cmd1 succeeds");
                println!("  cmd1 || cmd2     - Run cmd2 only if cmd1 fails");
                println!("  cmd1 ; cmd2      - Run both commands sequentially");
                println!("  cmd &            - Run command in background");
                println!("\nOther commands are executed as external programs");
            }
            Command::Echo(args) => {
                println!("{}", args.join(" "));
            }
            Command::Export(key, value) => {
                env::set_var(key, value);
                println!("{}={}", key, value);
            }
            Command::History => {
                return true;
            }
            Command::Jobs => {
                return true;
            }
            Command::Fg(_) => {
                return true;
            }
            Command::Bg(_) => {
                return true;
            }
            Command::Chain(commands) => {
                let mut last_success = true;

                for chained in commands {
                    let should_run = match chained.chain_type {
                        Some(ChainType::And) => last_success,
                        Some(ChainType::Or) => !last_success,
                        Some(ChainType::Semicolon) | None => true,
                    };

                    if should_run {
                        if let Some(cmd) = Command::parse(&chained.command) {
                            let result = cmd.execute(job_manager);
                            if !result {
                                return false; // Exit command was called
                            }
                            // For external commands, we track success/failure
                            last_success = result;
                        }
                    }
                }
            }
            Command::External(cmd, args, background) => {
                match ProcessCommand::new(cmd)
                    .args(args)
                    .spawn()
                {
                    Ok(mut child) => {
                        if *background {
                            let pid = child.id();
                            let command_str = format!("{} {}", cmd, args.join(" "));
                            job_manager.add_job(pid, command_str, child);
                        } else {
                            match child.wait() {
                                Ok(status) => {
                                    if !status.success() {
                                        return false; // Command failed
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error waiting for command: {}", e);
                                    return false;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("{}: {}", cmd, e);
                        return false;
                    }
                }
            }
        }
        true
    }
}
