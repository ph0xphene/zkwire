//! Cargo-style colorized terminal reporter.

use colored::*;
use halo2_proofs::dev::VerifyFailure;
use std::{fs, path::PathBuf};

use crate::parser::parse_failure;
use crate::report::{ErrorType, ZkReport};
use crate::tracker::ZkWireTracker;

const BAR_WIDTH: usize = 72;
const GRID_ROWS: usize = 5;
const GRID_COLS: usize = 5;
const GRID_CELL_WIDTH: usize = 10;
const GRID_ROW_WIDTH: usize = 5;

pub fn forge_trace(failures: &[VerifyFailure], tracker: &ZkWireTracker) {
    let reports: Vec<ZkReport> = failures
        .iter()
        .enumerate()
        .map(|(i, failure)| parse_failure(i + 1, failure, tracker))
        .collect();
    forge_reports(&reports, tracker);
}

pub fn forge_reports(reports: &[ZkReport], tracker: &ZkWireTracker) {
    let bar = "━".repeat(BAR_WIDTH);

    println!();
    println!("{}", bar.bright_black());
    println!(
        " {}   {} circuit failure{} detected",
        "ZkWire".bold().bright_magenta(),
        reports.len().to_string().bold().yellow(),
        if reports.len() == 1 { "" } else { "s" }
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

    for report in reports {
        print_report(report);
    }

    println!("{}", bar.bright_black());
    println!(
        " {} {}",
        "tip:".bright_yellow().bold(),
        "rerun after addressing the suggestions above.".bright_black()
    );
    println!();
}

pub fn forge_report(report: &ZkReport) {
    print_report(report);
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

    print_constraint_grid(r);

    if !r.value_found.is_empty() {
        println!("  {}", "values found".bright_black());
        for (idx, (name, dec)) in r.value_found.iter().enumerate() {
            let expression = semantic_expression_for_value(r, idx, name)
                .map(|expr| format!(" {}", format!("(from expression: {})", expr).bright_black()))
                .unwrap_or_default();
            println!(
                "    {} {} {}{}",
                "│".bright_black(),
                format!("{} =", name).cyan(),
                dec.bright_white(),
                expression
            );
        }
    }

    // Keep the path unstyled: ANSI escapes inside file links break detection in
    // some terminals/tmux setups. `./path:line:column` is the broadest format.
    if let Some(origin) = &r.origin {
        let path = clickable_source_link(origin.file, origin.line);
        println!(
            "  {} value first assigned at {}  ({} `{}`)",
            "= note:".bright_yellow().bold(),
            path,
            "annotation".bright_black(),
            origin.column_name.bright_white(),
        );
        print_source_context(origin.file, origin.line);
    }

    for warning in &r.warnings {
        println!("  {} {}", "= warning:".yellow().bold(), warning.yellow());
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

fn clickable_source_link(file: &str, line: u32) -> String {
    let path = if std::path::Path::new(file).is_absolute() || file.starts_with("./") {
        file.to_string()
    } else {
        format!("./{}", file)
    };

    format!("{}:{}:1", path, line)
}

fn semantic_expression_for_value<'a>(r: &'a ZkReport, idx: usize, name: &str) -> Option<&'a str> {
    let origin = r.origin.as_ref()?;
    if origin.expression.is_empty() {
        return None;
    }

    if origin.column_index == idx
        || origin.column_name == name
        || origin.column_name.contains(name)
        || name.contains(&origin.column_name)
    {
        Some(origin.expression.as_str())
    } else {
        None
    }
}

fn print_source_context(file: &str, line: u32) {
    let Some(path) = resolve_source_path(file) else {
        return;
    };
    let Ok(source) = fs::read_to_string(&path) else {
        return;
    };

    let lines: Vec<&str> = source.lines().collect();
    if line == 0 || lines.is_empty() {
        return;
    }

    let line_idx = line as usize - 1;
    if line_idx >= lines.len() {
        return;
    }

    let start = line_idx.saturating_sub(2);
    let end = (line_idx + 3).min(lines.len());
    let width = end.to_string().len().max(3);

    println!("  {}", "source context".bright_black());
    println!(
        "    {} {}",
        "-->".bright_blue().bold(),
        clickable_source_link(file, line)
    );
    println!("    {}", "│".bright_black());

    for (idx, text) in lines[start..end].iter().enumerate() {
        let number = start + idx + 1;
        let marker = if number == line as usize { ">" } else { " " };
        let rendered = highlight_rust_line(text);
        if number == line as usize {
            println!(
                "    {} {:>width$} {} {}",
                marker.truecolor(180, 83, 9).bold(),
                number.to_string().truecolor(180, 83, 9).bold(),
                "│".truecolor(180, 83, 9).bold(),
                rendered.bold(),
                width = width
            );
        } else {
            println!(
                "    {} {:>width$} {} {}",
                marker.bright_black(),
                number.to_string().bright_black(),
                "│".bright_black(),
                rendered,
                width = width
            );
        }
    }
}

fn resolve_source_path(file: &str) -> Option<PathBuf> {
    let path = PathBuf::from(file);
    if path.is_absolute() && path.exists() {
        return Some(path);
    }

    let cwd_path = PathBuf::from(file.trim_start_matches("./"));
    if cwd_path.exists() {
        return Some(cwd_path);
    }

    let manifest_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file.trim_start_matches("./"));
    if manifest_path.exists() {
        Some(manifest_path)
    } else {
        None
    }
}

fn highlight_rust_line(line: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "as", "const", "else", "enum", "fn", "for", "if", "impl", "let", "match", "mod", "mut",
        "pub", "return", "self", "struct", "trait", "type", "use", "where", "while",
    ];

    let mut rendered = String::new();
    let mut token = String::new();
    for ch in line.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            token.push(ch);
        } else {
            push_highlighted_token(&mut rendered, &token, KEYWORDS);
            token.clear();
            rendered.push(ch);
        }
    }
    push_highlighted_token(&mut rendered, &token, KEYWORDS);
    rendered
}

fn push_highlighted_token(rendered: &mut String, token: &str, keywords: &[&str]) {
    if token.is_empty() {
        return;
    }

    if keywords.contains(&token) {
        rendered.push_str(&token.blue().bold().to_string());
    } else {
        rendered.push_str(token);
    }
}

fn print_constraint_grid(r: &ZkReport) {
    if !matches!(r.error_type, ErrorType::ConstraintViolation) {
        return;
    }

    let Some(failing_row) = r.location.offset else {
        return;
    };

    let column_type = r.location.column_type.as_deref().unwrap_or("Advice");
    let failing_col = r.location.column_index.unwrap_or(0);
    let row_start = failing_row.saturating_sub(2);
    let col_start = failing_col.saturating_sub(2);
    let region = r.location.region.as_deref().unwrap_or("unknown region");

    println!(
        "  {} {}",
        "= layout:".truecolor(95, 95, 95).bold(),
        format!("{} around offset {}", region, failing_row).truecolor(170, 170, 170)
    );

    print_grid_border('┌', '┬', '┐');
    print_grid_header(column_type, col_start);
    print_grid_border('├', '┼', '┤');

    for row in row_start..row_start + GRID_ROWS {
        print!("    ");
        print!(
            "{}{}",
            "│".truecolor(95, 95, 95),
            format_cell(&row.to_string(), GRID_ROW_WIDTH).truecolor(95, 95, 95)
        );

        for col in col_start..col_start + GRID_COLS {
            let cell_text = grid_cell_value(r, row, col, row == failing_row && col == failing_col);
            let cell = if row == failing_row && col == failing_col {
                format_cell(&cell_text, GRID_CELL_WIDTH)
                    .truecolor(180, 83, 9)
                    .bold()
            } else {
                format_cell(&cell_text, GRID_CELL_WIDTH).truecolor(170, 170, 170)
            };
            print!("{}{}", "│".truecolor(95, 95, 95), cell);
        }
        println!("{}", "│".truecolor(95, 95, 95));
    }

    print_grid_border('└', '┴', '┘');
}

fn grid_cell_value(r: &ZkReport, row: usize, col: usize, failing: bool) -> String {
    if Some(row) != r.location.offset {
        return "·".to_string();
    }

    let Some((_, value)) = r.value_found.get(col) else {
        return if failing { "FAIL" } else { "·" }.to_string();
    };

    value.clone()
}

fn print_grid_header(column_type: &str, col_start: usize) {
    print!("    ");
    print!(
        "{}{}",
        "│".truecolor(95, 95, 95),
        format_cell("row", GRID_ROW_WIDTH)
            .truecolor(95, 95, 95)
            .bold()
    );

    for col in col_start..col_start + GRID_COLS {
        let label = format!("{} {}", column_type, col);
        print!(
            "{}{}",
            "│".truecolor(95, 95, 95),
            format_cell(&label, GRID_CELL_WIDTH)
                .truecolor(95, 95, 95)
                .bold()
        );
    }
    println!("{}", "│".truecolor(95, 95, 95));
}

fn print_grid_border(left: char, join: char, right: char) {
    let mut line = String::new();
    line.push(left);
    line.push_str(&"─".repeat(GRID_ROW_WIDTH));
    for _ in 0..GRID_COLS {
        line.push(join);
        line.push_str(&"─".repeat(GRID_CELL_WIDTH));
    }
    line.push(right);
    println!("    {}", line.truecolor(95, 95, 95));
}

fn format_cell(value: &str, width: usize) -> String {
    let truncated: String = value.chars().take(width).collect();
    format!("{:^width$}", truncated, width = width)
}
