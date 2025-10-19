use std::env;

pub struct Prompt {
    prefix: String,
}

impl Prompt {
    pub fn new() -> Self {
        Self {
            prefix: String::from(">"),
        }
    }
    
    pub fn get_string(&self) -> String {
        let cwd = env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| String::from("?"));
        
        format!("{} {} ", cwd, self.prefix)
    }
    
    #[allow(dead_code)]
    pub fn display(&self) {
        print!("{}", self.get_string());
    }
}
