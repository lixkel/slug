use crate::errors::SlugError;
use crate::statistics::CheckVerdict;
use std::io::{self, IsTerminal, Write};

// All terminal output lives here, rest of the crate uses this module

fn env_override() -> Option<bool> {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return Some(false);
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some_and(|v| v != "0") {
        return Some(true);
    }
    None
}

fn stdout_color() -> bool {
    env_override().unwrap_or_else(|| io::stdout().is_terminal())
}

fn stderr_color() -> bool {
    env_override().unwrap_or_else(|| io::stderr().is_terminal())
}

fn paint(code: &str, text: &str) -> String {
    if stdout_color() {
        format!("\x1b[{}m{}\x1b[0m", code, text)
    } else {
        text.to_string()
    }
}

fn paint_err(code: &str, text: &str) -> String {
    if stderr_color() {
        format!("\x1b[{}m{}\x1b[0m", code, text)
    } else {
        text.to_string()
    }
}

// Green
fn pass(text: &str) -> String {
    paint("32", text)
}

// Red
fn flag(text: &str) -> String {
    paint("31", text)
}

// Yellow
fn notice(text: &str) -> String {
    paint("33", text)
}

// Print a plain line to stdout
pub fn line(text: &str) {
    println!("{}", text);
}

// Print raw text to stdout
pub fn raw(text: &str) {
    print!("{}", text);
}

// Print bold section heading
pub fn section(text: &str) {
    println!("\n{}", paint("1", text));
}

// Print error to stderr
// Message lines starting with "help: " render as hints
pub fn error(text: &str) {
    let mut lines = text.lines();
    eprintln!("{} {}", paint_err("1;31", "error:"), lines.next().unwrap_or(""));
    for line in lines {
        match line.strip_prefix("help: ") {
            Some(hint) => eprintln!("{} {}", paint_err("1;36", "help:"), hint),
            None => eprintln!("{}", line),
        }
    }
}

// Print warning to stderr
pub fn warn(text: &str) {
    eprintln!("{} {}", paint_err("1;33", "warning:"), text);
}

// Ask yes/no question, anything except yes means no
pub fn confirm(question: &str) -> Result<bool, SlugError> {
    print!("{} [y/N] ", question);
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "Yes"))
}

// Print each check verdict that has something to say
pub fn print_verdicts(verdicts: &[CheckVerdict]) {
    for verdict in verdicts {
        if let Some(text) = render(verdict) {
            println!("{}", text);
        }
    }
}

// One check verdict as a display line, None = the check has nothing to say
fn render(verdict: &CheckVerdict) -> Option<String> {
    match verdict {
        CheckVerdict::Skipped => None,
        CheckVerdict::TooFewSamples { check, have, need } => {
            Some(notice(&format!("  Too few samples for {} ({} of {})", check, have, need)))
        }
        CheckVerdict::MissingMetric { check, metric } => {
            Some(notice(&format!("  Metric '{}' missing for {}", metric, check)))
        }
        CheckVerdict::Judged(report) => report.line.as_ref().map(|line| {
            if report.flagged {
                format!("  {}  {}", flag("flag"), line)
            } else {
                format!("  {}  {}", pass("pass"), line)
            }
        }),
    }
}
