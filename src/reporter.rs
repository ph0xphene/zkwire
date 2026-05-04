//! Cargo-style colorized terminal reporter.

use colored::*;
use halo2_proofs::dev::VerifyFailure;

use crate::parser::parse_failure;
use crate::report::{ErrorType, ZkReport};
use crate::tracker::ZkWireTracker;

const BAR_WIDTH: usize = 72;

pub fn forge_trace(failures: &[VerifyFailure], tracker: &ZkWireTracker) {
    let bar = "━".repeat(BAR_WIDTH);

    println!();
    println!("{}", bar.bright_black());
    println!(
        " {}   {} circuit failure{} detected",
        "ZkWire".bold().bright_magenta(),
        failures.len().to_string().bold().yellow(),
        if failures.len() == 1 { "" } else { "s" }
    );
    println!("{}", bar.bright_black());
    println!();

    let pi = tracker.public_inputs();
    if !pi.is_empty() {
        println!(
            " {}",
            "public inputs (as passed to MockProver)".bold().underline()
        );
        for (i, (pretty, raw)) in pi.iter().enumerate() {
            println!(
                "   [{}] {}   {}",
                i.to_string().bright_black(),
                pretty.bright_white(),
                format!("raw: {}", raw).bright_black()
            );
        }
        println!();
    }

    for (i, failure) in failures.iter().enumerate() {
        let report = parse_failure(i + 1, failure, tracker);
        print_report(&report);
    }

    println!("{}", bar.bright_black());
    println!(
        " {} {}",
        "tip:".bright_yellow().bold(),
        "rerun after addressing the suggestions above.".bright_black()
    );
    println!();
}

fn print_report(r: &ZkReport) {
    println!(
        "{}: {}",
        format!("error[{:02}]", r.index).red().bold(),
        error_type_label(&r.error_type).red().bold()
    );

    let mut loc_parts: Vec<String> = Vec::new();
    if let Some(region) = &r.location.region {
        loc_parts.push(format!("region `{}`", region));
    }
    if let Some(offset) = r.location.offset {
        loc_parts.push(format!("offset {}", offset));
    }
    if let Some(ct) = &r.location.column_type {
        let mut s = format!("{} column", ct);
        if let Some(ci) = r.location.column_index {
            s.push_str(&format!(" #{}", ci));
        }
        loc_parts.push(s);
    }
    if let Some(g) = &r.location.gate {
        if !g.is_empty() {
            loc_parts.push(format!("gate `{}`", g));
        }
    }
    if !loc_parts.is_empty() {
        println!(
            "  {} {}",
            "-->".bright_blue().bold(),
            loc_parts.join("  ·  ").bright_blue()
        );
    }

    if !r.value_found.is_empty() {
        println!("  {}", "values found".bright_black());
        for (name, dec) in &r.value_found {
            println!(
                "    {} {} {}",
                "│".bright_black(),
                format!("{} =", name).cyan(),
                dec.bright_white()
            );
        }
    }

    // Origin: rendered as a Rust-compiler-style `= note:` line. The path is
    // styled to look like a clickable file:line reference (cyan + underline)
    // so most modern terminals will let you Cmd/Ctrl-click straight to it.
    if let Some(origin) = &r.origin {
        let path = format!("{}:{}", origin.file, origin.line);
        println!(
            "  {} value first assigned at {}  ({} `{}`)",
            "= note:".bright_yellow().bold(),
            path.cyan().underline(),
            "annotation".bright_black(),
            origin.column_name.bright_white(),
        );
    }

    println!("  {} {}", "= help:".bright_yellow().bold(), r.suggestion);
    println!();
}

fn error_type_label(et: &ErrorType) -> String {
    match et {
        ErrorType::InstanceMismatch => "public input mismatch".to_string(),
        ErrorType::ConstraintViolation => "gate constraint not satisfied".to_string(),
        ErrorType::PermutationMismatch => "permutation / copy mismatch".to_string(),
        ErrorType::CellNotAssigned => "cell not assigned".to_string(),
        ErrorType::Lookup => "lookup constraint failed".to_string(),
        ErrorType::Unknown(s) => format!("unknown failure: {}", s),
    }
}
