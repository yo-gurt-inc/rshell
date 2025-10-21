use std::env;
use colored::*;

pub struct Prompt;

impl Prompt {
    pub fn new() -> Self {
        Self
    }

    pub fn get_string(&self) -> String {
        let cwd = env::current_dir()
            .map(|p| {
                let path = p.display().to_string();
                if let Ok(home) = env::var("HOME") {
                    if path.starts_with(&home) {
                        return path.replacen(&home, "~", 1);
                    }
                }
                path
            })
            .unwrap_or_else(|_| String::from("?"));

        let username = env::var("USER")
            .or_else(|_| env::var("USERNAME"))
            .unwrap_or_else(|_| String::from("unknown"));
        
        let hostname = env::var("HOSTNAME")
            .unwrap_or_else(|_| whoami::hostname());

        let prefix = if username == "root" { "#" } else { "$" };

        format!(
            "{}@{}:{} {} ",
            username.green(),
            hostname.green(),
            cwd.blue(),
            prefix.white()
        )
    }
}
