use std::env;
use std::process;
use std::time::Instant;

mod command;
mod prompt;
mod history;
mod editor;  // Changed from 'input' to 'editor'
mod shell;
mod variables;
mod jobs;
mod pipes;
mod redirects;
mod heredoc;

fn print_help() {
    println!("rshell - custom shell");
    println!();
    println!("Usage: rshell [OPTIONS]");
    println!("  -h, --help       Print this help");
    println!("  -v, --version    Print version");
}

fn print_version() {
    println!("RShell v 0.1.0");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // respond to common flags quickly so external tools (neofetch) don't hang
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        process::exit(0);
    }

    if args.iter().any(|a| a == "-v" || a == "--version" || a == "-V" || a == "-version") {
        print_version();
        process::exit(0);
    }
    let start = Instant::now();

    let mut shell = shell::Shell::new();

    eprintln!("DEBUG: Startup took {:?}", start.elapsed());

    shell.run();

}
