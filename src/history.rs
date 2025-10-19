use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::env;

pub struct History {
    commands: Vec<String>,
    file_path: PathBuf,
    position: usize,
}

impl History {
    pub fn new() -> Self {
        let file_path = Self::get_history_path();
        let commands = Self::load_from_file(&file_path);
        let position = commands.len();
        
        Self {
            commands,
            file_path,
            position,
        }
    }
    
    fn get_history_path() -> PathBuf {
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".mycli_history")
    }
    
    fn load_from_file(path: &PathBuf) -> Vec<String> {
        if let Ok(file) = File::open(path) {
            let reader = BufReader::new(file);
            reader.lines()
                .filter_map(|line| line.ok())
                .collect()
        } else {
            Vec::new()
        }
    }
    
    pub fn add(&mut self, command: String) {
        if command.trim().is_empty() {
            return;
        }
        
        // Don't add duplicate of last command
        if self.commands.last() != Some(&command) {
            self.commands.push(command.clone());
            self.save_to_file(&command);
        }
        
        self.position = self.commands.len();
    }
    
    fn save_to_file(&self, command: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
        {
            let _ = writeln!(file, "{}", command);
        }
    }
    
    pub fn previous(&mut self) -> Option<&String> {
        if self.position > 0 {
            self.position -= 1;
            self.commands.get(self.position)
        } else {
            None
        }
    }
    
    pub fn next(&mut self) -> Option<&String> {
        if self.position < self.commands.len() - 1 {
            self.position += 1;
            Some(&self.commands[self.position])
        } else {
            self.position = self.commands.len();
            None
        }
    }
    
    pub fn list(&self) {
        for (i, cmd) in self.commands.iter().enumerate() {
            println!("{}: {}", i + 1, cmd);
        }
    }

    #[allow(dead_code)]
    pub fn search(&self, pattern: &str) -> Vec<(usize, &String)> {
        self.commands
            .iter()
            .enumerate()
            .filter(|(_, cmd)| cmd.contains(pattern))
            .collect()
    }
}
