use std::env;

pub struct Prompt {
    prefix: String,
}

impl Prompt {
    pub fn new() -> Self {
        Self {
            prefix: String::from("#"),
        }
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

        format!("{}@{}:{} {} ", username, hostname, cwd, self.prefix)
    }

    #[allow(dead_code)]
    pub fn display(&self) {
        print!("{}", self.get_string());
    }
}
