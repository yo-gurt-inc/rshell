use std::env;
use crate::command::Command;
use crate::prompt::Prompt;
use crate::history::History;
use crate::input::LineEditor;
use crate::jobs::JobManager;
use crate::pipes::{parse_pipeline, run_pipeline};

pub struct Shell {
    prompt: Prompt,
    history: History,
    editor: LineEditor,
    job_manager: JobManager,
    running: bool,
}

impl Shell {
    pub fn new() -> Self {
        if let Ok(exe_path) = env::current_exe() {
            env::set_var("SHELL", exe_path.to_string_lossy().to_string());
        }

        Self {
            prompt: Prompt::new(),
            history: History::new(),
            editor: LineEditor::new(),
            job_manager: JobManager::new(),
            running: true,
        }
    }

    /// Read input with line continuation support
    fn read_input_with_continuation(&mut self) -> Result<String, std::io::Error> {
        let mut full_input = String::new();
        let mut prompt_str = self.prompt.get_string();
        let continuation_prompt = "> ";
        let mut first_line = true;

        loop {
            let line = self.editor.read_line(&prompt_str, &mut self.history)?;

            // Check for line continuation BEFORE adding to full_input
            let line_trimmed = line.trim_end();
            let has_trailing_backslash = line_trimmed.ends_with('\\') && {
                // Count backslashes to ensure it's not escaped
                let chars: Vec<char> = line_trimmed.chars().collect();
                let mut count = 0;
                for c in chars.iter().rev() {
                    if *c == '\\' { count += 1; } else { break; }
                }
                count % 2 == 1 // Odd number means continuation
            };

            if has_trailing_backslash {
                // Remove trailing backslash and add to input
                let without_backslash = &line_trimmed[..line_trimmed.len() - 1];
                if !first_line && !full_input.is_empty() {
                    full_input.push(' '); // Join with space
                }
                full_input.push_str(without_backslash);
                first_line = false;
                prompt_str = continuation_prompt.to_string();
                continue;
            }

            // Add the line to full input
            if !first_line {
                full_input.push('\n'); // Preserve newlines in multiline strings
            }
            full_input.push_str(&line);
            first_line = false;

            // Check if we need continuation (unclosed quotes)
            if Command::needs_line_continuation(&full_input) {
                // Use continuation prompt for next line
                prompt_str = continuation_prompt.to_string();
                continue;
            }

            // Input is complete
            break;
        }

        Ok(full_input)
    }

    pub fn run(&mut self) {
        println!("Type 'help' for available commands\n");

        // Simple signal handling - ignore Ctrl+C in shell process
        #[cfg(unix)]
        unsafe {
            use libc::{signal, SIGINT, SIG_IGN};
            signal(SIGINT, SIG_IGN);
        }

        while self.running {
            // Update background jobs
            self.job_manager.update_jobs();

            match self.read_input_with_continuation() {
                Ok(input) => {
                    let mut trimmed = input.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }

                    // Detect background flag for pipelines
                    let background = trimmed.ends_with('&');
                    if background {
                        trimmed = trimmed[..trimmed.len() - 1].trim().to_string();
                    }

                    self.history.add(trimmed.clone());

                    if trimmed.contains('|') {
                        // Pipeline detected - parse_pipeline returns Vec<Vec<String>>
                        let commands = parse_pipeline(&trimmed);

                        if background {
                            // Run pipeline in background thread
                            let commands_clone = commands.clone();
                            std::thread::spawn(move || {
                                if let Err(e) = run_pipeline(commands_clone) {
                                    eprintln!("Pipeline error: {}", e);
                                }
                            });
                        } else {
                            // Run pipeline in foreground
                            if let Err(e) = run_pipeline(commands) {
                                eprintln!("Pipeline error: {}", e);
                            }
                        }
                    } else {
                        // Single command
                        if let Some(cmd) = Command::parse(&trimmed) {
                            match cmd {
                                Command::History => self.history.list(),
                                Command::Jobs => self.list_jobs(),
                                Command::Fg(job_id) => self.foreground_job(job_id),
                                Command::Bg(job_id) => self.background_job(job_id),
                                Command::Exit => self.running = false,
                                _ => {
                                    // Execute external or builtin command
                                    self.running = cmd.execute(&mut self.job_manager);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading input: {}", e);
                    break;
                }
            }
        }
    }

    fn list_jobs(&self) {
        let jobs = self.job_manager.list_jobs();
        if jobs.is_empty() {
            println!("No background jobs");
        } else {
            for job in jobs {
                let status = match job.status {
                    crate::jobs::JobStatus::Running => "Running",
                    crate::jobs::JobStatus::Stopped => "Stopped",
                    crate::jobs::JobStatus::Done => "Done",
                };
                println!("[{}] {} {} {}", job.id, status, job.pid, job.command);
            }
        }
    }

    fn foreground_job(&mut self, job_id: u32) {
        if let Some(mut job) = self.job_manager.remove_job(job_id) {
            println!("{}", job.command);
            if let Some(ref mut child) = job.process {
                match child.wait() {
                    Ok(status) => {
                        println!("[{}] Done (exit: {})", job.id, status);
                    }
                    Err(e) => {
                        eprintln!("Error waiting for job {}: {}", job.id, e);
                    }
                }
            } else {
                println!("[{}] Job already completed", job.id);
            }
        } else {
            eprintln!("fg: job {} not found", job_id);
        }
    }

    fn background_job(&mut self, job_id: u32) {
        if self.job_manager.get_job(job_id).is_some() {
            println!("[{}] continued in background", job_id);
        } else {
            eprintln!("bg: job {} not found", job_id);
        }
    }
}
