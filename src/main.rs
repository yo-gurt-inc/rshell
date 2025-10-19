use std::env;
use std::process;

mod command;
mod prompt;
mod history;
mod input;
mod shell;
mod variables;
mod jobs;

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

    // Normal interactive shell startup
    let mut sh = shell::Shell::new();
    sh.run();
}