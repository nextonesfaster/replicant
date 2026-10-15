use std::{
    io::{self, Write},
    process::ExitCode,
};

use clap::Parser;
use replicant::{Comparison, Pattern, Side};

const HELP_HEADER: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

const ABOUT: &str = "Compare regular expressions and find a shortest counterexample";

const LONG_ABOUT: &str =
    "Compare two regular expressions using whole-string matching on UTF-8 strings.

If they differ, print a shortest string matched by exactly one pattern and
identify which pattern matches it. Length is measured in UTF-8 bytes.

Exit status is 0 if equivalent, 1 if different, and 2 on error.

Use '--' before patterns that begin with a hyphen.";

#[derive(Parser)]
#[command(
    name = "vk",
    version,
    before_help = HELP_HEADER,
    about = ABOUT,
    long_about = LONG_ABOUT,
)]
struct Cli {
    /// Left regular expression
    #[arg(value_name = "LEFT")]
    left: String,
    /// Right regular expression
    #[arg(value_name = "RIGHT")]
    right: String,
}

fn main() -> ExitCode {
    let args = Cli::parse();
    match run(args) {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "vk: {error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Cli) -> Result<ExitCode, String> {
    let left = Pattern::new(&args.left).map_err(|error| format!("left pattern: {error}"))?;
    let right = Pattern::new(&args.right).map_err(|error| format!("right pattern: {error}"))?;
    let result = left.compare(&right).map_err(|error| error.to_string())?;
    let (output, code) = match result {
        Comparison::Equivalent => ("equivalent".to_owned(), ExitCode::SUCCESS),
        Comparison::Different(example) => {
            let side = match example.matched_by {
                Side::Left => "left",
                Side::Right => "right",
            };
            (
                format!(
                    "different\ncounterexample: {:?}\nmatches: {side}",
                    example.input
                ),
                ExitCode::from(1),
            )
        }
    };
    writeln!(io::stdout().lock(), "{output}")
        .map_err(|error| format!("could not write output: {error}"))?;
    Ok(code)
}
