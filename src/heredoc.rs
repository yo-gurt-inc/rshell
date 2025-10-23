               let mut line = String::new();        stdin.read_line(&mut line)?;                let trimmed = line.trim_end_matches('\n');        if trimmed == delimiter {            break;        }        lines.push(line);    }        Ok(lines)}pub fn execute_heredoc(command: &str, delimiter: &str, _quoted: bool) -> io::Result<()> {    use std::process::{Command, Stdio};    use std::io::Write;        let lines = read_heredoc_lines(delimiter)?;    let content = lines.join("");        let parts: Vec<&str> = command.split_whitespace().collect();    if parts.is_empty() {        return Ok(());    }        let has_redirect_out = command.contains('>');    let (cmd_part, outfile) = if has_redirect_out {        if let Some(pos) = command.find('>') {            let cmd = command[..pos].trim();            let file = command[pos+1..].trim();            (cmd, Some(file))        } else {            (command, None)        }    } else {        (command, None)    };        let cmd_parts: Vec<&str> = cmd_part.split_whitespace().collect();    if cmd_parts.is_empty() {        return Ok(());    }        let program = cmd_parts[0];    let args = &cmd_parts[1..];        let mut child = Command::new(program)        .args(args)        .stdin(Stdio::piped())        .stdout(if outfile.is_some() { Stdio::piped() } else { Stdio::inherit() })        .spawn()?;        if let Some(mut stdin) = child.stdin.take() {        stdin.write_all(content.as_bytes())?;    }        let output = child.wait_with_output()?;        if let Some(file) = outfile {        let mut f = File::create(file)?;        f.write_all(&output.stdout)?;    }        Ok(())}EOFcat > src/shell_patch.rs << 'EOF'use std::env;use crate::command::Command;use crate::prompt::Prompt;use crate::history::History;use crate::editor::LineEditor;use crate::jobs::JobManager;use crate::pipes::{parse_pipeline, run_pipeline};use crate::redirects::ParsedCommand;use crate::heredoc;pub struct Shell {    prompt: Prompt,    history: History,    editor: LineEditor,    job_manager: JobManager,    running: bool,}impl Shell {    pub fn new() -> Self {        if let Ok(exe_path) = env::current_exe() {            env::set_var("SHELL", exe_path.to_string_lossy().to_string());        }        Self {            prompt: Prompt::new(),            history: History::new(),            editor: LineEditor::new(),            job_manager: JobManager::new(),            running: true,        }    }    fn read_input_with_continuation(&mut self) -> Result<String, std::io::Error> {        let mut full_input = String::new();        let mut first_line = true;        loop {            let prompt = if first_line {                self.prompt.get_string()            } else {                "> ".to_string()            };            let line = self.editor.read_line(&prompt, &mut self.history)?;            let line_trimmed = line.trim_end();            let has_trailing_backslash = line_trimmed.ends_with('\\') && {                let chars: Vec<char> = line_trimmed.chars().collect();                let mut count = 0;                for c in chars.iter().rev() {                    if *c == '\\' { count += 1; } else { break; }                }                count % 2 == 1            };            if has_trailing_backslash {                let without_backslash = &line_trimmed[..line_trimmed.len() - 1];                if !first_line && !full_input.is_empty() {                    full_input.push(' ');                }                full_input.push_str(without_backslash);                first_line = false;                continue;            }            if !first_line {                full_input.push('\n');            }            full_input.push_str(&line);            first_line = false;            if Command::needs_line_continuation(&full_input) {                continue;            }            break;        }        Ok(full_input)    }    pub fn run(&mut self) {        println!("Type 'help' for available commands\n");

        #[cfg(unix)]
        unsafe {
            use libc::{signal, SIGINT, SIG_IGN};
            signal(SIGINT, SIG_IGN);
        }

        while self.running {
            self.job_manager.update_jobs();

            match self.read_input_with_continuation() {
                Ok(input) => {
                    let mut trimmed = input.trim().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }

                    let background = trimmed.ends_with('&');
                    if background {
                        trimmed = trimmed[..trimmed.len() - 1].trim().to_string();
                    }

                    self.history.add(trimmed.clone());

                    if trimmed.contains("<<") {
                        if let Some((command, delimiter, quoted)) = heredoc::parse_heredoc(&trimmed) {
                            if let Err(e) = heredoc::execute_heredoc(&command, &delimiter, quoted) {
                                eprintln!("Error: {}", e);
                            }
                        }
                    } else if (trimmed.contains('<') || trimmed.contains('>')) && !trimmed.contains('|') {
                        let parsed = ParsedCommand::parse(&trimmed);
                        if let Err(e) = parsed.execute() {
                            eprintln!("Error: {}", e);
                        }
                    } else if trimmed.contains('|') {
                        let commands = parse_pipeline(&trimmed);

                        if background {
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
                        if let Some(cmd) = Command::parse(&trimmed) {
                            match cmd {
                                Command::History => self.history.list(),
                                Command::Jobs => self.list_jobs(),
                                Command::Fg(job_id) => self.foreground_job(job_id),
                                Command::Bg(job_id) => self.background_job(job_id),
                                Command::Exit => self.running = false,
                                _ => {
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
