mod limits;

use std::{
    io::{self, Write},
    process::ExitCode,
};

use clap::Parser;
use replicant_regex::{CompareOptions, Comparison, CompileOptions, Pattern, Side};
use serde::Serialize;

use limits::{Limit, parse_size_limit, parse_state_limit};

const HELP_HEADER: &str = concat!("replicant ", env!("CARGO_PKG_VERSION"));

const ABOUT: &str = "Compare regular expressions and find a shortest counterexample";

const LONG_ABOUT: &str =
    "Compare two regular expressions using whole-string matching on UTF-8 strings.

If they differ, print a shortest string matched by exactly one pattern and
identify which pattern matches it. Length is measured in UTF-8 bytes.

Memory limits accept bytes or units such as KB, MB, KiB, and MiB (case-sensitive).
Use 'none' to disable a limit. A value of 0 sets a zero limit.

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
    /// Print the comparison result as JSON
    #[arg(long)]
    json: bool,
    /// NFA size limit (default: 10MiB)
    #[arg(short = 'n', long = "nfa-limit", value_name = "SIZE", value_parser = parse_size_limit)]
    nfa_size_limit: Option<Limit>,
    /// DFA size limit (default: 10MiB)
    #[arg(short = 'd', long = "dfa-limit", value_name = "SIZE", value_parser = parse_size_limit)]
    dfa_size_limit: Option<Limit>,
    /// Auxiliary memory limit for DFA construction (default: 10MiB)
    #[arg(short = 't', long = "determinize-limit", value_name = "SIZE", value_parser = parse_size_limit)]
    determinize_size_limit: Option<Limit>,
    /// Maximum distinct DFA state pairs, including the initial pair (default: 100000)
    #[arg(short = 's', long = "state-limit", value_name = "COUNT", value_parser = parse_state_limit)]
    state_pair_limit: Option<Limit>,
}

#[derive(Serialize)]
struct ComparisonOutput<'a> {
    equivalent: bool,
    counterexample: Option<&'a str>,
    matches: Option<&'a str>,
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
    let defaults = CompileOptions::default();
    let compile_options = CompileOptions {
        nfa_size_limit: args
            .nfa_size_limit
            .map_or(defaults.nfa_size_limit, |limit| limit.0),
        dfa_size_limit: args
            .dfa_size_limit
            .map_or(defaults.dfa_size_limit, |limit| limit.0),
        determinize_size_limit: args
            .determinize_size_limit
            .map_or(defaults.determinize_size_limit, |limit| limit.0),
    };
    let compare_options = CompareOptions {
        state_pair_limit: args
            .state_pair_limit
            .map_or(CompareOptions::default().state_pair_limit, |limit| limit.0),
    };
    let left = Pattern::with_options(&args.left, &compile_options)
        .map_err(|error| format!("left pattern: {error}"))?;
    let right = Pattern::with_options(&args.right, &compile_options)
        .map_err(|error| format!("right pattern: {error}"))?;
    let result = left
        .compare_with_options(&right, &compare_options)
        .map_err(|error| error.to_string())?;
    let (input, side, code) = match &result {
        Comparison::Equivalent => (None, None, ExitCode::SUCCESS),
        Comparison::Different(example) => {
            let side = match example.matched_by {
                Side::Left => "left",
                Side::Right => "right",
            };
            (Some(example.input.as_str()), Some(side), ExitCode::from(1))
        }
    };
    let mut stdout = io::stdout().lock();
    if args.json {
        let output = ComparisonOutput {
            equivalent: matches!(result, Comparison::Equivalent),
            counterexample: input,
            matches: side,
        };
        serde_json::to_writer(&mut stdout, &output)
            .map_err(|error| format!("could not write JSON output: {error}"))?;
    } else {
        let output = match (input, side) {
            (Some(input), Some(side)) => {
                format!("different\ncounterexample: {input:?}\nmatches: {side}")
            }
            _ => "equivalent".to_owned(),
        };
        write!(stdout, "{output}").map_err(|error| format!("could not write output: {error}"))?;
    }
    writeln!(stdout).map_err(|error| format!("could not write output: {error}"))?;
    Ok(code)
}
