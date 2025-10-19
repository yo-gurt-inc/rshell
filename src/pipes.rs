use std::process::{Command, Stdio};
use std::io;

/// Parse user input into pipeline commands
/// e.g., "ls -l | grep rshell | wc -l" -> Vec<Vec<String>>
pub fn parse_pipeline(input: &str) -> Vec<Vec<String>> {
    input
        .split('|')
        .map(|cmd| cmd.trim()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect())
        .collect()
}

/// Execute a pipeline of commands
/// Connects stdout of each command to stdin of the next
pub fn run_pipeline(commands: Vec<Vec<String>>) -> io::Result<()> {
    if commands.is_empty() {
        return Ok(());
    }

    let mut children = Vec::new();
    let mut previous_stdout = None;

    for (i, cmd_parts) in commands.iter().enumerate() {
        if cmd_parts.is_empty() {
            continue;
        }

        let mut cmd = Command::new(&cmd_parts[0]);
        if cmd_parts.len() > 1 {
            cmd.args(&cmd_parts[1..]);
        }

        if let Some(stdin) = previous_stdout {
            cmd.stdin(stdin);
        }

        if i < commands.len() - 1 {
            cmd.stdout(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
        }

        let mut child = cmd.spawn()?;

        previous_stdout = if i < commands.len() - 1 {
            Some(Stdio::from(child.stdout.take().unwrap()))
        } else {
            None
        };

        children.push(child);
    }

    for mut child in children {
        child.wait()?;
    }

    Ok(())
}
