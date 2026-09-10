//! `susumu digest` — a short, pipe-friendly "what needs attention now" readout.
//!
//! Unlike `susumu readiness` (which replays a stored packet), the digest rescans
//! the project and runs commit attribution, so it is always current. It writes
//! nothing.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde::Serialize;
use susumu::model::{ProjectAnalysis, ReviewStatus};

use crate::expectation_readiness::{expectation_readiness, expectation_support};
use crate::review::checks::check_report;
use crate::review::handoff::handoff_report;
use crate::review::types::{
    CheckReport, CheckSeverity, ExpectationReadiness, HandoffReport, check_severity_label,
};
use crate::{DEFAULT_HISTORY_LIMIT, enrich_stale_findings, load_analysis};

#[derive(Debug, Args)]
pub(crate) struct DigestArgs {
    /// Directory to scan, or an existing .susu artifact.
    #[arg(default_value = ".")]
    pub(crate) target: PathBuf,

    /// Merge authored expectations from a .susu artifact or expectation-only fragment.
    #[arg(long, value_name = "FILE")]
    pub(crate) expectations: Option<PathBuf>,

    /// Merge verification records from a .susu artifact or verification-only fragment.
    #[arg(long, value_name = "FILE")]
    pub(crate) verifications: Option<PathBuf>,

    /// Merge decision records from a .susu artifact or decision-only fragment.
    #[arg(long, value_name = "FILE")]
    pub(crate) decisions: Option<PathBuf>,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    pub(crate) work: Option<PathBuf>,

    /// Show every item in each section instead of the top few.
    #[arg(long)]
    pub(crate) all: bool,

    /// Items to show per section before summarizing the rest.
    #[arg(long, default_value_t = 5)]
    pub(crate) max_per_section: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

pub(crate) fn run(args: &DigestArgs) -> Result<()> {
    let mut analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        None,
        false,
    )?;
    if args.target.is_dir() {
        enrich_stale_findings(&mut analysis, &args.target, DEFAULT_HISTORY_LIMIT);
    }
    let check = check_report(&analysis, false);
    let digest = Digest::build(&analysis, &check);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&digest)?);
    } else {
        digest.print(args.all, args.max_per_section);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct Digest {
    project: String,
    revision: Option<String>,
    generated_unix_seconds: u64,
    attention_count: usize,
    needs_reverification: Vec<AttentionItem>,
    checks: Vec<AttentionItem>,
    open_threads: Vec<AttentionItem>,
    expectations_without_verification: Vec<AttentionItem>,
}

#[derive(Debug, Serialize)]
struct AttentionItem {
    id: String,
    title: String,
    detail: String,
}

impl Digest {
    fn build(analysis: &ProjectAnalysis, check: &CheckReport) -> Self {
        let support = expectation_support(analysis);
        let readiness = expectation_readiness(analysis, &support);
        let handoff = handoff_report(analysis, check);

        let needs_reverification = reverification_items(analysis, &readiness);
        let checks = check_attention_items(check);
        let open_threads = open_thread_items(analysis);
        let expectations_without_verification = no_verification_items(&handoff);

        let attention_count = [
            &needs_reverification,
            &checks,
            &open_threads,
            &expectations_without_verification,
        ]
        .iter()
        .map(|section| section.len())
        .sum();

        Self {
            project: analysis.project_name.clone(),
            revision: analysis.source_revision.clone(),
            generated_unix_seconds: analysis.generated_unix_seconds,
            attention_count,
            needs_reverification,
            checks,
            open_threads,
            expectations_without_verification,
        }
    }

    fn print(&self, all: bool, max_per_section: usize) {
        let revision = self
            .revision
            .as_deref()
            .map(|sha| format!("  ({})", short_sha(sha)))
            .unwrap_or_default();
        println!("Susumu digest: {}{revision}", self.project);
        if self.attention_count == 0 {
            println!("Nothing needs attention.");
            return;
        }
        println!("{} item(s) need attention.", self.attention_count);

        print_section(
            "Needs re-verification",
            &self.needs_reverification,
            all,
            max_per_section,
        );
        print_section(
            "Checks needing attention",
            &self.checks,
            all,
            max_per_section,
        );
        print_section(
            "Open review threads",
            &self.open_threads,
            all,
            max_per_section,
        );
        print_section(
            "Expectations with no verification",
            &self.expectations_without_verification,
            all,
            max_per_section,
        );

        println!();
        println!("Run `susumu review` to refresh the portal.");
    }
}

fn reverification_items(
    analysis: &ProjectAnalysis,
    readiness: &[ExpectationReadiness],
) -> Vec<AttentionItem> {
    readiness
        .iter()
        .filter(|row| row.bucket == "needs_reverification")
        .map(|row| AttentionItem {
            id: row.expectation_id.clone(),
            title: row.title.clone(),
            detail: reverification_detail(analysis, &row.expectation_id)
                .unwrap_or_else(|| row.next_action.clone()),
        })
        .collect()
}

fn check_attention_items(check: &CheckReport) -> Vec<AttentionItem> {
    check
        .items
        .iter()
        .filter(|item| item.severity != CheckSeverity::Attention)
        .filter(|item| {
            !item.title.starts_with("SUS023")
                && !item.title.starts_with("SUS033")
                && !item.title.starts_with("open review thread")
        })
        .map(|item| AttentionItem {
            id: check_severity_label(item.severity).to_owned(),
            title: item.title.clone(),
            detail: item.detail.clone(),
        })
        .collect()
}

fn open_thread_items(analysis: &ProjectAnalysis) -> Vec<AttentionItem> {
    analysis
        .review_threads
        .iter()
        .filter(|thread| thread.status == ReviewStatus::Open)
        .map(|thread| AttentionItem {
            id: thread.id.clone(),
            title: thread.title.clone(),
            detail: format!(
                "owner={} kind={} target={}{}",
                thread.owner.as_deref().unwrap_or("-"),
                thread.kind,
                thread.target,
                thread
                    .subject
                    .as_deref()
                    .map(|subject| format!(":{subject}"))
                    .unwrap_or_default(),
            ),
        })
        .collect()
}

fn no_verification_items(handoff: &HandoffReport) -> Vec<AttentionItem> {
    handoff
        .expectations_without_verification
        .iter()
        .map(|record| AttentionItem {
            id: record.id.clone(),
            title: record.title.clone(),
            detail: format!("target={}", record.target),
        })
        .collect()
}

fn print_section(title: &str, items: &[AttentionItem], all: bool, max_per_section: usize) {
    if items.is_empty() {
        return;
    }
    println!();
    println!("{title} ({})", items.len());
    let limit = if all { items.len() } else { max_per_section };
    for item in items.iter().take(limit) {
        println!("  {}  {}", item.id, item.title);
        if !item.detail.is_empty() {
            println!("    {}", item.detail);
        }
    }
    if items.len() > limit {
        println!("  … and {} more", items.len() - limit);
    }
}

/// The enriched dirty-finding detail for an expectation, if one is present.
fn reverification_detail(analysis: &ProjectAnalysis, expectation_id: &str) -> Option<String> {
    let verification_ids: Vec<&str> = analysis
        .verifications
        .iter()
        .filter(|verification| verification.expectation_id == expectation_id)
        .map(|verification| verification.id.as_str())
        .collect();
    analysis
        .findings
        .iter()
        .find(|finding| {
            finding.is_dirty_evidence()
                && finding
                    .subject
                    .as_deref()
                    .is_some_and(|subject| verification_ids.contains(&subject))
        })
        .map(|finding| finding.detail.clone())
}

fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

#[cfg(test)]
mod tests;
