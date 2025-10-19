use crate::jobs::JobManager;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Stdio};

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
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        let background = input.ends_with('&');
        let input = if background {
            input[..input.len() - 1].trim()
        } else {
            input
        };

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let cmd = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        match cmd {
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
                program: cmd.to_string(),
                args,
                background,
            }),
        }
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
                println!("\nAdd '&' at the end of a command to run it in background");
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
                            // Don't exit the shell on error
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
                            // Don't exit the shell on error
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
