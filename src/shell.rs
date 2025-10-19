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

    pub fn run(&mut self) {
        println!("Type 'help' for available commands\n");

        while self.running {
            // Update background jobs
            self.job_manager.update_jobs();

            let prompt_str = self.prompt.get_string();

            match self.editor.read_line(&prompt_str, &mut self.history) {
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
                        // Pipeline detected
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
