use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{self, Command as ProcessCommand},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use serde::Serialize;
use sha2::{Digest, Sha256};
use susumu::{
    analysis::{anchor_decision_bases, anchor_verification_bases, refresh_derived_analysis},
    model::{
        Decision, DecisionStatus, Expectation, ExpectationStatus, ExpectationTarget,
        ProjectAnalysis, ReviewAnchor, ReviewCommentKind, ReviewStatus, ReviewThread, Verification,
        VerificationExecution, VerificationStatus, Work, WorkKind, WorkStatus,
    },
    parse_decisions, parse_expectations, parse_review_threads, parse_susu, parse_verifications,
    parse_works, scan_project, tui, write_decisions, write_expectations, write_review_threads,
    write_susu, write_verifications, write_works,
};
mod attestation;
mod cli;
mod diff_commands;
mod expectation_readiness;
mod git;
mod review;

#[allow(clippy::wildcard_imports)]
use cli::commands::*;
#[allow(clippy::wildcard_imports)]
use cli::daily::*;
use cli::daily_options::{ExpectationsArgs, ResolveArgs, StatusArgs, VerifyArgs};
use cli::dispatch::run_command;
use cli::loading::load_analysis;
use cli::project::{
    check, current_unix_seconds, diff, expectation_title, handoff, init_repository, write_text_file,
};
use cli::record_options::{
    AddDecision, AddExpectation, AddReviewThread, AddVerification, AddWork, ChainVerificationArgs,
    DecisionCommand, ExpectationCommand, ListDecisions, ListExpectations, ListReviewThreads,
    ListVerifications, ListWorks, RemoveDecision, RemoveExpectation, RemoveReviewThread,
    RemoveVerification, RemoveWork, ReviewThreadCommand, VerificationCommand, WorkCommand,
};
use cli::records::{
    add_decision, add_expectation, add_review_thread, add_verification, add_work,
    hash_evidence_file, inspect_attestation, inspect_git_signature, list_decisions,
    list_expectations, list_review_threads, list_verifications, list_works, read_execution_file,
    remove_decision, remove_expectation, remove_review_thread, remove_verification, remove_work,
    resolve_target, verification_chain,
};
#[cfg(test)]
use cli::records::{resolve_file_subject, verification_chain_digest, verify_verification_chain};
#[allow(clippy::wildcard_imports)]
use cli::support::*;
use cli::values::GitTargetDepth;
#[cfg(test)]
use cli::values::{WorkKindArg, WorkStatusArg};
use diff_commands::{
    diff_report, git_rewind, print_diff_json, print_diff_report, read_analysis_artifact,
};
use review::commands::{
    build_review, create_review, diff_reviews, export_review_html, open_review, serve_review,
};
#[cfg(test)]
use review::commands::{read_review_packet, review_diff_regressed, review_diff_report};

use cli::git::{
    GitCommand, GitConnectArgs, GitImportArgs, GitLinkArgs, GitRewindArgs, GitShortcutArgs,
    GitSignatureArgs,
};
use expectation_readiness::expectation_support;
use git::connect::{
    GitConnectReport, GitConnectedRecord, GitConnection, build_git_connect_report, contains_token,
    matched_artifact_file_ids, missing_expectation_work_records,
    single_language_matched_expectation,
};
#[allow(clippy::wildcard_imports)]
use git::execution::*;
use git::history::{git_commit_for_ref, git_commits, git_commits_for};
use git::reports::{print_git_connect_json, print_git_connect_report};
#[cfg(test)]
use git::snapshot::safe_snapshot_path;
use git::snapshot::{git_repo_label, git_snapshot_dir};
use git::types::{
    GitCommit, GitConnectExport, GitExpectationLink, GitImportContext, GitImportJson,
    GitImportRecordJson, GitWorkTarget, ImportedGitWork,
};
use review::checks::{
    check_item_jsons, check_json, check_report, print_check_json, print_check_report,
};
use review::handoff::{
    handoff_report, print_handoff_json, print_handoff_records, print_handoff_report,
    print_handoff_workflows, print_string_section,
};
use review::packet::review_packet;
#[cfg(test)]
use review::portal::{PORTAL_CONFIG_FILE, parse_portal_config, review_portal_html};
use review::portal::{
    handle_review_request, load_for_packet as load_portal_config_for_packet,
    load_for_target as load_portal_config_for_target, review_portal_html_with_config,
};
use review::readiness::{self as readiness_command, ReadinessArgs};
use review::types::{
    CheckItem, CheckItemJson, CheckReport, CheckSeverity, ExpectationSupport,
    ExpectationVerificationSupport, ReviewItemStored, ReviewPacketStored, check_result_reason,
};

#[derive(Debug, Args)]
struct InitArgs {
    /// Repository directory to initialize.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Expectations sidecar to create. Relative paths are resolved under the target directory.
    #[arg(short, long, default_value = "expectations.susu")]
    file: PathBuf,

    /// Project name to use in starter expectation text.
    #[arg(long)]
    name: Option<String>,

    /// Provenance label for the starter expectations.
    #[arg(long, default_value = "human:maintainer")]
    source: String,

    /// Overwrite an existing sidecar.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// Directory to scan, or an existing .susu artifact to check.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Merge authored expectations from a .susu artifact or expectation-only fragment.
    #[arg(long, value_name = "FILE")]
    expectations: Option<PathBuf>,

    /// Merge verification records from a .susu artifact or verification-only fragment.
    #[arg(long, value_name = "FILE")]
    verifications: Option<PathBuf>,

    /// Merge decision records from a .susu artifact or decision-only fragment.
    #[arg(long, value_name = "FILE")]
    decisions: Option<PathBuf>,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    work: Option<PathBuf>,

    /// Fail on warnings as well as critical items.
    #[arg(long)]
    strict: bool,

    /// Maximum review items to print.
    #[arg(long, default_value_t = 10)]
    max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct DiffArgs {
    /// Older .susu artifact.
    old: PathBuf,

    /// Newer .susu artifact.
    new: PathBuf,

    /// Exit nonzero when stale verification or decision evidence is present.
    #[arg(long)]
    fail_on_stale: bool,

    /// Maximum changed items to print per section.
    #[arg(long, default_value_t = 10)]
    max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct HandoffArgs {
    /// Directory to scan, or an existing .susu artifact to summarize.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Merge authored expectations from a .susu artifact or expectation-only fragment.
    #[arg(long, value_name = "FILE")]
    expectations: Option<PathBuf>,

    /// Merge verification records from a .susu artifact or verification-only fragment.
    #[arg(long, value_name = "FILE")]
    verifications: Option<PathBuf>,

    /// Merge decision records from a .susu artifact or decision-only fragment.
    #[arg(long, value_name = "FILE")]
    decisions: Option<PathBuf>,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    work: Option<PathBuf>,

    /// Maximum items to print per section.
    #[arg(long, default_value_t = 8)]
    max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ReviewCommand {
    /// Scan, check, and create review outputs in one command.
    Build(ReviewBuildArgs),

    /// Create a standalone review packet from an artifact or project.
    Create(ReviewCreateArgs),

    /// Open and replay a saved review packet.
    Open(ReviewOpenArgs),

    /// Compare two saved review packets.
    Diff(ReviewDiffArgs),

    /// Serve a saved review packet as a local web portal.
    Serve(ReviewServeArgs),

    /// Export a saved review packet as a standalone HTML portal.
    ExportHtml(ReviewExportHtmlArgs),
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
struct ReviewShortcutArgs {
    /// Directory to scan, or an existing .susu artifact to package.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Directory for convention-based Susumu outputs.
    #[arg(short = 'o', long, default_value = ".susumu", value_name = "DIR")]
    output_dir: PathBuf,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    work: Option<PathBuf>,

    /// Fail the embedded check result on warnings as well as critical items.
    #[arg(long)]
    strict: bool,

    /// Exit nonzero after writing outputs if the check result failed.
    #[arg(long)]
    fail_on_check: bool,

    /// Skip writing the standalone HTML portal.
    #[arg(long)]
    no_html: bool,

    /// Serve the built review packet as a local web portal after writing outputs.
    #[arg(long)]
    serve: bool,

    /// Host interface to bind when --serve is used.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind when --serve is used. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    port: u16,

    /// Emit a machine-readable build summary.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
struct ReviewBuildArgs {
    /// Directory to scan, or an existing .susu artifact to package.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Merge authored expectations from a .susu artifact or expectation-only fragment.
    #[arg(long, value_name = "FILE")]
    expectations: Option<PathBuf>,

    /// Merge verification records from a .susu artifact or verification-only fragment.
    #[arg(long, value_name = "FILE")]
    verifications: Option<PathBuf>,

    /// Merge decision records from a .susu artifact or decision-only fragment.
    #[arg(long, value_name = "FILE")]
    decisions: Option<PathBuf>,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    work: Option<PathBuf>,

    /// Write the generated .susu artifact to this file.
    #[arg(long, default_value = "target/susumu.susu", value_name = "FILE")]
    artifact_output: PathBuf,

    /// Write the review packet to this file.
    #[arg(
        short,
        long,
        default_value = "target/susumu.review.susu",
        value_name = "FILE"
    )]
    output: PathBuf,

    /// Optionally write the machine-readable check report JSON.
    #[arg(long, value_name = "FILE")]
    check_json: Option<PathBuf>,

    /// Optionally export the review portal as standalone HTML.
    #[arg(long, value_name = "FILE")]
    html: Option<PathBuf>,

    /// Fail the embedded check result on warnings as well as critical items.
    #[arg(long)]
    strict: bool,

    /// Exit nonzero after writing outputs if the check result failed.
    #[arg(long)]
    fail_on_check: bool,

    /// Emit a machine-readable build summary.
    #[arg(long)]
    json: bool,

    /// Serve the built review packet as a local web portal after writing outputs.
    #[arg(long)]
    serve: bool,

    /// Host interface to bind when --serve is used.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind when --serve is used. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    port: u16,
}

#[derive(Debug, Args)]
struct ReviewCreateArgs {
    /// Directory to scan, or an existing .susu artifact to package.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Merge authored expectations from a .susu artifact or expectation-only fragment.
    #[arg(long, value_name = "FILE")]
    expectations: Option<PathBuf>,

    /// Merge verification records from a .susu artifact or verification-only fragment.
    #[arg(long, value_name = "FILE")]
    verifications: Option<PathBuf>,

    /// Merge decision records from a .susu artifact or decision-only fragment.
    #[arg(long, value_name = "FILE")]
    decisions: Option<PathBuf>,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    work: Option<PathBuf>,

    /// Fail the embedded check result on warnings as well as critical items.
    #[arg(long)]
    strict: bool,

    /// Write the review packet to this file. If omitted, the packet is printed.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Print the full review packet JSON even when --output is supplied.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ReviewOpenArgs {
    /// Review packet created by `susumu review create`.
    packet: PathBuf,

    /// Maximum items to print per section.
    #[arg(long, default_value_t = 8)]
    max_items: usize,

    /// Emit the stored review packet JSON.
    #[arg(long)]
    json: bool,

    /// Open the embedded artifact in the Susumu TUI.
    #[arg(long)]
    tui: bool,
}

#[derive(Debug, Args)]
struct ReviewDiffArgs {
    /// Older review packet.
    old: PathBuf,

    /// Newer review packet.
    new: PathBuf,

    /// Maximum items to print per section.
    #[arg(long, default_value_t = 8)]
    max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,

    /// Exit nonzero when the newer packet has more critical items or newly fails.
    #[arg(long)]
    fail_on_regression: bool,
}

#[derive(Debug, Args)]
struct ReviewServeArgs {
    /// Review packet created by `susumu review create`.
    packet: PathBuf,

    /// Host interface to bind. Defaults to localhost.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    port: u16,
}

#[derive(Debug, Args)]
struct ReviewExportHtmlArgs {
    /// Review packet created by `susumu review create`.
    packet: PathBuf,

    /// HTML file to write.
    #[arg(short, long, value_name = "FILE")]
    output: PathBuf,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args)]
struct OpenArgs {
    /// Review packet whose sibling review.html file should be opened.
    #[arg(default_value = ".susumu/review.susu")]
    packet: PathBuf,

    /// Host interface to bind.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to bind. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    port: u16,

    /// Serve the packet locally instead of opening its static HTML export.
    #[arg(long)]
    serve: bool,

    /// Print the review summary instead of opening the portal.
    #[arg(long)]
    summary: bool,

    /// Open the embedded artifact in the Susumu TUI instead of opening the portal.
    #[arg(long)]
    tui: bool,

    /// Maximum items to print when --summary is used.
    #[arg(long, default_value_t = 8)]
    max_items: usize,

    /// Emit the stored review packet JSON instead of opening the portal.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum AttestationCommand {
    /// Parse and structurally inspect an attestation envelope.
    Inspect(InspectAttestationArgs),
}

#[derive(Debug, Args)]
struct InspectAttestationArgs {
    /// JSON attestation envelope to inspect.
    #[arg(short, long)]
    file: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(command) = cli.command {
        return run_command(command);
    }

    let analysis = load_analysis(
        &cli.target,
        cli.expectations.as_ref(),
        cli.verifications.as_ref(),
        cli.decisions.as_ref(),
        cli.work.as_ref(),
        None,
        true,
    )?;

    if let Some(output) = &cli.output {
        let source = write_susu(&analysis, cli.minify)?;
        fs::write(output, source)
            .with_context(|| format!("could not write {}", output.display()))?;
        eprintln!("wrote {}", output.display());
    }

    if cli.headless {
        if cli.output.is_none() {
            print!("{}", write_susu(&analysis, cli.minify)?);
        }
        return Ok(());
    }

    tui::run(analysis, cli.output)
}

#[cfg(test)]
#[path = "main/cli_record_tests.rs"]
mod cli_record_tests;
#[cfg(test)]
#[path = "main/cli_tests.rs"]
mod cli_tests;
#[cfg(test)]
#[path = "main/git_tests.rs"]
mod git_tests;
#[cfg(test)]
#[path = "main/portal_tests.rs"]
mod portal_tests;
#[cfg(test)]
#[path = "main/review_packet_tests.rs"]
mod review_packet_tests;
#[cfg(test)]
#[path = "main/review_tests.rs"]
mod review_tests;
#[cfg(test)]
#[path = "main/test_support.rs"]
mod test_support;
