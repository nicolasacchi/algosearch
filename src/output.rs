use crate::error::AppError;
use colored::Colorize;
use serde::Serialize;
use std::io::IsTerminal;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
}

#[derive(Debug)]
pub struct Presenter {
    pub output_mode: OutputMode,
    pub verbose: bool,
}

impl Presenter {
    pub fn new(json_flag: bool, is_agent: bool, verbose: bool) -> Self {
        let output_mode = if json_flag || is_agent {
            OutputMode::Json
        } else {
            OutputMode::Human
        };

        let colors_enabled = output_mode == OutputMode::Human
            && std::env::var("NO_COLOR").is_err()
            && std::io::stdout().is_terminal();

        if !colors_enabled {
            colored::control::set_override(false);
        }

        Presenter {
            output_mode,
            verbose,
        }
    }

    pub fn success<T: Serialize>(&self, value: &T, human_format: impl FnOnce(&T) -> String) {
        match self.output_mode {
            OutputMode::Json => {
                println!(
                    "{}",
                    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!(
                        "{{\"error\": \"serialization failed: {}\"}}",
                        e
                    ))
                );
            }
            OutputMode::Human => {
                println!("{}", human_format(value));
            }
        }
    }

    pub fn error(&self, err: &AppError) {
        match self.output_mode {
            OutputMode::Json => {
                let mut obj = serde_json::json!({
                    "error": err.to_string(),
                });
                if let Some(suggestion) = err.suggestion() {
                    obj["suggestion"] = serde_json::Value::String(suggestion.to_string());
                }
                println!("{}", serde_json::to_string_pretty(&obj).unwrap());
            }
            OutputMode::Human => {
                eprintln!("{} {}", "error:".red().bold(), err);
                if let Some(suggestion) = err.suggestion() {
                    eprintln!("  {} {}", "hint:".yellow(), suggestion);
                }
            }
        }
    }

    pub fn progress(&self, msg: &str) {
        if self.verbose {
            eprintln!("{} {}", "...".dimmed(), msg);
        }
    }

    pub fn prompt(&self, msg: &str) -> Option<String> {
        if self.output_mode == OutputMode::Json {
            return None;
        }
        eprint!("{}", msg);
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok()?;
        Some(input.trim().to_string())
    }

    pub fn is_json(&self) -> bool {
        self.output_mode == OutputMode::Json
    }
}
