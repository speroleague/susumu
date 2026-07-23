use anyhow::{Context, Result};

use crate::{
    cli::git::GitConnectArgs,
    git::{
        connect::{GitConnectReport, GitConnectedRecord, GitConnection},
        types::{GitConnectExport, GitConnectJson},
    },
};

pub(crate) fn print_git_connect_report(
    args: &GitConnectArgs,
    report: &GitConnectReport,
    export: Option<&GitConnectExport>,
) {
    println!(
        "Susumu git connect: {} -> {}",
        args.repo.display(),
        args.artifact.display()
    );
    println!("commits: {}", report.records.len());
    println!("connected: {}", report.connected);
    println!("needs_record: {}", report.needs_record);
    println!("unconnected: {}", report.unconnected);
    if let Some(export) = export {
        println!("exported_work: {} -> {}", export.written, export.path);
    }
    println!();

    for record in report.records.iter().take(args.max_items) {
        println!(
            "[{}] {}  {}",
            record.status, record.short_commit, record.title
        );
        println!("  author: {}", record.author);
        println!("  date: {}", record.date);
        println!("  reasons: {}", record.reasons.join("; "));
        print_connected_section("workflows", &record.workflows);
        print_connected_section("expectations", &record.expectations);
        print_connected_section("verifications", &record.verifications);
        print_connected_section("decisions", &record.decisions);
        print_connected_section("work", &record.works);
        print_git_suggestions(record);
        if !record.changed_files.is_empty() {
            println!("  changed:");
            for path in record.changed_files.iter().take(5) {
                println!("    - {path}");
            }
            if record.changed_files.len() > 5 {
                println!("    ... {} more", record.changed_files.len() - 5);
            }
        }
        println!();
    }

    if report.records.len() > args.max_items {
        println!("... {} more commits", report.records.len() - args.max_items);
    }
}

pub(crate) fn print_git_suggestions(record: &GitConnection) {
    if record.status != "unconnected" {
        return;
    }
    println!("  next:");
    if record.suggestions.is_empty() {
        println!("    - susumu expectation list");
        println!(
            "    - susumu git link {} <expectation-id>",
            record.short_commit
        );
        return;
    }
    println!("    likely expectations:");
    for suggestion in &record.suggestions {
        println!(
            "      - {}  {} (score {})",
            suggestion.expectation_id, suggestion.title, suggestion.score
        );
    }
    println!("    commands:");
    for suggestion in &record.suggestions {
        println!("      - {}", suggestion.command);
    }
}

pub(crate) fn print_connected_section(title: &str, records: &[GitConnectedRecord]) {
    if records.is_empty() {
        return;
    }
    println!("  {title}:");
    for record in records {
        println!("    - {}  {} ({})", record.id, record.title, record.reason);
    }
}

pub(crate) fn print_git_connect_json(
    args: &GitConnectArgs,
    report: &GitConnectReport,
    export: Option<&GitConnectExport>,
) -> Result<()> {
    let output = GitConnectJson {
        repo: args.repo.display().to_string(),
        artifact: args.artifact.display().to_string(),
        since: args.since.as_deref(),
        until: args.until.as_deref(),
        commits: report.records.len(),
        connected: report.connected,
        needs_record: report.needs_record,
        unconnected: report.unconnected,
        export,
        records: &report.records,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize git connect report")?
    );
    Ok(())
}
