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

// Bold
fn bold(text: &str) -> String {
    paint("1", text)
}

// Print a plain line to stdout
pub fn line(text: &str) {
    println!("{}", text);
}

// Print raw text to stdout
pub fn raw(text: &str) {
    print!("{}", text);
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

// Render one bechmark report
// Flagged benchmarks expand
pub fn benchmark_report(name: &str, name_width: usize, verdicts: &[CheckVerdict], flagged: bool) {
    if flagged {
        println!("  {}  {}", flag("FLAG"), bold(name));
        print_check_rows(verdicts);
        return;
    }

    // skip if every check was skipped, otherwise benchmark passed
    let mut tag = notice("skip");
    for verdict in verdicts {
        if matches!(verdict, CheckVerdict::Judged(_)) {
            tag = pass("pass");
            break;
        }
    }

    let padded = format!("{:<width$}", name, width = name_width);
    println!("  {}  {}  {}", tag, bold(&padded), check_summaries(verdicts).join(", "));
}

// One verdict as (tag, check, text), None means show nothing
fn describe(verdict: &CheckVerdict) -> Option<(&'static str, &'static str, String)> {
    match verdict {
        CheckVerdict::Skipped => None,
        CheckVerdict::TooFewSamples { check, have, need } => {
            Some(("skip", check, format!("{}/{} samples", have, need)))
        }
        CheckVerdict::MissingMetric { check, metric } => {
            Some(("skip", check, format!("no {} metric", metric)))
        }
        CheckVerdict::Judged(report) => {
            // Silent pass has nothing to show
            if !report.flagged && report.text.is_empty() {
                return None;
            }
            let tag = if report.flagged { "flag" } else { "pass" };
            Some((tag, report.check, report.text.clone()))
        }
    }
}

// Expanded block, one aligned colored row per check
fn print_check_rows(verdicts: &[CheckVerdict]) {
    for verdict in verdicts {
        if let Some((tag, check, text)) = describe(verdict) {
            let tag = match tag {
                "flag" => flag(tag),
                "pass" => pass(tag),
                _ => notice(tag),
            };
            print_row(&tag, check, &text);
        }
    }
}

// Fixed column width, longest check name is confidence
fn print_row(tag: &str, check: &str, text: &str) {
    println!("        {}  {:<10}  {}", tag, check, text);
}

// Compact "check text" pieces for the one line summary
fn check_summaries(verdicts: &[CheckVerdict]) -> Vec<String> {
    let mut pieces = Vec::new();
    for verdict in verdicts {
        if let Some((_, check, text)) = describe(verdict) {
            pieces.push(format!("{} {}", check, text));
        }
    }
    pieces
}
