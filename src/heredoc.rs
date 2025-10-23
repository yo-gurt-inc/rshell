use std::io::{self, Write};
use std::fs::File;

pub fn parse_heredoc(input: &str) -> Option<(String, String, bool)> {
    if let Some(pos) = input.find("<<") {
        let before = input[..pos].trim();
        let after = input[pos + 2..].trim();
        
        let mut parts = after.splitn(2, char::is_whitespace);
        let delimiter = parts.next()?.trim();
        
        let quoted = (delimiter.starts_with('\'') && delimiter.ends_with('\'')) ||
                     (delimiter.starts_with('"') && delimiter.ends_with('"'));
        
        let delimiter = if quoted {
            &delimiter[1..delimiter.len()-1]
        } else {
            delimiter
        };
        
        return Some((before.to_string(), delimiter.to_string(), quoted));
    }
    None
}

pub fn read_heredoc_lines(delimiter: &str) -> io::Result<Vec<String>> {
    let mut lines = Vec::new();
    let stdin = io::stdin();
    
    loop {
        print!("> ");
        io::stdout().flush()?;
        
        let mut line = String::new();
        stdin.read_line(&mut line)?;
        
        let trimmed = line.trim_end_matches('\n');
        if trimmed == delimiter {
            break;
        }
        lines.push(line);
    }
    
    Ok(lines)
}

pub fn execute_heredoc(command: &str, delimiter: &str, _quoted: bool) -> io::Result<()> {
    use std::process::{Command, Stdio};
    use std::io::Write;
    
    let lines = read_heredoc_lines(delimiter)?;
    let content = lines.join("");
    
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }
    
    let has_redirect_out = command.contains('>');
    let (cmd_part, outfile) = if has_redirect_out {
        if let Some(pos) = command.find('>') {
            let cmd = command[..pos].trim();
            let file = command[pos+1..].trim();
            (cmd, Some(file))
        } else {
            (command, None)
        }
    } else {
        (command, None)
    };
    
    let cmd_parts: Vec<&str> = cmd_part.split_whitespace().collect();
    if cmd_parts.is_empty() {
        return Ok(());
    }
    
    let program = cmd_parts[0];
    let args = &cmd_parts[1..];
    
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(if outfile.is_some() { Stdio::piped() } else { Stdio::inherit() })
        .spawn()?;
    
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes())?;
    }
    
    let output = child.wait_with_output()?;
    
    if let Some(file) = outfile {
        let mut f = File::create(file)?;
        f.write_all(&output.stdout)?;
    }
    
    Ok(())
}
