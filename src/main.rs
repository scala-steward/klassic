mod cli;

use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use cli::{ExecutionConfig, ParsedCommand, RunAction, parse_command_line, usage};
use klassic_eval::{Evaluator, EvaluatorConfig, evaluate_text_with_config};

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some(command) = parse_command_line(&args) else {
        eprintln!("{}", usage());
        return ExitCode::from(1);
    };

    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}

fn run(command: ParsedCommand) -> Result<(), u8> {
    match command.action {
        RunAction::EvaluateExpression(expression) => {
            let config = EvaluatorConfig {
                deny_trust: command.config.deny_trust,
                warn_trust: command.config.warn_trust,
            };
            match evaluate_text_with_config("<expression>", &expression, config) {
                Ok(value) => {
                    println!("{value}");
                    Ok(())
                }
                Err(error) => {
                    eprintln!("{error}");
                    Err(1)
                }
            }
        }
        RunAction::EvaluateFile(path) => {
            let text = match fs::read_to_string(&path) {
                Ok(text) => text,
                Err(error) => {
                    eprintln!("{}: {error}", path.display());
                    return Err(1);
                }
            };
            let config = EvaluatorConfig {
                deny_trust: command.config.deny_trust,
                warn_trust: command.config.warn_trust,
            };
            match evaluate_text_with_config(&path.display().to_string(), &text, config) {
                Ok(_) => Ok(()),
                Err(error) => {
                    eprintln!("{error}");
                    Err(1)
                }
            }
        }
        RunAction::StartRepl => {
            start_repl(command.config);
            Ok(())
        }
    }
}

fn start_repl(config: ExecutionConfig) {
    let mut history = Vec::<String>::new();
    let mut buffer = String::new();
    let mut evaluator = Evaluator::with_config(EvaluatorConfig {
        deny_trust: config.deny_trust,
        warn_trust: config.warn_trust,
    });

    loop {
        print!("{}", if buffer.is_empty() { "> " } else { "| " });
        let _ = io::stdout().flush();

        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if buffer.is_empty() && trimmed == ":exit" {
            break;
        }
        if buffer.is_empty() && trimmed == ":history" {
            for (index, command) in history.iter().enumerate() {
                println!("{}: {}", index + 1, command);
            }
            continue;
        }

        buffer.push_str(trimmed);
        buffer.push('\n');

        match evaluator.evaluate_text("<repl>", &buffer) {
            Ok(value) => {
                println!("value = {value}");
                history.push(buffer.trim_end().to_string());
                buffer.clear();
            }
            Err(error) if error.is_incomplete() => {}
            Err(error) => {
                println!("Error: {error}");
                buffer.clear();
            }
        }
    }
}
