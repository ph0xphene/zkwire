use std::{
    any::Any,
    io::{self, BufRead, Write},
    panic,
    process::ExitCode,
};

use clap::{Parser, Subcommand};
use colored::*;
use zkwire::{AssignSite, ZkWireTracker, forge_report, forge_reports, parse_raw_failure};

const DEMO_FAILURE: &str = include_str!("../fixtures/demo_failure.txt");
const MAX_FAILURE_BUFFER_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "zkwire",
    version,
    about = "Cargo-style diagnostics for Halo2 MockProver failures",
    long_about = "ZkWire reads Halo2 VerifyFailure Debug dumps and turns them into clickable, Cargo-style diagnostics with decoded field elements and local layout context."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show a pre-baked ZkWire diagnostic without running a circuit.
    Demo,
    /// Read stdin and rewrite Halo2 VerifyFailure dumps as ZkWire reports.
    Explain {
        /// Emit a machine-readable JSON array instead of terminal diagnostics.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if !is_broken_pipe_payload(info.payload()) {
            default_hook(info);
        }
    }));

    match panic::catch_unwind(run_cli) {
        Ok(Ok(())) => ExitCode::SUCCESS,
        Ok(Err(err)) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Ok(Err(err)) => {
            eprintln!("zkwire: {}", err);
            ExitCode::FAILURE
        }
        Err(payload) if is_broken_pipe_payload(payload.as_ref()) => ExitCode::SUCCESS,
        Err(payload) => panic::resume_unwind(payload),
    }
}

fn run_cli() -> io::Result<()> {
    match Cli::parse().command {
        Command::Demo => run_demo(),
        Command::Explain { json } => run_explain(json),
    }
}

fn is_broken_pipe_payload(payload: &(dyn Any + Send)) -> bool {
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied());

    message.is_some_and(|message| {
        message.contains("Broken pipe") || message.contains("failed printing to stdout")
    })
}

fn run_demo() -> io::Result<()> {
    let tracker = demo_tracker();
    let report = parse_raw_failure(1, DEMO_FAILURE, &tracker);

    println!(
        "{} {}",
        "ZkWire demo:".bold().bright_magenta(),
        "hardcoded Halo2 failure rendered as a source-linked diagnostic".bright_black()
    );
    forge_reports(&[report], &tracker);
    Ok(())
}

fn run_explain(json: bool) -> io::Result<()> {
    let tracker = ZkWireTracker::new();
    let stdin = io::stdin();
    let mut scanner = FailureScanner::new(&tracker, json);

    if json {
        print!("[");
        io::stdout().flush()?;
    }

    for line in stdin.lock().lines() {
        scanner.push_line(&line?)?;
    }
    scanner.finish()
}

fn demo_tracker() -> ZkWireTracker {
    let tracker = ZkWireTracker::new();
    tracker.record(AssignSite {
        region: "broken fibonacci".to_string(),
        offset: 8,
        column_index: 2,
        column_name: "fib(c) intentionally wrong".to_string(),
        expression: "a + b + 1".to_string(),
        file: "examples/broken_fibonacci.rs",
        line: 74,
    });
    tracker
}

struct FailureScanner<'a> {
    tracker: &'a ZkWireTracker,
    buffer: String,
    brace_depth: isize,
    collecting: bool,
    next_index: usize,
    json: bool,
    first_json_report: bool,
}

impl<'a> FailureScanner<'a> {
    fn new(tracker: &'a ZkWireTracker, json: bool) -> Self {
        Self {
            tracker,
            buffer: String::new(),
            brace_depth: 0,
            collecting: false,
            next_index: 1,
            json,
            first_json_report: true,
        }
    }

    fn push_line(&mut self, line: &str) -> io::Result<()> {
        if self.collecting {
            self.push_failure_line(line);
            if self.buffer.len() > MAX_FAILURE_BUFFER_BYTES {
                self.abort_collection()?;
                return Ok(());
            }
            if self.brace_depth <= 0 {
                self.flush_report()?;
            }
            return Ok(());
        }

        if is_failure_start(line) {
            self.collecting = true;
            self.push_failure_line(line);
            if self.buffer.len() > MAX_FAILURE_BUFFER_BYTES {
                self.abort_collection()?;
                return Ok(());
            }
            if self.brace_depth <= 0 {
                self.flush_report()?;
            }
        } else if !self.json {
            println!("{}", line);
            io::stdout().flush()?;
        }

        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        if self.collecting && !self.buffer.trim().is_empty() {
            self.flush_report()?;
        }
        if self.json {
            println!("]");
        }
        io::stdout().flush()
    }

    fn push_failure_line(&mut self, line: &str) {
        self.brace_depth += brace_delta(line);
        self.buffer.push_str(line);
        self.buffer.push('\n');
    }

    fn flush_report(&mut self) -> io::Result<()> {
        let report = parse_raw_failure(self.next_index, &self.buffer, self.tracker);
        if self.json {
            let mut stdout = io::stdout();
            if self.first_json_report {
                self.first_json_report = false;
            } else {
                write!(stdout, ",")?;
            }
            serde_json::to_writer(&mut stdout, &report).map_err(io::Error::other)?;
            stdout.flush()?;
        } else {
            forge_report(&report);
        }
        self.next_index += 1;
        self.buffer.clear();
        self.brace_depth = 0;
        self.collecting = false;
        Ok(())
    }

    fn abort_collection(&mut self) -> io::Result<()> {
        if !self.json {
            print!("{}", self.buffer);
            io::stdout().flush()?;
        }
        self.buffer.clear();
        self.brace_depth = 0;
        self.collecting = false;
        Ok(())
    }
}

fn is_failure_start(line: &str) -> bool {
    let trimmed = line.trim_start_matches(|c: char| c == '[' || c == ',' || c.is_whitespace());
    [
        "ConstraintNotSatisfied",
        "Permutation",
        "InstanceCellNotAssigned",
        "CellNotAssigned",
        "Lookup",
    ]
    .iter()
    .any(|variant| trimmed.starts_with(variant))
}

fn brace_delta(line: &str) -> isize {
    let opens = line.chars().filter(|c| *c == '{').count() as isize;
    let closes = line.chars().filter(|c| *c == '}').count() as isize;
    opens - closes
}
