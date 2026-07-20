use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
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
        ProjectAnalysis, Verification, VerificationStatus, Work, WorkKind, WorkStatus,
    },
    parse_decisions, parse_expectations, parse_susu, parse_verifications, parse_works,
    scan_project, tui, write_decisions, write_expectations, write_susu, write_verifications,
    write_works,
};
mod checks;
mod cli_values;
mod expectation_readiness;
mod git_connect;
mod handoff;
mod review_packet;
mod review_types;

use checks::{check_item_jsons, check_json, check_report, print_check_json, print_check_report};
use cli_values::{
    DecisionStatusArg, ExpectationStatusArg, ExpectationTargetArg, GitTargetDepth,
    GitTargetDepthArg, VerificationStatusArg, WorkKindArg, WorkStatusArg,
};
use expectation_readiness::expectation_support;
use git_connect::{
    GitConnectReport, GitConnectedRecord, GitConnection, build_git_connect_report, contains_token,
    matched_artifact_file_ids, missing_expectation_work_records,
    single_language_matched_expectation,
};
use handoff::{
    handoff_report, print_handoff_json, print_handoff_records, print_handoff_report,
    print_handoff_workflows, print_string_section,
};
use review_packet::review_packet;
use review_types::{
    CheckItem, CheckItemJson, CheckReport, CheckSeverity, ExpectationReadiness, ExpectationSupport,
    ExpectationVerificationSupport, READINESS_BUCKETS, ReviewItemStored, ReviewPacketStored,
    check_result_reason,
};

#[derive(Debug, Parser)]
#[command(
    name = "susumu",
    version,
    about = "Make a codebase's workflows visible"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Directory to scan, or an existing .susu artifact to open.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Write the analysis to this .susu file.
    #[arg(short, long)]
    output: Option<PathBuf>,

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

    /// Emit compact .susu syntax.
    #[arg(long)]
    minify: bool,

    /// Scan or load without opening the terminal interface.
    #[arg(long)]
    headless: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a starter expectations sidecar for a repository.
    Init(InitArgs),

    /// Check an artifact or project for review blockers.
    Check(CheckArgs),

    /// Compare two .susu artifacts.
    Diff(DiffArgs),

    /// Produce a compact project handoff for humans or agents.
    Handoff(HandoffArgs),

    /// Create the daily Susumu review outputs, or use advanced review subcommands.
    Review {
        #[command(flatten)]
        args: ReviewShortcutArgs,

        #[command(subcommand)]
        command: Option<ReviewCommand>,
    },

    /// Open the latest Susumu review portal.
    Open(OpenArgs),

    /// Show the current Susumu project status.
    Status(StatusArgs),

    /// Show expectation readiness from the latest review packet.
    Readiness(ReadinessArgs),

    /// Resolve a source path to a scanner-assigned evidence id.
    Resolve(ResolveArgs),

    /// Browse and search expectation ids for reviews, Git links, and verification.
    Expectations(ExpectationsArgs),

    /// Record verification evidence for an expectation.
    Verify(VerifyArgs),

    /// Author expectation sidecar records.
    Expectation {
        #[command(subcommand)]
        command: ExpectationCommand,
    },

    /// Author verification sidecar records.
    Verification {
        #[command(subcommand)]
        command: VerificationCommand,
    },

    /// Author decision sidecar records.
    Decision {
        #[command(subcommand)]
        command: DecisionCommand,
    },

    /// Author work sidecar records.
    Work {
        #[command(subcommand)]
        command: WorkCommand,
    },

    /// Connect local Git history to Susumu work, or use advanced Git subcommands.
    Git {
        #[command(flatten)]
        args: GitShortcutArgs,

        #[command(subcommand)]
        command: Option<GitCommand>,
    },
}

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

#[derive(Debug, Args)]
struct StatusArgs {
    /// Directory to scan, or an existing .susu artifact to check.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Directory for convention-based Susumu outputs.
    #[arg(long, default_value = ".susumu", value_name = "DIR")]
    output_dir: PathBuf,

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
struct ReadinessArgs {
    /// Review packet to inspect.
    #[arg(
        short,
        long,
        default_value = ".susumu/review.susu",
        value_name = "FILE"
    )]
    packet: PathBuf,

    /// Maximum readiness items to print.
    #[arg(long, default_value_t = 20)]
    max_items: usize,

    /// Filter by readiness bucket.
    #[arg(long, value_name = "BUCKET")]
    bucket: Option<String>,

    /// Search expectation id, title, target, subject, label, or status.
    #[arg(short, long)]
    search: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ResolveArgs {
    /// Source path to resolve, relative to the project root.
    path: PathBuf,

    /// Project directory to scan.
    #[arg(long, default_value = ".")]
    target: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ExpectationsArgs {
    /// Directory to scan, or an existing .susu artifact to inspect.
    #[arg(default_value = ".")]
    target: PathBuf,

    /// Read expectations from a specific sidecar or artifact instead of scanning/loading target.
    #[arg(short, long, value_name = "FILE")]
    file: Option<PathBuf>,

    /// Search expectation id, title, detail, source, target, subject, or support status.
    #[arg(short, long)]
    search: Option<String>,

    /// Filter by expectation status: proposed, accepted, or superseded.
    #[arg(long)]
    status: Option<ExpectationStatusArg>,

    /// Maximum expectations to print.
    #[arg(long, default_value_t = 50)]
    max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
struct VerifyArgs {
    /// Expectation id being checked.
    expectation: String,

    /// Directory or artifact used to validate the expectation id.
    #[arg(long, default_value = ".")]
    target: PathBuf,

    /// Verification sidecar to update.
    #[arg(short, long, default_value = "verifications.susu")]
    file: PathBuf,

    /// Optional explicit id. Omit to derive a stable id from the record.
    #[arg(long)]
    id: Option<String>,

    /// Verification id this record supersedes.
    #[arg(long)]
    supersedes: Option<String>,

    /// Mark the verification as passed.
    #[arg(long, conflicts_with_all = ["failed", "inconclusive"])]
    passed: bool,

    /// Mark the verification as failed.
    #[arg(long, conflicts_with_all = ["passed", "inconclusive"])]
    failed: bool,

    /// Mark the verification as inconclusive.
    #[arg(long, conflicts_with_all = ["passed", "failed"])]
    inconclusive: bool,

    /// Method used to check the expectation.
    #[arg(long)]
    method: String,

    /// Provenance label such as human:engineer or ci:github-actions.
    #[arg(long, default_value = "human:local")]
    source: String,

    /// Optional evidence id or external evidence reference.
    #[arg(long)]
    evidence: Option<String>,

    /// Optional evidence fingerprint this verification was based on.
    #[arg(long)]
    basis: Option<String>,

    /// Verification detail. Defaults to a generated summary.
    #[arg(long)]
    detail: Option<String>,

    /// Emit compact .susu syntax.
    #[arg(long)]
    minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ExpectationCommand {
    /// Add or replace one expectation in an expectation-only sidecar.
    Add(AddExpectation),

    /// List expectations from a sidecar or artifact.
    List(ListExpectations),

    /// Remove one expectation from an expectation-only sidecar.
    Remove(RemoveExpectation),
}

#[derive(Debug, Subcommand)]
enum VerificationCommand {
    /// Add or replace one verification in a verification-only sidecar.
    Add(AddVerification),

    /// List verifications from a sidecar or artifact.
    List(ListVerifications),

    /// Remove one verification from a verification-only sidecar.
    Remove(RemoveVerification),
}

#[derive(Debug, Subcommand)]
enum DecisionCommand {
    /// Add or replace one decision in a decision-only sidecar.
    Add(AddDecision),

    /// List decisions from a sidecar or artifact.
    List(ListDecisions),

    /// Remove one decision from a decision-only sidecar.
    Remove(RemoveDecision),
}

#[derive(Debug, Subcommand)]
enum WorkCommand {
    /// Add or replace one work record in a work-only sidecar.
    Add(AddWork),

    /// List work records from a sidecar or artifact.
    List(ListWorks),

    /// Remove one work record from a work-only sidecar.
    Remove(RemoveWork),
}

#[derive(Debug, Subcommand)]
enum GitCommand {
    /// Connect commits to workflows, records, and expectations.
    Connect(GitConnectArgs),

    /// Explicitly link one commit to one expectation as a work record.
    Link(GitLinkArgs),

    /// Import commits as work records.
    Import(GitImportArgs),

    /// Compare the current artifact to code evidence from an older Git ref.
    Rewind(GitRewindArgs),
}

#[derive(Debug, Args)]
struct AddExpectation {
    /// Expectation sidecar to update.
    #[arg(short, long, default_value = "expectations.susu")]
    file: PathBuf,

    /// Optional explicit id. Omit to derive a stable id from the record.
    #[arg(long)]
    id: Option<String>,

    /// Target kind: project, file, symbol, or workflow.
    #[arg(long)]
    target: ExpectationTargetArg,

    /// Target id. Required for file, symbol, and workflow expectations.
    #[arg(long)]
    subject: Option<String>,

    /// Project directory used to resolve a file path subject to its scanner id.
    #[arg(long, default_value = ".")]
    target_root: PathBuf,

    /// Status: proposed, accepted, or superseded.
    #[arg(long, default_value = "proposed")]
    status: ExpectationStatusArg,

    /// Provenance label such as human:product or policy:security.
    #[arg(long, default_value = "human:local")]
    source: String,

    /// Short expectation title.
    #[arg(long)]
    title: String,

    /// Full expectation detail.
    #[arg(long)]
    detail: String,
}

#[derive(Debug, Args)]
struct ListExpectations {
    /// Expectation sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "expectations.susu")]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct RemoveExpectation {
    /// Expectation sidecar to update.
    #[arg(short, long, default_value = "expectations.susu")]
    file: PathBuf,

    /// Expectation id to remove.
    id: String,
}

#[derive(Debug, Args)]
struct AddVerification {
    /// Verification sidecar to update.
    #[arg(short, long, default_value = "verifications.susu")]
    file: PathBuf,

    /// Optional explicit id. Omit to derive a stable id from the record.
    #[arg(long)]
    id: Option<String>,

    /// Verification id this record supersedes.
    #[arg(long)]
    supersedes: Option<String>,

    /// Expectation id being checked.
    #[arg(long)]
    expectation: String,

    /// Result: passed, failed, or inconclusive.
    #[arg(long)]
    status: VerificationStatusArg,

    /// Method used to check the expectation.
    #[arg(long)]
    method: String,

    /// Provenance label such as human:engineer or ci:github-actions.
    #[arg(long, default_value = "human:local")]
    source: String,

    /// Optional evidence id or external evidence reference.
    #[arg(long)]
    evidence: Option<String>,

    /// Optional evidence fingerprint this verification was based on.
    #[arg(long)]
    basis: Option<String>,

    /// Verification detail.
    #[arg(long)]
    detail: String,
}

#[derive(Debug, Args)]
struct ListVerifications {
    /// Verification sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "verifications.susu")]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct RemoveVerification {
    /// Verification sidecar to update.
    #[arg(short, long, default_value = "verifications.susu")]
    file: PathBuf,

    /// Verification id to remove.
    id: String,
}

#[derive(Debug, Args)]
struct AddDecision {
    /// Decision sidecar to update.
    #[arg(short, long, default_value = "decisions.susu")]
    file: PathBuf,

    /// Optional explicit id. Omit to derive a stable id from the record.
    #[arg(long)]
    id: Option<String>,

    /// Target kind: project, file, symbol, or workflow.
    #[arg(long)]
    target: ExpectationTargetArg,

    /// Target id. Required for file, symbol, and workflow decisions.
    #[arg(long)]
    subject: Option<String>,

    /// Status: proposed, accepted, rejected, or superseded.
    #[arg(long, default_value = "proposed")]
    status: DecisionStatusArg,

    /// Provenance label such as human:director or import:jira.
    #[arg(long, default_value = "human:local")]
    source: String,

    /// Optional evidence fingerprint this decision was based on.
    #[arg(long)]
    basis: Option<String>,

    /// Short decision title.
    #[arg(long)]
    title: String,

    /// Full decision detail.
    #[arg(long)]
    detail: String,
}

#[derive(Debug, Args)]
struct ListDecisions {
    /// Decision sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "decisions.susu")]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct RemoveDecision {
    /// Decision sidecar to update.
    #[arg(short, long, default_value = "decisions.susu")]
    file: PathBuf,

    /// Decision id to remove.
    id: String,
}

#[derive(Debug, Args)]
struct AddWork {
    /// Work sidecar to update.
    #[arg(short, long, default_value = "work.susu")]
    file: PathBuf,

    /// Optional explicit id. Omit to derive a stable id from the record.
    #[arg(long)]
    id: Option<String>,

    /// Target kind: project, file, symbol, or workflow.
    #[arg(long)]
    target: ExpectationTargetArg,

    /// Target id. Required for file, symbol, and workflow work records.
    #[arg(long)]
    subject: Option<String>,

    /// Optional expectation id this work addresses.
    #[arg(long)]
    expectation: Option<String>,

    /// Kind: implementation, verification, documentation, infrastructure, review, or other.
    #[arg(long, default_value = "implementation")]
    kind: WorkKindArg,

    /// Status: proposed, `in_progress`, completed, blocked, or superseded.
    #[arg(long, default_value = "completed")]
    status: WorkStatusArg,

    /// Provenance label such as human:engineer, agent:codex, or import:git.
    #[arg(long, default_value = "human:local")]
    source: String,

    /// Optional evidence id or external reference such as commit:abc123.
    #[arg(long)]
    evidence: Option<String>,

    /// Short work title.
    #[arg(long)]
    title: String,

    /// Full work detail.
    #[arg(long)]
    detail: String,
}

#[derive(Debug, Args)]
struct ListWorks {
    /// Work sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "work.susu")]
    file: PathBuf,
}

#[derive(Debug, Args)]
struct RemoveWork {
    /// Work sidecar to update.
    #[arg(short, long, default_value = "work.susu")]
    file: PathBuf,

    /// Work id to remove.
    id: String,
}

#[derive(Debug, Args)]
struct GitConnectArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Current .susu artifact to connect against.
    #[arg(long)]
    artifact: PathBuf,

    /// Starting revision or ref, such as main or HEAD~10.
    #[arg(long)]
    since: Option<String>,

    /// Ending revision or ref. Defaults to HEAD when --since is used.
    #[arg(long)]
    until: Option<String>,

    /// Maximum number of commits to inspect.
    #[arg(long)]
    limit: Option<usize>,

    /// Maximum commit connections to print.
    #[arg(long, default_value_t = 20)]
    max_items: usize,

    /// Write work records for commits marked `needs_record`.
    #[arg(long)]
    export_work: Option<PathBuf>,

    /// Provenance label for exported work records.
    #[arg(long, default_value = "import:git-connect")]
    source: String,

    /// Emit compact .susu syntax when exporting work.
    #[arg(long)]
    minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct GitShortcutArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Current .susu artifact to connect against.
    #[arg(long, default_value = ".susumu/project.susu")]
    artifact: PathBuf,

    /// Work sidecar to update with connected commits.
    #[arg(short, long, default_value = ".susumu/work.susu")]
    output: PathBuf,

    /// Starting revision or ref, such as main or HEAD~10.
    #[arg(long)]
    since: Option<String>,

    /// Ending revision or ref. Defaults to HEAD when --since is used.
    #[arg(long)]
    until: Option<String>,

    /// Maximum number of commits to inspect.
    #[arg(long, default_value_t = 25)]
    limit: usize,

    /// Maximum commit connections to print.
    #[arg(long, default_value_t = 20)]
    max_items: usize,

    /// Do not write work records; only print the connections.
    #[arg(long)]
    no_export: bool,

    /// Provenance label for exported work records.
    #[arg(long, default_value = "import:git-connect")]
    source: String,

    /// Emit compact .susu syntax when exporting work.
    #[arg(long)]
    minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct GitLinkArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Current .susu artifact containing the expectation.
    #[arg(long, default_value = ".susumu/project.susu")]
    artifact: PathBuf,

    /// Work sidecar to update.
    #[arg(short, long, default_value = ".susumu/work.susu")]
    output: PathBuf,

    /// Commit hash or ref to link.
    commit: String,

    /// Expectation id this commit supports.
    expectation: String,

    /// Provenance label for the linked work record.
    #[arg(long, default_value = "human:git-link")]
    source: String,

    /// Kind: implementation, verification, documentation, infrastructure, review, or other.
    #[arg(long, default_value = "implementation")]
    kind: WorkKindArg,

    /// Status: proposed, `in_progress`, completed, blocked, or superseded.
    #[arg(long, default_value = "completed")]
    status: WorkStatusArg,

    /// Override the work record title. Defaults to the commit subject.
    #[arg(long)]
    title: Option<String>,

    /// Add a note to the generated work detail.
    #[arg(long)]
    detail: Option<String>,

    /// Emit compact .susu syntax.
    #[arg(long)]
    minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct GitImportArgs {
    /// Git repository to read.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Work sidecar to update.
    #[arg(short, long, default_value = "work.susu")]
    output: PathBuf,

    /// Optional .susu artifact used to map changed files to evidence ids.
    #[arg(long)]
    artifact: Option<PathBuf>,

    /// How far imported commits should be targeted: project, file, or workflow.
    #[arg(long, default_value = "file")]
    target_depth: GitTargetDepthArg,

    /// Starting revision or ref, such as main or HEAD~10.
    #[arg(long)]
    since: Option<String>,

    /// Ending revision or ref. Defaults to HEAD when --since is used.
    #[arg(long)]
    until: Option<String>,

    /// Maximum number of commits to import.
    #[arg(long)]
    limit: Option<usize>,

    /// Provenance label for imported work records.
    #[arg(long, default_value = "import:git")]
    source: String,

    /// Emit compact .susu syntax.
    #[arg(long)]
    minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct GitRewindArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Older revision or ref to scan, such as HEAD~1 or main.
    #[arg(long)]
    from: String,

    /// Current .susu artifact to compare against. If omitted, scan the repository now.
    #[arg(long)]
    artifact: Option<PathBuf>,

    /// Optionally write the generated old-ref artifact for inspection.
    #[arg(long)]
    old_output: Option<PathBuf>,

    /// Emit compact .susu syntax when writing --old-output.
    #[arg(long)]
    minify: bool,

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

fn run_command(command: Command) -> Result<()> {
    match command {
        Command::Init(args) => init_repository(&args),
        Command::Check(args) => check(&args),
        Command::Diff(args) => diff(&args),
        Command::Handoff(args) => handoff(&args),
        Command::Review { args, command } => {
            if let Some(command) = command {
                match command {
                    ReviewCommand::Build(args) => build_review(&args),
                    ReviewCommand::Create(args) => create_review(&args),
                    ReviewCommand::Open(args) => open_review(&args),
                    ReviewCommand::Diff(args) => diff_reviews(&args),
                    ReviewCommand::Serve(args) => serve_review(&args),
                    ReviewCommand::ExportHtml(args) => export_review_html(&args),
                }
            } else {
                review_shortcut(&args)
            }
        }
        Command::Open(args) => open_shortcut(&args),
        Command::Status(args) => status_shortcut(&args),
        Command::Readiness(args) => readiness_shortcut(&args),
        Command::Resolve(args) => resolve_target(&args),
        Command::Expectations(args) => expectations_shortcut(&args),
        Command::Verify(args) => verify_shortcut(args),
        Command::Expectation { command } => match command {
            ExpectationCommand::Add(args) => add_expectation(args),
            ExpectationCommand::List(args) => list_expectations(&args),
            ExpectationCommand::Remove(args) => remove_expectation(&args),
        },
        Command::Verification { command } => match command {
            VerificationCommand::Add(args) => add_verification(args),
            VerificationCommand::List(args) => list_verifications(&args),
            VerificationCommand::Remove(args) => remove_verification(&args),
        },
        Command::Decision { command } => match command {
            DecisionCommand::Add(args) => add_decision(args),
            DecisionCommand::List(args) => list_decisions(&args),
            DecisionCommand::Remove(args) => remove_decision(&args),
        },
        Command::Work { command } => match command {
            WorkCommand::Add(args) => add_work(args),
            WorkCommand::List(args) => list_works(&args),
            WorkCommand::Remove(args) => remove_work(&args),
        },
        Command::Git { args, command } => {
            if let Some(command) = command {
                match command {
                    GitCommand::Connect(args) => git_connect(&args),
                    GitCommand::Link(args) => git_link(&args),
                    GitCommand::Import(args) => import_git_work(&args),
                    GitCommand::Rewind(args) => git_rewind(&args),
                }
            } else {
                git_shortcut(&args)
            }
        }
    }
}

#[derive(Debug)]
struct DailyReviewPaths {
    artifact: PathBuf,
    packet: PathBuf,
    check_json: PathBuf,
    html: PathBuf,
    work: PathBuf,
}

#[derive(Debug)]
struct ReviewBuildState {
    project_name: String,
    check: CheckReport,
}

fn review_shortcut(args: &ReviewShortcutArgs) -> Result<()> {
    let paths = daily_review_paths(&args.target, &args.output_dir);
    let work = args
        .work
        .clone()
        .or_else(|| paths.work.exists().then_some(paths.work.clone()));
    build_review(&ReviewBuildArgs {
        target: args.target.clone(),
        expectations: None,
        verifications: None,
        decisions: None,
        work,
        artifact_output: paths.artifact,
        output: paths.packet,
        check_json: Some(paths.check_json),
        html: (!args.no_html).then_some(paths.html),
        strict: args.strict,
        fail_on_check: args.fail_on_check,
        json: args.json,
        serve: args.serve,
        host: args.host.clone(),
        port: args.port,
    })
}

fn open_shortcut(args: &OpenArgs) -> Result<()> {
    if args.summary || args.tui || args.json {
        return open_review(&ReviewOpenArgs {
            packet: args.packet.clone(),
            max_items: args.max_items,
            json: args.json,
            tui: args.tui,
        });
    }

    if args.serve {
        return serve_review(&ReviewServeArgs {
            packet: args.packet.clone(),
            host: args.host.clone(),
            port: args.port,
        });
    }

    open_static_review(&args.packet)
}

fn open_static_review(packet: &Path) -> Result<()> {
    let html = packet.with_extension("html");
    if !html.is_file() {
        bail!(
            "could not find static review portal at {}; run `susumu review` first",
            html.display()
        );
    }

    let status = if cfg!(target_os = "windows") {
        ProcessCommand::new("explorer.exe").arg(&html).status()
    } else if cfg!(target_os = "macos") {
        ProcessCommand::new("open").arg(&html).status()
    } else {
        ProcessCommand::new("xdg-open").arg(&html).status()
    }
    .with_context(|| format!("could not open static review portal {}", html.display()))?;

    if !status.success() {
        bail!(
            "could not open static review portal {} (browser launcher exited with {})",
            html.display(),
            status
        );
    }

    println!("Opened static review portal: {}", html.display());
    Ok(())
}

fn status_shortcut(args: &StatusArgs) -> Result<()> {
    let paths = daily_review_paths(&args.target, &args.output_dir);
    let work = paths.work.exists().then_some(paths.work);
    check(&CheckArgs {
        target: args.target.clone(),
        expectations: None,
        verifications: None,
        decisions: None,
        work,
        strict: args.strict,
        max_items: args.max_items,
        json: args.json,
    })
}

fn readiness_shortcut(args: &ReadinessArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet).with_context(|| {
        format!(
            "could not read readiness from {}; run `susumu review` first",
            args.packet.display()
        )
    })?;
    let bucket = canonical_readiness_bucket(args.bucket.as_deref())?;
    let items = filtered_readiness_items(
        &packet.expectation_readiness,
        bucket,
        args.search.as_deref(),
    );
    if args.json {
        print_readiness_json(
            &args.packet,
            &packet,
            &items,
            bucket,
            args.search.as_deref(),
        )?;
    } else {
        print_readiness_report(
            &args.packet,
            &packet,
            &items,
            bucket,
            args.search.as_deref(),
            args.max_items,
        );
    }
    Ok(())
}

fn print_readiness_report(
    packet_path: &Path,
    packet: &ReviewPacketStored,
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
    max_items: usize,
) {
    println!("Susumu readiness: {}", packet.project.name);
    println!("Packet: {}", packet_path.display());
    println!(
        "Result: {} ({})",
        packet.result.status, packet.result.reason
    );
    println!(
        "Showing: {} of {} expectations",
        items.len(),
        packet.expectation_readiness.len()
    );
    if bucket.is_some() || search.is_some() {
        println!(
            "Filters: bucket={} search={}",
            bucket.unwrap_or("any"),
            search.unwrap_or("any")
        );
    }
    println!();
    print_readiness_counts(items);
    println!();
    print_expectation_readiness(items, max_items);
}

fn print_readiness_counts(items: &[ExpectationReadiness]) {
    println!("Readiness counts");
    for (bucket, label) in READINESS_BUCKETS {
        let count = items.iter().filter(|item| item.bucket == bucket).count();
        println!("  - {label}: {count}");
    }
}

fn print_readiness_json(
    packet_path: &Path,
    packet: &ReviewPacketStored,
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
) -> Result<()> {
    let output = readiness_json(packet_path, packet, items, bucket, search);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize readiness report")?
    );
    Ok(())
}

fn readiness_json(
    packet_path: &Path,
    packet: &ReviewPacketStored,
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
) -> serde_json::Value {
    let counts = READINESS_BUCKETS
        .iter()
        .map(|(bucket, label)| {
            serde_json::json!({
                "bucket": bucket,
                "label": label,
                "count": items
                    .iter()
                    .filter(|item| item.bucket == *bucket)
                    .count(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "packet": packet_path.display().to_string(),
        "project": &packet.project,
        "result": &packet.result,
        "total": packet.expectation_readiness.len(),
        "shown": items.len(),
        "filters": {
            "bucket": bucket,
            "search": search,
        },
        "counts": counts,
        "items": items,
    })
}

fn canonical_readiness_bucket(bucket: Option<&str>) -> Result<Option<&'static str>> {
    let Some(bucket) = bucket else {
        return Ok(None);
    };
    let normalized = normalize_readiness_filter(bucket);
    let canonical = READINESS_BUCKETS
        .iter()
        .find(|(candidate, label)| {
            normalize_readiness_filter(candidate) == normalized
                || normalize_readiness_filter(label) == normalized
        })
        .map(|(candidate, _)| *candidate);
    canonical.map(Some).with_context(|| {
        format!(
            "unknown readiness bucket `{bucket}`; expected one of: {}",
            readiness_bucket_help()
        )
    })
}

fn readiness_bucket_help() -> String {
    READINESS_BUCKETS
        .iter()
        .map(|(bucket, _)| *bucket)
        .collect::<Vec<_>>()
        .join(", ")
}

fn normalize_readiness_filter(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_separator = false;
    for ch in value.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            last_was_separator = false;
        } else if !last_was_separator && !normalized.is_empty() {
            normalized.push('_');
            last_was_separator = true;
        }
    }
    if normalized.ends_with('_') {
        normalized.pop();
    }
    normalized
}

fn filtered_readiness_items(
    items: &[ExpectationReadiness],
    bucket: Option<&str>,
    search: Option<&str>,
) -> Vec<ExpectationReadiness> {
    let search = search.map(str::to_lowercase);
    items
        .iter()
        .filter(|item| bucket.is_none_or(|bucket| item.bucket == bucket))
        .filter(|item| {
            search
                .as_deref()
                .is_none_or(|search| readiness_item_matches_search(item, search))
        })
        .cloned()
        .collect()
}

fn readiness_item_matches_search(item: &ExpectationReadiness, search: &str) -> bool {
    [
        item.expectation_id.as_str(),
        item.title.as_str(),
        item.target.as_str(),
        item.subject.as_deref().unwrap_or_default(),
        item.bucket.as_str(),
        item.label.as_str(),
        item.support_status.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(search))
}

#[derive(Debug, Serialize)]
struct ExpectationsJson {
    source: String,
    total: usize,
    shown: usize,
    search: Option<String>,
    status: Option<String>,
    expectations: Vec<ExpectationBrowseRow>,
}

#[derive(Debug, Clone, Serialize)]
struct ExpectationBrowseRow {
    id: String,
    title: String,
    detail: String,
    target: String,
    subject: Option<String>,
    status: String,
    source: String,
    support_status: Option<String>,
    target_observed: Option<bool>,
    verification: Option<ExpectationVerificationSupport>,
    work: Option<usize>,
    decisions: Option<usize>,
    findings: Option<usize>,
}

fn expectations_shortcut(args: &ExpectationsArgs) -> Result<()> {
    let (source, rows) = expectation_browse_rows(args)?;
    let status = args.status.clone().map(ExpectationStatus::from);
    let filtered = filter_expectation_rows(
        rows,
        args.search.as_deref(),
        status.as_ref().map(ToString::to_string).as_deref(),
    );
    let shown = filtered
        .iter()
        .take(args.max_items)
        .cloned()
        .collect::<Vec<_>>();

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ExpectationsJson {
                source,
                total: filtered.len(),
                shown: shown.len(),
                search: args.search.clone(),
                status: status.map(|value| value.to_string()),
                expectations: shown,
            })
            .context("could not serialize expectations")?
        );
    } else {
        print_expectations_shortcut(
            &source,
            &filtered,
            &shown,
            args.search.as_deref(),
            status.as_ref(),
        );
    }
    Ok(())
}

fn expectation_browse_rows(args: &ExpectationsArgs) -> Result<(String, Vec<ExpectationBrowseRow>)> {
    if let Some(file) = &args.file {
        let expectations = read_expectations_file(file)?;
        return Ok((
            file.display().to_string(),
            expectations
                .iter()
                .map(|expectation| expectation_browse_row(expectation, None))
                .collect(),
        ));
    }

    let paths = daily_review_paths(&args.target, Path::new(".susumu"));
    let work = paths.work.exists().then_some(paths.work);
    let analysis = load_analysis(&args.target, None, None, None, work.as_ref(), false)?;
    let support = expectation_support(&analysis)
        .into_iter()
        .map(|item| (item.expectation_id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    Ok((
        args.target.display().to_string(),
        analysis
            .expectations
            .iter()
            .map(|expectation| expectation_browse_row(expectation, support.get(&expectation.id)))
            .collect(),
    ))
}

fn expectation_browse_row(
    expectation: &Expectation,
    support: Option<&ExpectationSupport>,
) -> ExpectationBrowseRow {
    ExpectationBrowseRow {
        id: expectation.id.clone(),
        title: expectation.title.clone(),
        detail: expectation.detail.clone(),
        target: expectation.target.to_string(),
        subject: expectation.subject.clone(),
        status: expectation.status.to_string(),
        source: expectation.source.clone(),
        support_status: support.map(|support| support.support_status.clone()),
        target_observed: support.map(|support| support.target_observed),
        verification: support.map(|support| support.verification.clone()),
        work: support.map(|support| support.work),
        decisions: support.map(|support| support.decisions),
        findings: support.map(|support| support.findings),
    }
}

fn filter_expectation_rows(
    mut rows: Vec<ExpectationBrowseRow>,
    search: Option<&str>,
    status: Option<&str>,
) -> Vec<ExpectationBrowseRow> {
    if let Some(status) = status {
        rows.retain(|row| row.status == status);
    }
    if let Some(search) = search.map(str::trim).filter(|search| !search.is_empty()) {
        let search = search.to_ascii_lowercase();
        rows.retain(|row| expectation_row_matches(row, &search));
    }
    rows.sort_by(|left, right| {
        left.status
            .cmp(&right.status)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows
}

fn expectation_row_matches(row: &ExpectationBrowseRow, search: &str) -> bool {
    [
        row.id.as_str(),
        row.title.as_str(),
        row.detail.as_str(),
        row.target.as_str(),
        row.subject.as_deref().unwrap_or("-"),
        row.status.as_str(),
        row.source.as_str(),
        row.support_status.as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|value| value.to_ascii_lowercase().contains(search))
}

fn print_expectations_shortcut(
    source: &str,
    filtered: &[ExpectationBrowseRow],
    shown: &[ExpectationBrowseRow],
    search: Option<&str>,
    status: Option<&ExpectationStatus>,
) {
    println!("Susumu expectations: {}", filtered.len());
    println!("Source: {source}");
    if let Some(search) = search.filter(|value| !value.trim().is_empty()) {
        println!("Search: {search}");
    }
    if let Some(status) = status {
        println!("Status: {status}");
    }
    if shown.is_empty() {
        println!("No matching expectations.");
        return;
    }

    let mut current_status = "";
    for row in shown {
        if row.status != current_status {
            current_status = &row.status;
            println!();
            println!("{}", expectation_status_heading(current_status));
        }
        println!("  {}", row.id);
        println!("    {}", row.title);
        println!(
            "    target={} subject={} source={}",
            row.target,
            row.subject.as_deref().unwrap_or("-"),
            row.source
        );
        if let Some(support_status) = &row.support_status {
            println!(
                "    support={} observed={} verification={}/{}/{} work={} decisions={} findings={}",
                support_status,
                row.target_observed.unwrap_or(false),
                row.verification
                    .as_ref()
                    .map_or(0, |verification| verification.passed),
                row.verification
                    .as_ref()
                    .map_or(0, |verification| verification.failed),
                row.verification
                    .as_ref()
                    .map_or(0, |verification| verification.inconclusive),
                row.work.unwrap_or(0),
                row.decisions.unwrap_or(0),
                row.findings.unwrap_or(0)
            );
        }
    }
    if filtered.len() > shown.len() {
        println!();
        println!("... {} more expectations", filtered.len() - shown.len());
    }
}

fn expectation_status_heading(status: &str) -> &'static str {
    match status {
        "accepted" => "Accepted",
        "proposed" => "Proposed",
        "superseded" => "Superseded",
        _ => "Other",
    }
}

#[derive(Debug, Serialize)]
struct VerifyJson {
    file: String,
    id: String,
    expectation: String,
    status: String,
    method: String,
    evidence: Option<String>,
    source: String,
}

fn verify_shortcut(args: VerifyArgs) -> Result<()> {
    let status = verification_status_from_flags(&args)?;
    let analysis = load_analysis(&args.target, None, None, None, None, false)?;
    let expectation = analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == args.expectation)
        .with_context(|| {
            format!(
                "{} does not contain expectation {}; try `susumu expectations --search <term>`",
                args.target.display(),
                args.expectation
            )
        })?;
    let evidence = args.evidence.filter(|value| !value.trim().is_empty());
    let detail = args.detail.unwrap_or_else(|| {
        format!(
            "Recorded by susumu verify. Expectation: {} - {}. Method: {}.",
            expectation.id, expectation.title, args.method
        )
    });
    let id = args.id.unwrap_or_else(|| {
        verification_id(
            &expectation.id,
            status,
            args.supersedes.as_deref(),
            &args.method,
            &args.source,
            evidence.as_deref(),
            &detail,
        )
    });
    let verification = Verification {
        id,
        expectation_id: expectation.id.clone(),
        status,
        supersedes: args.supersedes.filter(|value| !value.trim().is_empty()),
        method: args.method,
        source: args.source,
        evidence,
        basis: args.basis.filter(|value| !value.trim().is_empty()),
        detail,
    };
    let written = write_verification_record(&args.file, verification, args.minify)?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&VerifyJson {
                file: args.file.display().to_string(),
                id: written.id,
                expectation: written.expectation_id,
                status: written.status.to_string(),
                method: written.method,
                evidence: written.evidence,
                source: written.source,
            })
            .context("could not serialize verification report")?
        );
    } else {
        println!(
            "wrote verification {} to {}",
            written.id,
            args.file.display()
        );
        println!("Expectation: {}  {}", expectation.id, expectation.title);
        println!("Status: {}", written.status);
        println!("Method: {}", written.method);
        println!("next:");
        println!("  susumu review");
    }

    Ok(())
}

fn verification_status_from_flags(args: &VerifyArgs) -> Result<VerificationStatus> {
    match (args.passed, args.failed, args.inconclusive) {
        (true, false, false) => Ok(VerificationStatus::Passed),
        (false, true, false) => Ok(VerificationStatus::Failed),
        (false, false, true) => Ok(VerificationStatus::Inconclusive),
        (false, false, false) => {
            bail!("choose one verification status: --passed, --failed, or --inconclusive")
        }
        _ => bail!("choose only one verification status"),
    }
}

fn write_verification_record(
    file: &Path,
    verification: Verification,
    minify: bool,
) -> Result<Verification> {
    let mut verifications = if file.exists() {
        read_verification_sidecar(&file.to_path_buf())?
    } else {
        Vec::new()
    };
    if let Some(existing) = verifications
        .iter()
        .find(|current| current.id == verification.id)
    {
        if existing == &verification {
            return Ok(existing.clone());
        }
        bail!(
            "verification {} already exists; use a new id and --supersedes to preserve history",
            verification.id
        );
    }
    if let Some(superseded_id) = verification.supersedes.as_deref() {
        let Some(superseded) = verifications
            .iter()
            .find(|current| current.id == superseded_id)
        else {
            bail!(
                "verification {} cannot supersede missing verification {}",
                verification.id,
                superseded_id
            );
        };
        if superseded.expectation_id != verification.expectation_id {
            bail!(
                "verification {} must supersede a record for expectation {}, not {}",
                verification.id,
                verification.expectation_id,
                superseded.expectation_id
            );
        }
    }
    merge_verifications(&mut verifications, vec![verification.clone()]);
    write_text_file(file, &write_verifications(&verifications, minify)?)?;
    Ok(verification)
}

fn git_shortcut(args: &GitShortcutArgs) -> Result<()> {
    let artifact = git_shortcut_artifact(args)?;
    let connect_args = git_shortcut_connect_args(args);
    run_git_connect(&connect_args, &artifact)
}

fn git_shortcut_artifact(args: &GitShortcutArgs) -> Result<ProjectAnalysis> {
    let work = args.output.exists().then_some(&args.output);
    load_analysis(&args.artifact, None, None, None, work, false)
}

fn git_shortcut_connect_args(args: &GitShortcutArgs) -> GitConnectArgs {
    GitConnectArgs {
        repo: args.repo.clone(),
        artifact: args.artifact.clone(),
        since: args.since.clone(),
        until: args.until.clone(),
        limit: Some(args.limit),
        max_items: args.max_items,
        export_work: (!args.no_export).then_some(args.output.clone()),
        source: args.source.clone(),
        minify: args.minify,
        json: args.json,
    }
}

fn daily_review_paths(target: &Path, output_dir: &Path) -> DailyReviewPaths {
    let base = conventional_output_dir(target, output_dir);
    DailyReviewPaths {
        artifact: base.join("project.susu"),
        packet: base.join("review.susu"),
        check_json: base.join("check.json"),
        html: base.join("review.html"),
        work: base.join("work.susu"),
    }
}

fn conventional_output_dir(target: &Path, output_dir: &Path) -> PathBuf {
    if output_dir.is_absolute() {
        return output_dir.to_path_buf();
    }
    if target.is_dir() {
        target.join(output_dir)
    } else {
        output_dir.to_path_buf()
    }
}

fn load_analysis(
    target: &PathBuf,
    expectations: Option<&PathBuf>,
    verifications: Option<&PathBuf>,
    decisions: Option<&PathBuf>,
    work: Option<&PathBuf>,
    log_merges: bool,
) -> Result<ProjectAnalysis> {
    let is_artifact = target
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("susu"));

    let mut analysis = if is_artifact {
        let source = fs::read_to_string(target)
            .with_context(|| format!("could not read {}", target.display()))?;
        parse_susu(&source).with_context(|| format!("could not parse {}", target.display()))?
    } else {
        if !target.is_dir() {
            bail!("{} is not a directory or .susu file", target.display());
        }
        scan_project(target)?
    };
    refresh_derived_analysis(&mut analysis);

    let discovered_expectations = if expectations.is_none() && !is_artifact {
        let candidate = target.join("expectations.susu");
        candidate.exists().then_some(candidate)
    } else {
        None
    };
    let expectations = expectations.or(discovered_expectations.as_ref());
    let discovered_verifications = if verifications.is_none() && !is_artifact {
        let candidate = target.join("verifications.susu");
        candidate.exists().then_some(candidate)
    } else {
        None
    };
    let verifications = verifications.or(discovered_verifications.as_ref());

    if let Some(expectations) = expectations {
        let source = fs::read_to_string(expectations)
            .with_context(|| format!("could not read {}", expectations.display()))?;
        let imported = parse_expectations(&source).with_context(|| {
            format!(
                "could not parse expectations from {}",
                expectations.display()
            )
        })?;
        let count = imported.len();
        merge_expectations(&mut analysis.expectations, imported);
        refresh_derived_analysis(&mut analysis);
        if log_merges {
            eprintln!(
                "merged {count} expectations from {}",
                expectations.display()
            );
        }
    }

    if let Some(verifications) = verifications {
        let source = fs::read_to_string(verifications)
            .with_context(|| format!("could not read {}", verifications.display()))?;
        let imported = parse_verifications(&source).with_context(|| {
            format!(
                "could not parse verifications from {}",
                verifications.display()
            )
        })?;
        let count = imported.len();
        merge_verifications(&mut analysis.verifications, imported);
        anchor_verification_bases(&mut analysis);
        refresh_derived_analysis(&mut analysis);
        if log_merges {
            eprintln!(
                "merged {count} verifications from {}",
                verifications.display()
            );
        }
    }

    if let Some(decisions) = decisions {
        let source = fs::read_to_string(decisions)
            .with_context(|| format!("could not read {}", decisions.display()))?;
        let imported = parse_decisions(&source)
            .with_context(|| format!("could not parse decisions from {}", decisions.display()))?;
        let count = imported.len();
        merge_decisions(&mut analysis.decisions, imported);
        anchor_decision_bases(&mut analysis);
        refresh_derived_analysis(&mut analysis);
        if log_merges {
            eprintln!("merged {count} decisions from {}", decisions.display());
        }
    }

    if let Some(work) = work {
        let source = fs::read_to_string(work)
            .with_context(|| format!("could not read {}", work.display()))?;
        let imported = parse_works(&source)
            .with_context(|| format!("could not parse work from {}", work.display()))?;
        let count = imported.len();
        merge_works(&mut analysis.works, imported);
        refresh_derived_analysis(&mut analysis);
        if log_merges {
            eprintln!("merged {count} work records from {}", work.display());
        }
    }

    Ok(analysis)
}

fn init_repository(args: &InitArgs) -> Result<()> {
    if !args.target.is_dir() {
        bail!("{} is not a directory", args.target.display());
    }

    let file = if args.file.is_absolute() {
        args.file.clone()
    } else {
        args.target.join(&args.file)
    };

    if file.exists() && !args.force {
        bail!(
            "{} already exists; use --force to replace it",
            file.display()
        );
    }

    if let Some(parent) = file.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let project_name = args.name.clone().unwrap_or_else(|| {
        args.target
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("project")
            .to_owned()
    });
    let expectations = starter_expectations(&project_name, &args.source);
    fs::write(&file, write_expectations(&expectations, false)?)
        .with_context(|| format!("could not write {}", file.display()))?;

    eprintln!(
        "wrote {} starter expectations to {}",
        expectations.len(),
        file.display()
    );
    eprintln!("next: susumu review {}", args.target.display());
    Ok(())
}

fn starter_expectations(project_name: &str, source: &str) -> Vec<Expectation> {
    let records = [
        (
            ExpectationStatus::Accepted,
            format!("{project_name} keeps expectations explicit"),
            format!(
                "{project_name} should keep project expectations in an authored expectations.susu sidecar so implementation evidence and intent can be reviewed together."
            ),
        ),
        (
            ExpectationStatus::Proposed,
            format!("{project_name} documents primary workflows"),
            format!(
                "{project_name} should describe the business or product workflows that matter most, then link those expectations to observed files, symbols, or workflows as evidence improves."
            ),
        ),
        (
            ExpectationStatus::Proposed,
            format!("{project_name} records verification evidence"),
            format!(
                "{project_name} should record how important expectations are checked, such as tests, CI runs, manual reviews, policy checks, runtime traces, or release approvals."
            ),
        ),
    ];

    records
        .into_iter()
        .map(|(status, title, detail)| {
            let id = expectation_id(
                ExpectationTarget::Project,
                None,
                status,
                source,
                &title,
                &detail,
            );
            Expectation {
                id,
                target: ExpectationTarget::Project,
                subject: None,
                status,
                source: source.to_owned(),
                title,
                detail,
            }
        })
        .collect()
}

fn write_text_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("could not write {}", path.display()))
}

fn check(args: &CheckArgs) -> Result<()> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        false,
    )?;
    let report = check_report(&analysis, args.strict);
    if args.json {
        print_check_json(&analysis, &report)?;
    } else {
        print_check_report(&analysis, &report, args.max_items);
    }
    if report.failed {
        process::exit(1);
    }
    Ok(())
}

fn diff(args: &DiffArgs) -> Result<()> {
    let old = read_analysis_artifact(&args.old)?;
    let new = read_analysis_artifact(&args.new)?;
    let report = diff_report(&old, &new);
    if args.json {
        print_diff_json(&old, &new, &report, args.fail_on_stale)?;
    } else {
        print_diff_report(&old, &new, &report, args.max_items);
    }
    if args.fail_on_stale && !report.stale_items.is_empty() {
        process::exit(1);
    }
    Ok(())
}

fn handoff(args: &HandoffArgs) -> Result<()> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        false,
    )?;
    let check = check_report(&analysis, false);
    let report = handoff_report(&analysis, &check);
    if args.json {
        print_handoff_json(&analysis, &check, &report)?;
    } else {
        print_handoff_report(&analysis, &check, &report, args.max_items);
    }
    Ok(())
}

fn create_review(args: &ReviewCreateArgs) -> Result<()> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        false,
    )?;
    let check = check_report(&analysis, args.strict);
    let handoff = handoff_report(&analysis, &check);
    let packet = review_packet(
        args.target.display().to_string(),
        current_unix_seconds(),
        &analysis,
        &check,
        &handoff,
    );
    let packet_json =
        serde_json::to_string_pretty(&packet).context("could not serialize review packet")?;

    if let Some(output) = &args.output {
        fs::write(output, &packet_json)
            .with_context(|| format!("could not write {}", output.display()))?;
        eprintln!("wrote review packet {}", output.display());
    }

    if args.json || args.output.is_none() {
        println!("{packet_json}");
    } else {
        println!("Susumu review packet: {}", analysis.project_name);
        println!("Root: {}", analysis.root);
        println!(
            "Review: {} critical, {} warning, {} attention ({})",
            check.critical,
            check.warning,
            check.attention,
            check_result_reason(&check)
        );
        println!("Top workflows: {}", handoff.top_workflows.len());
        println!("Suggested next actions: {}", handoff.next_actions.len());
    }

    Ok(())
}

fn build_review(args: &ReviewBuildArgs) -> Result<()> {
    let state = write_review_build_outputs(args)?;
    print_review_build_summary(args, &state)?;

    if args.serve {
        serve_review(&ReviewServeArgs {
            packet: args.output.clone(),
            host: args.host.clone(),
            port: args.port,
        })?;
    }

    if args.fail_on_check && state.check.failed {
        process::exit(1);
    }
    Ok(())
}

fn write_review_build_outputs(args: &ReviewBuildArgs) -> Result<ReviewBuildState> {
    let analysis = load_analysis(
        &args.target,
        args.expectations.as_ref(),
        args.verifications.as_ref(),
        args.decisions.as_ref(),
        args.work.as_ref(),
        true,
    )?;
    write_text_file(&args.artifact_output, &write_susu(&analysis, false)?)?;

    let check = check_report(&analysis, args.strict);
    write_review_build_check_json(args, &analysis, &check)?;
    write_review_build_packet(args, &analysis, &check)?;

    Ok(ReviewBuildState {
        project_name: analysis.project_name,
        check,
    })
}

fn write_review_build_check_json(
    args: &ReviewBuildArgs,
    analysis: &ProjectAnalysis,
    check: &CheckReport,
) -> Result<()> {
    let Some(check_output) = &args.check_json else {
        return Ok(());
    };
    let check_json = check_json(analysis, check);
    write_text_file(
        check_output,
        &serde_json::to_string_pretty(&check_json).context("could not serialize check report")?,
    )
}

fn write_review_build_packet(
    args: &ReviewBuildArgs,
    analysis: &ProjectAnalysis,
    check: &CheckReport,
) -> Result<()> {
    let handoff = handoff_report(analysis, check);
    let packet = review_packet(
        args.target.display().to_string(),
        current_unix_seconds(),
        analysis,
        check,
        &handoff,
    );
    let packet_json =
        serde_json::to_string_pretty(&packet).context("could not serialize review packet")?;
    write_text_file(&args.output, &packet_json)?;
    write_review_build_html(args, &packet_json)
}

fn write_review_build_html(args: &ReviewBuildArgs, packet_json: &str) -> Result<()> {
    let Some(html_output) = &args.html else {
        return Ok(());
    };
    let stored_packet: ReviewPacketStored =
        serde_json::from_str(packet_json).context("could not read built review packet")?;
    let config = load_portal_config_for_target(&args.target)?;
    write_text_file(
        html_output,
        &review_portal_html_with_config(&stored_packet, &config)?,
    )
}

fn print_review_build_summary(args: &ReviewBuildArgs, state: &ReviewBuildState) -> Result<()> {
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "project": state.project_name,
                "artifact": args.artifact_output,
                "review_packet": args.output,
                "check_json": args.check_json,
                "html": args.html,
                "result": {
                    "status": if state.check.failed { "failed" } else { "passed" },
                    "failed": state.check.failed,
                    "strict": state.check.strict,
                    "reason": check_result_reason(&state.check),
                },
                "review": {
                    "critical": state.check.critical,
                    "warning": state.check.warning,
                    "attention": state.check.attention,
                }
            }))
            .context("could not serialize review build summary")?
        );
    } else {
        println!("Susumu review build: {}", state.project_name);
        println!("Artifact: {}", args.artifact_output.display());
        println!("Review packet: {}", args.output.display());
        if let Some(check_output) = &args.check_json {
            println!("Check JSON: {}", check_output.display());
        }
        if let Some(html_output) = &args.html {
            println!("HTML portal: {}", html_output.display());
        }
        println!(
            "Review: {} critical, {} warning, {} attention ({})",
            state.check.critical,
            state.check.warning,
            state.check.attention,
            check_result_reason(&state.check)
        );
    }

    Ok(())
}

fn open_review(args: &ReviewOpenArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&packet).context("could not serialize review packet")?
        );
        return Ok(());
    }
    if args.tui {
        return tui::run(packet.artifact, None);
    }
    print_review_packet(&packet, args.max_items);
    Ok(())
}

fn diff_reviews(args: &ReviewDiffArgs) -> Result<()> {
    let old = read_review_packet(&args.old)?;
    let new = read_review_packet(&args.new)?;
    let report = review_diff_report(&old, &new);
    if args.json {
        print_review_diff_json(args, &old, &new, &report)?;
    } else {
        print_review_diff(&old, &new, &report, args.max_items);
    }
    if args.fail_on_regression && review_diff_regressed(&old, &new) {
        process::exit(1);
    }
    Ok(())
}

fn serve_review(args: &ReviewServeArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet)?;
    let config = load_portal_config_for_packet(&packet, &args.packet)?;
    let html = review_portal_html_with_config(&packet, &config)?;
    let packet_json =
        serde_json::to_string_pretty(&packet).context("could not serialize review packet")?;
    let listener =
        TcpListener::bind(format!("{}:{}", args.host, args.port)).with_context(|| {
            format!(
                "could not bind review server to {}:{}",
                args.host, args.port
            )
        })?;
    let address = listener
        .local_addr()
        .context("could not read review server address")?;
    println!("Susumu review portal: http://{address}/");
    println!("Serving {}", args.packet.display());
    println!("Press Ctrl+C to stop.");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(error) = handle_review_request(stream, &html, &packet_json) {
                    eprintln!("warning: could not serve review request: {error}");
                }
            }
            Err(error) => eprintln!("warning: review server connection failed: {error}"),
        }
    }
    Ok(())
}

fn export_review_html(args: &ReviewExportHtmlArgs) -> Result<()> {
    let packet = read_review_packet(&args.packet)?;
    let config = load_portal_config_for_packet(&packet, &args.packet)?;
    let html = review_portal_html_with_config(&packet, &config)?;
    fs::write(&args.output, html)
        .with_context(|| format!("could not write {}", args.output.display()))?;
    println!("wrote review portal {}", args.output.display());
    Ok(())
}

fn read_review_packet(path: &PathBuf) -> Result<ReviewPacketStored> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut packet = serde_json::from_str::<ReviewPacketStored>(&source)
        .with_context(|| format!("could not parse review packet {}", path.display()))?;
    if packet.schema_version != "susumu.review.v1" {
        bail!(
            "{} uses unsupported review schema `{}`",
            path.display(),
            packet.schema_version
        );
    }
    refresh_derived_analysis(&mut packet.artifact);
    Ok(packet)
}

const PORTAL_CONFIG_FILE: &str = "susumu.toml";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct PortalConfig {
    title: Option<String>,
    css_vars: BTreeMap<String, String>,
}

fn handle_review_request(mut stream: TcpStream, html: &str, packet_json: &str) -> Result<()> {
    let mut buffer = [0_u8; 8192];
    let bytes = stream
        .read(&mut buffer)
        .context("could not read HTTP request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let Some(request_line) = request.lines().next() else {
        return write_http_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "missing request line",
        );
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/");
    let path = path.split('?').next().unwrap_or(path);
    if !matches!(method, "GET" | "HEAD") {
        return write_http_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "method not allowed",
        );
    }
    let (status, content_type, body) = match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", html),
        "/review.json" => ("200 OK", "application/json; charset=utf-8", packet_json),
        "/healthz" => ("200 OK", "text/plain; charset=utf-8", "ok"),
        _ => ("404 Not Found", "text/plain; charset=utf-8", "not found"),
    };
    if method == "HEAD" {
        write_http_head(&mut stream, status, content_type, body.len())
    } else {
        write_http_response(&mut stream, status, content_type, body)
    }
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<()> {
    write_http_head(stream, status, content_type, body.len())?;
    stream
        .write_all(body.as_bytes())
        .context("could not write HTTP response body")
}

fn write_http_head(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    content_length: usize,
) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(head.as_bytes())
        .context("could not write HTTP response head")
}

fn load_portal_config_for_target(target: &Path) -> Result<PortalConfig> {
    let config_path = if target.is_dir() {
        target.join(PORTAL_CONFIG_FILE)
    } else {
        target
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PORTAL_CONFIG_FILE)
    };
    read_portal_config(&config_path)
}

fn load_portal_config_for_packet(
    packet: &ReviewPacketStored,
    packet_path: &Path,
) -> Result<PortalConfig> {
    let project_root = PathBuf::from(&packet.project.root);
    let config_path = if project_root.is_dir() {
        project_root.join(PORTAL_CONFIG_FILE)
    } else {
        packet_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PORTAL_CONFIG_FILE)
    };
    read_portal_config(&config_path)
}

fn read_portal_config(path: &Path) -> Result<PortalConfig> {
    if !path.exists() {
        return Ok(PortalConfig::default());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_portal_config(&source).with_context(|| format!("could not parse {}", path.display()))
}

fn parse_portal_config(source: &str) -> Result<PortalConfig> {
    let mut config = PortalConfig::default();
    let mut in_portal = false;
    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_portal = line == "[portal]";
            continue;
        }
        if !in_portal {
            continue;
        }
        let (key, raw_value) = line
            .split_once('=')
            .with_context(|| format!("portal config line {line_number} must use key = value"))?;
        let key = key.trim();
        let value = parse_portal_config_value(raw_value.trim())
            .with_context(|| format!("invalid portal config value on line {line_number}"))?;
        apply_portal_config_value(&mut config, key, value)
            .with_context(|| format!("invalid portal config key `{key}` on line {line_number}"))?;
    }
    Ok(config)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quote == Some('"') => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '#' if quote.is_none() => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_portal_config_value(raw: &str) -> Result<String> {
    if raw.starts_with('"') {
        return serde_json::from_str(raw).context("double-quoted values must be valid strings");
    }
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        return Ok(raw[1..raw.len() - 1].to_owned());
    }
    Ok(raw.to_owned())
}

fn apply_portal_config_value(config: &mut PortalConfig, key: &str, value: String) -> Result<()> {
    match key {
        "title" => {
            config.title = (!value.trim().is_empty()).then_some(value);
        }
        "background" | "bg" => config.set_color("--bg", &value)?,
        "panel" => config.set_color("--panel", &value)?,
        "panel2" => config.set_color("--panel2", &value)?,
        "text" => config.set_color("--text", &value)?,
        "muted" => config.set_color("--muted", &value)?,
        "line" => config.set_color("--line", &value)?,
        "accent" => config.set_color("--accent", &value)?,
        "accent2" => config.set_color("--accent2", &value)?,
        "bad" => config.set_color("--bad", &value)?,
        "warn" => config.set_color("--warn", &value)?,
        "ok" => config.set_color("--ok", &value)?,
        _ => bail!("supported keys are title and portal color names"),
    }
    Ok(())
}

impl PortalConfig {
    fn set_color(&mut self, css_var: &str, value: &str) -> Result<()> {
        if !is_hex_color(value) {
            bail!("portal colors must be #rgb or #rrggbb hex values");
        }
        self.css_vars.insert(css_var.to_owned(), value.to_owned());
        Ok(())
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|character| character.is_ascii_hexdigit())
}

fn portal_config_style(config: &PortalConfig) -> String {
    if config.css_vars.is_empty() {
        return String::new();
    }
    let declarations = config
        .css_vars
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(":root{{{declarations}}}")
}

#[cfg(test)]
fn review_portal_html(packet: &ReviewPacketStored) -> Result<String> {
    review_portal_html_with_config(packet, &PortalConfig::default())
}

fn review_portal_html_with_config(
    packet: &ReviewPacketStored,
    config: &PortalConfig,
) -> Result<String> {
    let packet_json = serde_json::to_string(packet)
        .context("could not serialize packet for review portal")?
        .replace("</", "<\\/");
    let portal_title = config.title.as_deref().unwrap_or("Susumu Review");
    let portal_eyebrow = config.title.as_deref().unwrap_or("Susumu review packet");
    Ok(review_portal_template()
        .replace("__SUSUMU_PORTAL_TITLE__", &html_escape(portal_title))
        .replace("__SUSUMU_PORTAL_EYEBROW__", &html_escape(portal_eyebrow))
        .replace("__SUSUMU_PORTAL_THEME__", &portal_config_style(config))
        .replace(
            "__SUSUMU_REVIEW_TITLE__",
            &html_escape(&packet.project.name),
        )
        .replace("__SUSUMU_REVIEW_DATA__", &packet_json))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[allow(clippy::too_many_lines)]
fn review_portal_template() -> &'static str {
    r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__SUSUMU_PORTAL_TITLE__ &middot; __SUSUMU_REVIEW_TITLE__</title>
<style>
:root{color-scheme:dark;--bg:#11131a;--panel:#1a1f2b;--panel2:#202638;--text:#e8e2d7;--muted:#aaa292;--line:#363b49;--accent:#9eb7a0;--accent2:#aaa2bf;--bad:#cc8e8a;--warn:#c8aa72;--ok:#91ad86}
__SUSUMU_PORTAL_THEME__
*{box-sizing:border-box}body{margin:0;font-family:Inter,ui-sans-serif,system-ui,-apple-system,Segoe UI,sans-serif;background:radial-gradient(circle at 20% -10%,#282f3f 0,#11131a 38%),var(--bg);color:var(--text)}
.shell{max-width:1220px;margin:0 auto;padding:40px 22px 70px}.hero{display:grid;grid-template-columns:1.4fr .8fr;gap:20px;align-items:stretch}.card{min-width:0;max-width:100%;overflow:hidden;background:linear-gradient(180deg,rgba(255,255,255,.045),rgba(255,255,255,.02));border:1px solid var(--line);border-radius:24px;box-shadow:0 24px 70px rgba(0,0,0,.22);padding:24px;backdrop-filter:blur(12px)}
.eyebrow{color:var(--accent);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{font-size:clamp(34px,6vw,68px);line-height:.94;margin:12px 0}.sub{color:var(--muted);font-size:16px;line-height:1.6}.pill{display:inline-flex;gap:8px;align-items:center;border:1px solid var(--line);border-radius:999px;padding:7px 11px;color:var(--muted);font-size:13px;margin:4px 4px 0 0}.pill strong{color:var(--text)}
.result{font-size:28px;font-weight:850}.failed{color:var(--bad)}.passed{color:var(--ok)}.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-top:18px}.metric{background:rgba(255,255,255,.045);border:1px solid var(--line);border-radius:18px;padding:16px}.metric b{display:block;font-size:28px}.metric span{color:var(--muted);font-size:13px}
.toolbar{position:sticky;top:0;z-index:3;margin:26px -8px 20px;padding:10px 8px;background:linear-gradient(180deg,rgba(17,19,26,.98),rgba(17,19,26,.78));backdrop-filter:blur(12px)}button{appearance:none;border:1px solid var(--line);border-radius:999px;background:#1b2130;color:var(--text);padding:10px 14px;margin:4px;cursor:pointer;transition:.18s ease}button:hover,button.active{border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.12);transform:translateY(-1px)}
.section{display:none;animation:rise .28s ease}.section.active{display:block}@keyframes rise{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:none}}h2{font-size:28px;margin:0 0 14px}.list{display:grid;gap:12px;min-width:0}.item{min-width:0;overflow-wrap:anywhere;border:1px solid var(--line);border-radius:18px;background:rgba(255,255,255,.03);padding:16px}.item.clickable{cursor:pointer;transition:.18s ease}.item.clickable:hover,.item.selected{border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.11);transform:translateY(-1px)}.item h3{margin:0 0 8px;font-size:17px}.meta{color:var(--muted);font-size:13px;line-height:1.5}.detail{color:#d5ccbd;line-height:1.55}.tag{display:inline-block;border-radius:999px;padding:4px 9px;margin-right:6px;font-size:12px;background:#252b3b;color:#ddd6ca}.critical{background:rgba(204,142,138,.15);color:#ead0cc}.warning{background:rgba(200,170,114,.14);color:#eadab8}.attention{background:rgba(170,162,191,.15);color:#ded8e8}.workflow-score{font-size:24px;color:var(--accent);font-weight:850}.cols{display:grid;grid-template-columns:1fr 1fr;gap:16px}.workflow-layout{display:grid;grid-template-columns:minmax(280px,.8fr) minmax(0,1.2fr);gap:16px;align-items:start;min-width:0;max-width:100%}.workflow-layout>*{min-width:0}.detail-pane{position:sticky;top:98px;align-self:start;min-width:0;max-width:100%;overflow:hidden}.traceability-layout{height:calc(100vh - 180px);min-height:540px;align-items:stretch}.traceability-list,.traceability-detail{min-width:0;max-width:100%;min-height:0;overflow:auto;overscroll-behavior:contain;padding:8px 6px 0 0}.traceability-detail{position:static;align-self:stretch}.mini{display:grid;gap:8px;min-width:0}.mini .item{padding:12px}.ladder{display:grid;gap:10px;margin:10px 0 16px}.ladder-step{position:relative;border:1px solid var(--line);border-radius:16px;background:rgba(255,255,255,.03);padding:13px 14px 13px 46px}.ladder-step:before{content:'';position:absolute;left:17px;top:18px;width:12px;height:12px;border-radius:999px;background:var(--muted);box-shadow:0 0 0 5px rgba(170,162,146,.09)}.ladder-step:after{content:'';position:absolute;left:22px;top:36px;bottom:-18px;width:2px;background:var(--line)}.ladder-step:last-child:after{display:none}.ladder-step.good{border-color:rgba(145,173,134,.45)}.ladder-step.good:before{background:var(--ok);box-shadow:0 0 0 5px rgba(145,173,134,.12)}.ladder-step.warn{border-color:rgba(200,170,114,.48)}.ladder-step.warn:before{background:var(--warn);box-shadow:0 0 0 5px rgba(200,170,114,.12)}.ladder-step.bad{border-color:rgba(204,142,138,.5)}.ladder-step.bad:before{background:var(--bad);box-shadow:0 0 0 5px rgba(204,142,138,.12)}.ladder-label{display:block;color:var(--muted);font-size:12px;font-weight:800;letter-spacing:.08em;text-transform:uppercase}.ladder-step strong{display:block;margin-top:3px}.ladder-step small{display:block;color:#d5ccbd;line-height:1.45;margin-top:4px}.next-action{border-color:rgba(158,183,160,.45);background:linear-gradient(135deg,rgba(158,183,160,.12),rgba(170,162,191,.08))}.search{width:100%;border:1px solid var(--line);border-radius:16px;background:#171b26;color:var(--text);padding:13px 15px;margin:0 0 14px}.empty{color:var(--muted);border:1px dashed var(--line);border-radius:18px;padding:22px;text-align:center}.code{max-width:100%;overflow:auto;background:#131821;border:1px solid #32394a;border-radius:16px;padding:12px;font:13px/1.55 ui-monospace,SFMono-Regular,Consolas,Menlo,monospace}.code-line{display:grid;grid-template-columns:64px minmax(0,1fr);min-width:max-content}.code-line.mark{background:rgba(158,183,160,.09);border-left:3px solid var(--accent)}.ln{color:#777f8f;text-align:right;padding-right:14px;user-select:none}.src{white-space:pre}
@media(max-width:850px){.hero,.cols,.workflow-layout{grid-template-columns:1fr}.grid{grid-template-columns:repeat(2,1fr)}.detail-pane{position:static}.traceability-layout{height:auto;min-height:0}.traceability-list,.traceability-detail{overflow:visible;padding-right:0}}
</style>
</head>
<body>
<div class="shell">
  <header class="hero">
    <div class="card">
      <div class="eyebrow">__SUSUMU_PORTAL_EYEBROW__</div>
      <h1 id="projectName"></h1>
      <p class="sub" id="projectSub"></p>
      <div id="pills"></div>
    </div>
    <div class="card">
      <div class="eyebrow">Current result</div>
      <div id="result" class="result"></div>
      <p class="sub" id="resultReason"></p>
      <div class="grid">
        <div class="metric"><b id="critical"></b><span>critical</span></div>
        <div class="metric"><b id="warning"></b><span>warnings</span></div>
        <div class="metric"><b id="attention"></b><span>attention</span></div>
        <div class="metric"><b id="workflows"></b><span>workflows</span></div>
      </div>
    </div>
  </header>
  <nav class="toolbar" id="tabs"></nav>
  <input class="search" id="search" placeholder="Filter visible section&hellip;">
  <main id="sections"></main>
</div>
<script>
const packet = __SUSUMU_REVIEW_DATA__;
const $ = (id) => document.getElementById(id);
const esc = (v) => String(v ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const list = (items, render, empty='Nothing here yet.') => items && items.length ? `<div class="list">${items.map(render).join('')}</div>` : `<div class="empty">${empty}</div>`;
const severity = (s) => s === 'critical' ? 'critical' : s === 'warning' ? 'warning' : 'attention';
function item(title, body, meta='', tags='', extra=''){return `<article class="item">${tags}<h3>${esc(title)}</h3>${meta?`<div class="meta">${meta}</div>`:''}<div class="detail">${esc(body)}</div>${extra}</article>`}
let selectedWorkflowId = null;
let selectedExpectationId = null;
const tabs = [
 ['overview','Overview'],
 ['readiness','Readiness'],
 ['review','Review'],
 ['workflows','Top workflows'],
 ['traceability','Traceability'],
 ['source','Source'],
 ['records','Records'],
 ['dirty','Dirty/stale'],
 ['artifact','Artifact'],
 ['actions','Next actions']
];
function section(id,title,html){return `<section class="section" id="section-${id}"><div class="card"><h2>${title}</h2>${html}</div></section>`}
function tokenHtml(line){return line.tokens&&line.tokens.length?line.tokens.map(t=>`<span style="color:${esc(t.color)}">${esc(t.text)}</span>`).join(''):esc(line.text)}
function codePreviewBlock(p){return `<div class="code">${(p.lines||[]).map(line=>`<div class="code-line ${line.number>=p.highlight_start&&line.number<=p.highlight_end?'mark':''}"><span class="ln">${line.number}</span><span class="src">${tokenHtml(line)}</span></div>`).join('')}</div>`}
function codePreview(p){return `<article class="item"><h3>${esc(p.path)}</h3><div class="meta">${esc(p.language)} &middot; lines ${p.start_line}-${p.end_line} &middot; highlight ${p.highlight_start}-${p.highlight_end}</div>${codePreviewBlock(p)}</article>`}
function fileById(id){return (packet.artifact.files||[]).find(f=>f.id===id)}
function symbolById(id){return (packet.artifact.symbols||[]).find(s=>s.id===id)}
function previewForLocation(fileId,location){if(!fileId)return null;const previews=packet.source_previews||[];if(location){const exact=previews.find(p=>p.file_id===fileId&&p.highlight_start===location.start_line&&p.highlight_end===location.end_line);if(exact)return exact;}return previews.find(p=>p.file_id===fileId)||null}
function targetPreview(target,subject){if(!subject)return null;if(target==='workflow'){const w=workflowById(subject);return w?previewForLocation(w.file_id,w.location):null;}if(target==='symbol'){const s=symbolById(subject);return s?previewForLocation(s.file_id,s.location):null;}if(target==='file')return previewForLocation(subject,null);return null}
function sourcePreviewExtra(p){return p?`<div style="margin-top:12px">${codePreviewBlock(p)}</div>`:''}
function sourceMetaForPreview(p){return p?` &middot; source=${esc(p.path)}:${p.highlight_start}`:''}
function workflows(){return packet.artifact.workflows||[]}
function workflowById(id){return workflows().find(w=>w.id===id)}
function workflowSummary(id){return (packet.top_workflows||[]).find(w=>w.id===id)}
function workflowExpectations(id){return (packet.artifact.expectations||[]).filter(e=>e.target==='workflow'&&e.subject===id)}
function workflowVerifications(id){const ids=new Set(workflowExpectations(id).map(e=>e.id));return (packet.artifact.verifications||[]).filter(v=>ids.has(v.expectation_id))}
function workflowDecisions(id){return (packet.artifact.decisions||[]).filter(d=>d.target==='workflow'&&d.subject===id)}
function workflowWork(id){const ids=new Set(workflowExpectations(id).map(e=>e.id));return (packet.artifact.works||[]).filter(w=>(w.target==='workflow'&&w.subject===id)||(w.expectation_id&&ids.has(w.expectation_id)))}
function workflowPreview(id){const w=workflowById(id);return w?previewForLocation(w.file_id,w.location):null}
function workflowCard(w){const summary=workflowSummary(w.id)||{score:0,detail:'Workflow detected from scanner evidence.',expectations:workflowExpectations(w.id).length,verifications:workflowVerifications(w.id).length,work:workflowWork(w.id).length};return `<article class="item clickable ${w.id===selectedWorkflowId?'selected':''}" data-workflow-id="${esc(w.id)}"><div class="workflow-score">${summary.score}</div><h3>${esc(w.trigger)}</h3><div class="meta">${esc(w.id)} &middot; ${esc(w.framework)} &middot; expectations=${summary.expectations} &middot; verifications=${summary.verifications} &middot; work=${summary.work}</div><div class="detail">${esc(summary.detail)}</div></article>`}
function miniList(items,render,empty){return items&&items.length?`<div class="mini">${items.map(render).join('')}</div>`:`<div class="empty">${empty}</div>`}
function verificationItem(v){const e=expectationById(v.expectation_id);const p=e?targetPreview(e.target,e.subject):null;return item(`${v.status} verification`,v.detail,`${esc(v.id)} &middot; method=${esc(v.method)} &middot; evidence=${esc(v.evidence??'-')} &middot; basis=${esc(v.basis??'-')}${sourceMetaForPreview(p)}`,'',sourcePreviewExtra(p))}
function decisionItem(d){const p=targetPreview(d.target,d.subject);return item(d.title,d.detail,`${esc(d.id)} &middot; ${esc(d.status)} &middot; source=${esc(d.source)} &middot; basis=${esc(d.basis??'-')}${sourceMetaForPreview(p)}`,'',sourcePreviewExtra(p))}
function workItem(w){const p=targetPreview(w.target,w.subject);return item(w.title,w.detail,`${esc(w.id)} &middot; ${esc(w.kind)} &middot; ${esc(w.status)} &middot; evidence=${esc(w.evidence??'-')}${sourceMetaForPreview(p)}`,'',sourcePreviewExtra(p))}
function workflowDetail(id){const w=workflowById(id);if(!w)return `<div class="empty">Select a workflow to inspect its evidence.</div>`;const summary=workflowSummary(id);const preview=workflowPreview(id);return `<div class="item"><h3>${esc(w.trigger)}</h3><div class="meta">${esc(w.id)} &middot; ${esc(w.framework)} &middot; handler=${esc(w.handler??'-')} &middot; confidence=${esc(w.confidence)}</div><div class="detail">${esc(summary?.detail||'Workflow detected from scanner evidence.')}</div></div><h3>Linked expectations</h3>${miniList(workflowExpectations(id),e=>item(e.title,e.detail,`${esc(e.id)} &middot; ${esc(e.status)} &middot; source=${esc(e.source)}`),'No linked expectations.')}<h3>Linked verifications</h3>${miniList(workflowVerifications(id),verificationItem,'No linked verifications.')}<h3>Linked decisions</h3>${miniList(workflowDecisions(id),decisionItem,'No linked decisions.')}<h3>Linked work</h3>${miniList(workflowWork(id),workItem,'No linked work.')}<h3>Source evidence</h3>${preview?codePreview(preview):'<div class="empty">No source preview embedded for this workflow.</div>'}`}
function workflowsSection(){const first=workflows()[0]?.id;selectedWorkflowId=selectedWorkflowId||first;return `<div class="workflow-layout"><div>${list(workflows(),workflowCard,'No workflows detected.')}</div><aside class="detail-pane" id="workflowDetail">${workflowDetail(selectedWorkflowId)}</aside></div>`}
function expectations(){return packet.artifact.expectations||[]}
function expectationById(id){return expectations().find(e=>e.id===id)}
function expectationWorkflow(e){return e&&e.target==='workflow'?workflowById(e.subject):null}
function expectationVerifications(id){return (packet.artifact.verifications||[]).filter(v=>v.expectation_id===id)}
function expectationWork(id){return (packet.artifact.works||[]).filter(w=>w.expectation_id===id)}
function expectationDecisions(e){if(!e)return[];return (packet.artifact.decisions||[]).filter(d=>d.target===e.target&&(d.subject??null)===(e.subject??null))}
function expectationSupport(id){return (packet.expectation_support||[]).find(s=>s.expectation_id===id)}
function supportMeta(s){return s?`${esc(s.support_status)} &middot; target=${s.target_observed?'observed':'missing'} &middot; verifications=${s.verification.passed}/${s.verification.failed}/${s.verification.inconclusive} &middot; work=${s.work} &middot; decisions=${s.decisions}`:'support=unknown'}
function supportReasons(s){return s?miniList(s.reasons||[],r=>item(r,'','support reason'),'No support reasons recorded.'): '<div class="empty">No support summary embedded.</div>'}
function verificationTotal(s){return s?s.verification.passed+s.verification.failed+s.verification.inconclusive:0}
function expectationNextAction(e,s){if(!s)return 'Rebuild the review packet so Susumu can summarize this expectation.';if(s.verification.failed>0)return 'Review the failed verification before relying on this expectation.';if(!s.target_observed)return 'Find or reconnect the target this expectation is about.';if(s.verification.passed>0)return 'Verified: ready for review or business confidence.';if(s.work===0)return `Connect work with susumu git or susumu git link <commit> ${e.id}.`;if(verificationTotal(s)===0)return `Record verification with susumu verify ${e.id} --passed --method "<check>".`;if(s.verification.inconclusive>0&&s.verification.passed===0)return 'Resolve the inconclusive verification evidence.';return 'Review the support evidence and decide whether more verification is needed.'}
function ladderStep(label,value,tone,detail=''){return `<div class="ladder-step ${tone}"><span class="ladder-label">${esc(label)}</span><strong>${esc(value)}</strong>${detail?`<small>${esc(detail)}</small>`:''}</div>`}
function expectationLadder(e,s){if(!s)return '<div class="empty">No evidence ladder embedded for this expectation.</div>';const total=verificationTotal(s);const verificationDetail=`passed=${s.verification.passed}, failed=${s.verification.failed}, inconclusive=${s.verification.inconclusive}`;return `<div class="ladder" data-evidence-ladder="${esc(e.id)}">${ladderStep('Target observation',s.target_observed?'Target observed':'Target missing',s.target_observed?'good':'bad',`${s.target}${s.subject?':'+s.subject:''}`)}${ladderStep('Work support',s.work>0?`${s.work} linked work record(s)`:'No linked work yet',s.work>0?'good':'warn','Work says what changed for this expectation.')}${ladderStep('Verification evidence',total>0?`${total} verification record(s)`:'No verification yet',s.verification.failed>0?'bad':s.verification.passed>0?'good':'warn',verificationDetail)}${ladderStep('Decision context',s.decisions>0?`${s.decisions} decision record(s)`:'No decision context yet',s.decisions>0?'good':'warn','Decisions record judgment, exceptions, and business context.')}${ladderStep('Review status',s.support_status,s.verification.failed>0||!s.target_observed?'bad':s.verification.passed>0?'good':'warn',(s.reasons||[]).join('; '))}</div><article class="item next-action"><h3>Suggested next action</h3><div class="detail">${esc(expectationNextAction(e,s))}</div></article>`}
function expectationCard(e){const s=expectationSupport(e.id);return `<article class="item clickable ${e.id===selectedExpectationId?'selected':''}" data-expectation-id="${esc(e.id)}"><h3>${esc(e.title)}</h3><div class="meta">${esc(e.id)} &middot; ${esc(e.status)} &middot; ${esc(e.target)}${e.subject?`:${esc(e.subject)}`:''} &middot; ${supportMeta(s)}</div><div class="detail">${esc(e.detail)}</div></article>`}
function expectationDetail(id){const e=expectationById(id);if(!e)return `<div class="empty">Select an expectation to inspect its traceability.</div>`;const workflow=expectationWorkflow(e);const preview=workflow?workflowPreview(workflow.id):targetPreview(e.target,e.subject);const s=expectationSupport(id);return `<div class="item"><h3>${esc(e.title)}</h3><div class="meta">${esc(e.id)} &middot; ${esc(e.status)} &middot; source=${esc(e.source)} &middot; target=${esc(e.target)}${e.subject?`:${esc(e.subject)}`:''}</div><div class="detail">${esc(e.detail)}</div></div><h3>Evidence ladder</h3>${expectationLadder(e,s)}<h3>Support summary</h3><div class="item"><h3>${esc(s?.support_status||'unknown')}</h3><div class="meta">${supportMeta(s)}</div></div><h3>Support reasons</h3>${supportReasons(s)}<h3>Workflow context</h3>${workflow?miniList([workflow],w=>item(w.trigger,`${esc(w.framework)} &middot; handler=${esc(w.handler??'-')} &middot; confidence=${esc(w.confidence)}`,w.id),'No workflow context.'): '<div class="empty">This expectation is not attached to a workflow.</div>'}<h3>Verifications</h3>${miniList(expectationVerifications(id),verificationItem,'No verification records.')}<h3>Work records</h3>${miniList(expectationWork(id),workItem,'No work records.')}<h3>Decisions on same target</h3>${miniList(expectationDecisions(e),decisionItem,'No decisions on this target.')}<h3>Source evidence</h3>${preview?codePreview(preview):'<div class="empty">No source preview embedded for this expectation.</div>'}`}
function readinessBucket(s){if(!s)return 'Unknown';if(s.verification.failed>0)return 'Failed verification';if(!s.target_observed)return 'Missing target';if(s.verification.passed>0)return 'Verified';if(s.work>0)return 'Has work, needs verification';return 'No linked work yet'}
function readinessTone(bucket){return bucket==='Verified'?'good':bucket==='Failed verification'||bucket==='Missing target'?'bad':'warn'}
function readinessItems(){const stored=packet.expectation_readiness||[];if(stored.length)return stored.map(r=>({id:r.expectation_id,title:r.title,label:r.label,next_action:r.next_action}));return expectations().map(e=>{const s=expectationSupport(e.id);return {id:e.id,title:e.title,label:readinessBucket(s),next_action:expectationNextAction(e,s)}})}
function readinessRow(r){const s=expectationSupport(r.id);return item(r.title,r.next_action,`${esc(r.id)} &middot; ${esc(r.label)} &middot; ${supportMeta(s)}`,`<span class="tag ${readinessTone(r.label)==='good'?'passed':readinessTone(r.label)==='bad'?'critical':'warning'}">${esc(r.label)}</span>`)}
function readinessSection(){const order=['Failed verification','Missing target','Has work, needs verification','No linked work yet','Verified','Unknown'];const rows=readinessItems();const metrics=order.map(label=>`<div class="metric"><b>${rows.filter(r=>r.label===label).length}</b><span>${esc(label)}</span></div>`).join('');return `<div class="grid">${metrics}</div><div class="list" style="margin-top:16px">${order.map(label=>{const items=rows.filter(r=>r.label===label).map(readinessRow).join('');return items?`<div><h3>${esc(label)}</h3><div class="mini">${items}</div></div>`:''}).join('')||'<div class="empty">No expectations authored yet.</div>'}</div>`}
function traceabilitySection(){const first=expectations()[0]?.id;selectedExpectationId=selectedExpectationId||first;return `<div class="workflow-layout traceability-layout"><div class="traceability-list">${list(expectations(),expectationCard,'No expectations authored yet.')}</div><aside class="detail-pane traceability-detail" id="expectationDetail">${expectationDetail(selectedExpectationId)}</aside></div>`}
function dirtyFinding(f){return ['SUS023','SUS033'].includes(f.rule_id)}
function staleFinding(f){return ['SUS011','SUS021','SUS031','SUS041','SUS043'].includes(f.rule_id)}
function findingPreview(f){return previewForLocation(f.file_id,f.location)}
function findingCard(f){const p=findingPreview(f);return item(`${f.rule_id}: ${f.title}`,f.detail,`source=${esc(f.source)} &middot; subject=${esc(f.subject??'-')}${sourceMetaForPreview(p)}`,`<span class="tag ${severity(f.severity)}">${esc(f.severity)}</span>` ,sourcePreviewExtra(p))}
function dirtySection(){const findings=packet.artifact.findings||[];const dirty=findings.filter(dirtyFinding);const stale=findings.filter(staleFinding);return `<div class="cols"><div><h3>Dirty evidence</h3>${list(dirty,findingCard,'No changed verification or decision evidence detected.')}</div><div><h3>Stale or missing record targets</h3>${list(stale,findingCard,'No stale record targets detected.')}</div></div>`}
function render(){
 $('projectName').textContent = packet.project.name;
 $('projectSub').textContent = packet.project.root;
 $('result').textContent = packet.result.status;
 $('result').classList.add(packet.result.failed ? 'failed' : 'passed');
 $('resultReason').textContent = packet.result.reason;
 $('critical').textContent = packet.review.critical;
 $('warning').textContent = packet.review.warning;
 $('attention').textContent = packet.review.attention;
 $('workflows').textContent = packet.evidence.workflows;
 $('pills').innerHTML = [
  ['schema',packet.schema_version],['created',packet.created_unix_seconds],['source',packet.source.input],
  ['files',packet.evidence.files],['flows',packet.evidence.flows],['findings',packet.evidence.findings]
 ].map(([k,v])=>`<span class="pill">${esc(k)} <strong>${esc(v)}</strong></span>`).join('');
 $('tabs').innerHTML = tabs.map(([id,label],i)=>`<button class="${i===0?'active':''}" data-tab="${id}">${label}</button>`).join('');
 $('sections').innerHTML = [
  section('overview','Overview', `<div class="grid">
    <div class="metric"><b>${packet.records.expectations}</b><span>expectations</span></div>
    <div class="metric"><b>${packet.records.verifications}</b><span>verifications</span></div>
    <div class="metric"><b>${packet.records.decisions}</b><span>decisions</span></div>
    <div class="metric"><b>${packet.records.work}</b><span>work records</span></div>
  </div><div class="cols" style="margin-top:16px"><div>${list(packet.caveats,a=>item('Caveat',a))}</div><div>${list(packet.next_actions,a=>item('Suggested action',a))}</div></div>`),
  section('readiness','Expectation readiness board', readinessSection()),
  section('review','Needs review', list(packet.review_items, r => item(r.title, r.detail, `source=${esc(r.source)}`, `<span class="tag ${severity(r.severity)}">${esc(r.severity)}</span>`), 'No review items derived.')),
  section('workflows','Workflow evidence', workflowsSection()),
  section('traceability','Expectation traceability', traceabilitySection()),
  section('source','Source previews', list(packet.source_previews, codePreview, 'No source snippets were embedded. Create the review packet from a local project or artifact with readable source files.')),
  section('records','Records requiring follow-up', `<div class="cols"><div><h3>Expectations without verification</h3>${list(packet.expectations_without_verification, r => item(r.title, r.reason, `${esc(r.id)} &middot; ${esc(r.target)} &middot; source=${esc(r.source)}`), 'All expectations have verification records.')}</div><div><h3>Work needing verification</h3>${list(packet.work_needing_verification, r => item(r.title, r.reason, `${esc(r.id)} &middot; ${esc(r.target)} &middot; source=${esc(r.source)}`), 'No work records need verification.')}</div></div>`),
  section('dirty','Dirty and stale evidence', dirtySection()),
  section('artifact','Embedded artifact', `<div class="cols"><div><h3>Files</h3>${list(packet.artifact.files, f => item(f.path, `${f.language} &middot; ${f.lines} lines &middot; ${f.bytes} bytes`, f.id), 'No files.')}</div><div><h3>Workflows</h3>${list(packet.artifact.workflows, w => item(w.trigger, `${w.framework} &middot; handler=${w.handler ?? '-'} &middot; confidence=${w.confidence}`, w.id), 'No workflows.')}</div></div>`),
  section('actions','Next actions', list(packet.next_actions, a=>item('Action',a), 'No next actions.'))
 ].join('');
 document.querySelector('#section-overview').classList.add('active');
 document.querySelectorAll('[data-tab]').forEach(btn=>btn.addEventListener('click',()=>activate(btn.dataset.tab)));
 document.querySelectorAll('[data-workflow-id]').forEach(card=>card.addEventListener('click',()=>selectWorkflow(card.dataset.workflowId)));
 document.querySelectorAll('[data-expectation-id]').forEach(card=>card.addEventListener('click',()=>selectExpectation(card.dataset.expectationId)));
 $('search').addEventListener('input', filter);
}
function selectWorkflow(id){selectedWorkflowId=id;document.querySelectorAll('[data-workflow-id]').forEach(card=>card.classList.toggle('selected',card.dataset.workflowId===id));$('workflowDetail').innerHTML=workflowDetail(id);}
function selectExpectation(id){selectedExpectationId=id;document.querySelectorAll('[data-expectation-id]').forEach(card=>card.classList.toggle('selected',card.dataset.expectationId===id));$('expectationDetail').innerHTML=expectationDetail(id);}
function activate(id){document.querySelectorAll('[data-tab]').forEach(b=>b.classList.toggle('active',b.dataset.tab===id));document.querySelectorAll('.section').forEach(s=>s.classList.toggle('active',s.id===`section-${id}`));$('search').value='';filter();}
function filter(){const q=$('search').value.toLowerCase();document.querySelectorAll('.section.active .item').forEach(el=>el.style.display=el.textContent.toLowerCase().includes(q)?'':'none');}
render();
</script>
</body>
</html>"#
}

fn review_diff_report(old: &ReviewPacketStored, new: &ReviewPacketStored) -> ReviewDiffReport {
    ReviewDiffReport {
        artifact: diff_report(&old.artifact, &new.artifact),
        review_items: diff_by(
            &old.review_items,
            &new.review_items,
            review_item_key,
            review_item_label,
        ),
        next_actions: diff_strings(&old.next_actions, &new.next_actions),
        top_workflows: diff_by(
            &old.top_workflows,
            &new.top_workflows,
            |workflow| workflow.id.clone(),
            |workflow| {
                format!(
                    "{} - {} ({})",
                    workflow.id, workflow.trigger, workflow.framework
                )
            },
        ),
    }
}

fn diff_strings(old: &[String], new: &[String]) -> ChangeSummary {
    diff_by(old, new, Clone::clone, Clone::clone)
}

fn review_item_key(item: &ReviewItemStored) -> String {
    format!("{}|{}|{}", item.severity, item.title, item.source)
}

fn review_item_label(item: &ReviewItemStored) -> String {
    format!("[{}] {} ({})", item.severity, item.title, item.source)
}

fn review_diff_regressed(old: &ReviewPacketStored, new: &ReviewPacketStored) -> bool {
    (!old.result.failed && new.result.failed) || new.review.critical > old.review.critical
}

fn print_review_packet(packet: &ReviewPacketStored, max_items: usize) {
    println!("Susumu review packet: {}", packet.project.name);
    println!("Schema: {}", packet.schema_version);
    println!("Created: {}", packet.created_unix_seconds);
    println!("Source: {}", packet.source.input);
    println!("Root: {}", packet.project.root);
    println!();
    println!(
        "Evidence: {} files, {} workflows, {} flows, {} findings",
        packet.evidence.files,
        packet.evidence.workflows,
        packet.evidence.flows,
        packet.evidence.findings
    );
    println!(
        "Records: {} expectations, {} verifications, {} decisions, {} work",
        packet.records.expectations,
        packet.records.verifications,
        packet.records.decisions,
        packet.records.work
    );
    println!(
        "Review: {} critical, {} warning, {} attention",
        packet.review.critical, packet.review.warning, packet.review.attention
    );
    println!(
        "Result: {} ({})",
        packet.result.status, packet.result.reason
    );
    println!();
    print_handoff_workflows(&packet.top_workflows, max_items);
    print_stored_review_items(&packet.review_items, max_items);
    print_expectation_readiness(&packet.expectation_readiness, max_items);
    print_expectation_support(&packet.expectation_support, max_items);
    print_handoff_records(
        "Expectations without verification",
        &packet.expectations_without_verification,
        max_items,
    );
    print_handoff_records(
        "Work needing verification",
        &packet.work_needing_verification,
        max_items,
    );
    print_string_section("Caveats", &packet.caveats, max_items);
    print_string_section("Suggested next actions", &packet.next_actions, max_items);
}

fn print_expectation_readiness(items: &[ExpectationReadiness], max_items: usize) {
    println!();
    println!("Expectation readiness");
    if items.is_empty() {
        println!("  none");
        return;
    }
    for item in items.iter().take(max_items) {
        println!(
            "  - {} [{}] {}",
            item.title, item.label, item.expectation_id
        );
        println!("    next: {}", item.next_action);
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
}

fn print_expectation_support(items: &[ExpectationSupport], max_items: usize) {
    println!();
    println!("Expectation support");
    if items.is_empty() {
        println!("  none");
        return;
    }
    for item in items.iter().take(max_items) {
        println!(
            "  - {} [{}] target={}{}",
            item.title,
            item.support_status,
            item.target,
            item.subject
                .as_ref()
                .map_or_else(String::new, |subject| format!(":{subject}"))
        );
        println!(
            "    observed={} verifications={}/{}/{} work={} decisions={} findings={}",
            item.target_observed,
            item.verification.passed,
            item.verification.failed,
            item.verification.inconclusive,
            item.work,
            item.decisions,
            item.findings
        );
        for reason in item.reasons.iter().take(3) {
            println!("    reason: {reason}");
        }
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
}

fn print_stored_review_items(items: &[ReviewItemStored], max_items: usize) {
    println!("Needs review:");
    if items.is_empty() {
        println!("  none derived");
    }
    for item in items.iter().take(max_items) {
        println!("  [{}] {}", item.severity, item.title);
        println!("      source={}", item.source);
        println!("      {}", item.detail);
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
    println!();
}

fn print_review_diff(
    old: &ReviewPacketStored,
    new: &ReviewPacketStored,
    report: &ReviewDiffReport,
    max_items: usize,
) {
    println!(
        "Susumu review diff: {} -> {}",
        old.project.name, new.project.name
    );
    println!("Old created: {}", old.created_unix_seconds);
    println!("New created: {}", new.created_unix_seconds);
    println!("Result: {} -> {}", old.result.status, new.result.status);
    println!(
        "Review: critical {}->{}, warning {}->{}, attention {}->{}",
        old.review.critical,
        new.review.critical,
        old.review.warning,
        new.review.warning,
        old.review.attention,
        new.review.attention
    );
    if review_diff_regressed(old, new) {
        println!("Regression: yes");
    } else {
        println!("Regression: no");
    }
    println!();
    print_change_section("Review items", &report.review_items, max_items);
    print_change_section("Next actions", &report.next_actions, max_items);
    print_change_section("Top workflows", &report.top_workflows, max_items);
    println!("Embedded artifact changes:");
    print_change_section("Files", &report.artifact.files, max_items);
    print_change_section("Workflows", &report.artifact.workflows, max_items);
    print_change_section("Expectations", &report.artifact.expectations, max_items);
    print_change_section("Verifications", &report.artifact.verifications, max_items);
    print_change_section("Decisions", &report.artifact.decisions, max_items);
    print_change_section("Work", &report.artifact.works, max_items);
    print_freshness_section(&report.artifact.stale_items, max_items);
}

fn print_review_diff_json(
    args: &ReviewDiffArgs,
    old: &ReviewPacketStored,
    new: &ReviewPacketStored,
    report: &ReviewDiffReport,
) -> Result<()> {
    let regressed = review_diff_regressed(old, new);
    let output = serde_json::json!({
        "old": review_packet_summary_json(old),
        "new": review_packet_summary_json(new),
        "changes": {
            "review_items": change_summary_json(&report.review_items),
            "next_actions": change_summary_json(&report.next_actions),
            "top_workflows": change_summary_json(&report.top_workflows),
            "artifact": {
                "files": change_summary_json(&report.artifact.files),
                "workflows": change_summary_json(&report.artifact.workflows),
                "expectations": change_summary_json(&report.artifact.expectations),
                "verifications": change_summary_json(&report.artifact.verifications),
                "decisions": change_summary_json(&report.artifact.decisions),
                "work": change_summary_json(&report.artifact.works),
            },
        },
        "freshness": {
            "stale": report.artifact.stale_items.len(),
            "items": check_item_jsons(&report.artifact.stale_items),
        },
        "result": {
            "status": if args.fail_on_regression && regressed { "failed" } else { "passed" },
            "failed": args.fail_on_regression && regressed,
            "fail_on_regression": args.fail_on_regression,
            "regressed": regressed,
            "reason": review_diff_result_reason(args.fail_on_regression, regressed),
        },
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize review diff")?
    );
    Ok(())
}

fn review_packet_summary_json(packet: &ReviewPacketStored) -> serde_json::Value {
    serde_json::json!({
        "schema_version": &packet.schema_version,
        "created_unix_seconds": packet.created_unix_seconds,
        "source": &packet.source,
        "project": &packet.project,
        "evidence": &packet.evidence,
        "records": &packet.records,
        "review": &packet.review,
        "result": &packet.result,
    })
}

const fn review_diff_result_reason(fail_on_regression: bool, regressed: bool) -> &'static str {
    if fail_on_regression && regressed {
        "review regression present"
    } else if regressed {
        "passed with review regression"
    } else {
        "passed"
    }
}

fn git_rewind(args: &GitRewindArgs) -> Result<()> {
    let snapshot_dir = git_snapshot_dir(&args.repo, &args.from)?;
    let mut failed = false;
    let result = (|| -> Result<()> {
        let mut old = scan_project(&snapshot_dir)
            .with_context(|| format!("could not scan Git ref {}", args.from))?;
        old.project_name = format!("{}@{}", git_repo_label(&args.repo), args.from);
        let new = if let Some(artifact) = &args.artifact {
            read_analysis_artifact(artifact)?
        } else {
            load_analysis(&args.repo, None, None, None, None, false)?
        };
        if let Some(output) = &args.old_output {
            fs::write(output, write_susu(&old, args.minify)?)
                .with_context(|| format!("could not write {}", output.display()))?;
            eprintln!("wrote old-ref artifact {}", output.display());
        }

        let report = diff_report(&old, &new);
        if args.json {
            print_git_rewind_json(args, &old, &new, &report)?;
        } else {
            println!(
                "Susumu git rewind: {}@{} -> {}",
                args.repo.display(),
                args.from,
                args.artifact.as_ref().map_or_else(
                    || args.repo.display().to_string(),
                    |path| path.display().to_string()
                )
            );
            println!();
            print_diff_report(&old, &new, &report, args.max_items);
        }
        failed = args.fail_on_stale && !report.stale_items.is_empty();
        Ok(())
    })();

    if let Err(error) = fs::remove_dir_all(&snapshot_dir) {
        eprintln!(
            "warning: could not remove temporary Git snapshot {}: {error}",
            snapshot_dir.display()
        );
    }

    result?;
    if failed {
        process::exit(1);
    }
    Ok(())
}

fn read_analysis_artifact(path: &PathBuf) -> Result<ProjectAnalysis> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("susu"))
    {
        bail!("{} is not a .susu artifact", path.display());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut analysis =
        parse_susu(&source).with_context(|| format!("could not parse {}", path.display()))?;
    refresh_derived_analysis(&mut analysis);
    Ok(analysis)
}

#[derive(Debug, Default)]
struct ChangeSummary {
    added: Vec<String>,
    removed: Vec<String>,
    changed: Vec<String>,
}

impl ChangeSummary {
    fn total(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

#[derive(Debug)]
struct DiffReport {
    files: ChangeSummary,
    workflows: ChangeSummary,
    expectations: ChangeSummary,
    verifications: ChangeSummary,
    decisions: ChangeSummary,
    works: ChangeSummary,
    stale_items: Vec<CheckItem>,
}

#[derive(Debug, Serialize)]
struct DiffJson<'a> {
    old: DiffProjectJson<'a>,
    new: DiffProjectJson<'a>,
    changes: DiffChangesJson<'a>,
    freshness: DiffFreshnessJson<'a>,
    result: DiffResultJson<'a>,
}

#[derive(Debug, Serialize)]
struct DiffProjectJson<'a> {
    name: &'a str,
    root: &'a str,
    generated_unix_seconds: u64,
}

#[derive(Debug, Serialize)]
struct DiffChangesJson<'a> {
    files: ChangeSummaryJson<'a>,
    workflows: ChangeSummaryJson<'a>,
    expectations: ChangeSummaryJson<'a>,
    verifications: ChangeSummaryJson<'a>,
    decisions: ChangeSummaryJson<'a>,
    work: ChangeSummaryJson<'a>,
}

#[derive(Debug, Serialize)]
struct ChangeSummaryJson<'a> {
    added: &'a [String],
    removed: &'a [String],
    changed: &'a [String],
}

#[derive(Debug, Serialize)]
struct DiffFreshnessJson<'a> {
    stale: usize,
    items: Vec<CheckItemJson<'a>>,
}

#[derive(Debug, Serialize)]
struct DiffResultJson<'a> {
    status: &'a str,
    failed: bool,
    fail_on_stale: bool,
    reason: &'a str,
}

#[derive(Debug, Serialize)]
struct GitRewindJson<'a> {
    git: GitRewindGitJson,
    old: DiffProjectJson<'a>,
    new: DiffProjectJson<'a>,
    changes: DiffChangesJson<'a>,
    freshness: DiffFreshnessJson<'a>,
    result: DiffResultJson<'a>,
}

#[derive(Debug, Serialize)]
struct GitRewindGitJson {
    repo: String,
    from: String,
    artifact: Option<String>,
    old_output: Option<String>,
}

fn diff_report(old: &ProjectAnalysis, new: &ProjectAnalysis) -> DiffReport {
    DiffReport {
        files: diff_by(
            &old.files,
            &new.files,
            |file| file.path.clone(),
            |file| file.path.clone(),
        ),
        workflows: diff_by(
            &old.workflows,
            &new.workflows,
            |workflow| format!("{}|{}", workflow.framework, workflow.trigger),
            |workflow| format!("{} ({})", workflow.trigger, workflow.framework),
        ),
        expectations: diff_by(
            &old.expectations,
            &new.expectations,
            |expectation| expectation.id.clone(),
            |expectation| format!("{} - {}", expectation.id, expectation.title),
        ),
        verifications: diff_by(
            &old.verifications,
            &new.verifications,
            |verification| verification.id.clone(),
            |verification| {
                format!(
                    "{} - {}",
                    verification.id,
                    expectation_title(new, &verification.expectation_id)
                )
            },
        ),
        decisions: diff_by(
            &old.decisions,
            &new.decisions,
            |decision| decision.id.clone(),
            |decision| format!("{} - {}", decision.id, decision.title),
        ),
        works: diff_by(
            &old.works,
            &new.works,
            |work| work.id.clone(),
            |work| format!("{} - {}", work.id, work.title),
        ),
        stale_items: freshness_check_items(new),
    }
}

fn diff_by<T>(
    old: &[T],
    new: &[T],
    key: impl Fn(&T) -> String,
    label: impl Fn(&T) -> String,
) -> ChangeSummary
where
    T: PartialEq,
{
    let old_map = old
        .iter()
        .map(|item| (key(item), item))
        .collect::<BTreeMap<_, _>>();
    let new_map = new
        .iter()
        .map(|item| (key(item), item))
        .collect::<BTreeMap<_, _>>();
    let mut summary = ChangeSummary::default();

    for (item_key, new_item) in &new_map {
        match old_map.get(item_key) {
            Some(old_item) if *old_item != *new_item => summary.changed.push(label(new_item)),
            Some(_) => {}
            None => summary.added.push(label(new_item)),
        }
    }
    for (item_key, old_item) in &old_map {
        if !new_map.contains_key(item_key) {
            summary.removed.push(label(old_item));
        }
    }

    summary
}

fn freshness_check_items(analysis: &ProjectAnalysis) -> Vec<CheckItem> {
    analysis
        .findings
        .iter()
        .filter(|finding| matches!(finding.rule_id.as_str(), "SUS023" | "SUS033"))
        .map(|finding| CheckItem {
            severity: CheckSeverity::Warning,
            title: format!("{}: {}", finding.rule_id, finding.title),
            detail: finding.detail.clone(),
            source: finding.source.clone(),
        })
        .collect()
}

fn print_diff_report(
    old: &ProjectAnalysis,
    new: &ProjectAnalysis,
    report: &DiffReport,
    max_items: usize,
) {
    println!("Susumu diff: {} -> {}", old.project_name, new.project_name);
    println!("Old generated: {}", old.generated_unix_seconds);
    println!("New generated: {}", new.generated_unix_seconds);
    println!();
    print_change_section("Files", &report.files, max_items);
    print_change_section("Workflows", &report.workflows, max_items);
    print_change_section("Expectations", &report.expectations, max_items);
    print_change_section("Verifications", &report.verifications, max_items);
    print_change_section("Decisions", &report.decisions, max_items);
    print_change_section("Work", &report.works, max_items);
    print_freshness_section(&report.stale_items, max_items);
}

fn print_diff_json(
    old: &ProjectAnalysis,
    new: &ProjectAnalysis,
    report: &DiffReport,
    fail_on_stale: bool,
) -> Result<()> {
    let failed = fail_on_stale && !report.stale_items.is_empty();
    let output = DiffJson {
        old: DiffProjectJson {
            name: &old.project_name,
            root: &old.root,
            generated_unix_seconds: old.generated_unix_seconds,
        },
        new: DiffProjectJson {
            name: &new.project_name,
            root: &new.root,
            generated_unix_seconds: new.generated_unix_seconds,
        },
        changes: DiffChangesJson {
            files: change_summary_json(&report.files),
            workflows: change_summary_json(&report.workflows),
            expectations: change_summary_json(&report.expectations),
            verifications: change_summary_json(&report.verifications),
            decisions: change_summary_json(&report.decisions),
            work: change_summary_json(&report.works),
        },
        freshness: DiffFreshnessJson {
            stale: report.stale_items.len(),
            items: check_item_jsons(&report.stale_items),
        },
        result: DiffResultJson {
            status: if failed { "failed" } else { "passed" },
            failed,
            fail_on_stale,
            reason: diff_result_reason(failed, report.stale_items.len()),
        },
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize diff report")?
    );
    Ok(())
}

fn print_git_rewind_json(
    args: &GitRewindArgs,
    old: &ProjectAnalysis,
    new: &ProjectAnalysis,
    report: &DiffReport,
) -> Result<()> {
    let failed = args.fail_on_stale && !report.stale_items.is_empty();
    let output = GitRewindJson {
        git: GitRewindGitJson {
            repo: args.repo.display().to_string(),
            from: args.from.clone(),
            artifact: args
                .artifact
                .as_ref()
                .map(|path| path.display().to_string()),
            old_output: args
                .old_output
                .as_ref()
                .map(|path| path.display().to_string()),
        },
        old: DiffProjectJson {
            name: &old.project_name,
            root: &old.root,
            generated_unix_seconds: old.generated_unix_seconds,
        },
        new: DiffProjectJson {
            name: &new.project_name,
            root: &new.root,
            generated_unix_seconds: new.generated_unix_seconds,
        },
        changes: DiffChangesJson {
            files: change_summary_json(&report.files),
            workflows: change_summary_json(&report.workflows),
            expectations: change_summary_json(&report.expectations),
            verifications: change_summary_json(&report.verifications),
            decisions: change_summary_json(&report.decisions),
            work: change_summary_json(&report.works),
        },
        freshness: DiffFreshnessJson {
            stale: report.stale_items.len(),
            items: check_item_jsons(&report.stale_items),
        },
        result: DiffResultJson {
            status: if failed { "failed" } else { "passed" },
            failed,
            fail_on_stale: args.fail_on_stale,
            reason: diff_result_reason(failed, report.stale_items.len()),
        },
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize rewind report")?
    );
    Ok(())
}

fn change_summary_json(summary: &ChangeSummary) -> ChangeSummaryJson<'_> {
    ChangeSummaryJson {
        added: &summary.added,
        removed: &summary.removed,
        changed: &summary.changed,
    }
}

const fn diff_result_reason(failed: bool, stale: usize) -> &'static str {
    if failed {
        "stale evidence present"
    } else if stale > 0 {
        "passed with stale evidence"
    } else {
        "passed"
    }
}

fn print_change_section(title: &str, summary: &ChangeSummary, max_items: usize) {
    println!("{title}:");
    println!("  added: {}", summary.added.len());
    println!("  removed: {}", summary.removed.len());
    println!("  changed: {}", summary.changed.len());
    if summary.total() > 0 {
        print_labeled_items("+", &summary.added, max_items);
        print_labeled_items("-", &summary.removed, max_items);
        print_labeled_items("~", &summary.changed, max_items);
    }
    println!();
}

fn print_labeled_items(prefix: &str, items: &[String], max_items: usize) {
    for item in items.iter().take(max_items) {
        println!("  {prefix} {item}");
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
}

fn print_freshness_section(items: &[CheckItem], max_items: usize) {
    println!("Freshness:");
    println!("  stale: {}", items.len());
    for item in items.iter().take(max_items) {
        println!("  [warning] {}", item.title);
        println!("      source={}", item.source);
        println!("      {}", item.detail);
    }
    if items.len() > max_items {
        println!("  ... {} more", items.len() - max_items);
    }
    println!();
}

#[derive(Debug)]
struct ReviewDiffReport {
    artifact: DiffReport,
    review_items: ChangeSummary,
    next_actions: ChangeSummary,
    top_workflows: ChangeSummary,
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn expectation_title(analysis: &ProjectAnalysis, id: &str) -> String {
    analysis
        .expectations
        .iter()
        .find(|expectation| expectation.id == id)
        .map_or_else(|| id.to_owned(), |expectation| expectation.title.clone())
}

fn add_expectation(args: AddExpectation) -> Result<()> {
    let target = ExpectationTarget::from(args.target);
    let status = ExpectationStatus::from(args.status);
    let subject = expectation_subject(
        target,
        resolve_file_subject(&args.target_root, target, args.subject)?,
    )?;
    let id = args.id.unwrap_or_else(|| {
        expectation_id(
            target,
            subject.as_deref(),
            status,
            &args.source,
            &args.title,
            &args.detail,
        )
    });
    let expectation = Expectation {
        id,
        target,
        subject,
        status,
        source: args.source,
        title: args.title,
        detail: args.detail,
    };

    let mut expectations = if args.file.exists() {
        read_expectation_sidecar(&args.file)?
    } else {
        Vec::new()
    };

    let id = expectation.id.clone();
    merge_expectations(&mut expectations, vec![expectation]);
    fs::write(&args.file, write_expectations(&expectations, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("wrote expectation {id} to {}", args.file.display());
    Ok(())
}

fn resolve_target(args: &ResolveArgs) -> Result<()> {
    let analysis = scan_project(&args.target)
        .with_context(|| format!("could not scan {}", args.target.display()))?;
    let requested = normalize_git_path(&args.path.to_string_lossy());
    let matches = analysis
        .files
        .iter()
        .filter(|file| file.path == requested)
        .collect::<Vec<_>>();
    if matches.is_empty() {
        bail!(
            "could not resolve file path `{}` under {}; run `susumu resolve --help` for usage",
            args.path.display(),
            args.target.display()
        );
    }
    if args.json {
        println!(
            "{}",
            serde_json::json!({"path": matches[0].path, "id": matches[0].id})
        );
    } else {
        println!("{}\t{}", matches[0].id, matches[0].path);
    }
    Ok(())
}

fn resolve_file_subject(
    target_root: &Path,
    target: ExpectationTarget,
    subject: Option<String>,
) -> Result<Option<String>> {
    if target != ExpectationTarget::File {
        return Ok(subject);
    }
    let Some(subject) = subject else {
        return Ok(None);
    };
    if subject.starts_with("f_") {
        return Ok(Some(subject));
    }
    let analysis = scan_project(target_root).with_context(|| {
        format!(
            "could not scan {} while resolving file subject",
            target_root.display()
        )
    })?;
    let requested = normalize_git_path(&subject);
    let Some(file) = analysis.files.iter().find(|file| file.path == requested) else {
        bail!(
            "file subject `{subject}` did not resolve under {}; use `susumu resolve {subject}` or pass its f_ id",
            target_root.display()
        );
    };
    eprintln!("resolved file subject {subject} -> {}", file.id);
    Ok(Some(file.id.clone()))
}

fn list_expectations(args: &ListExpectations) -> Result<()> {
    let expectations = read_expectations_file(&args.file)?;
    if expectations.is_empty() {
        println!("No expectations in {}", args.file.display());
        return Ok(());
    }

    for expectation in expectations {
        let subject = expectation.subject.as_deref().unwrap_or("-");
        println!(
            "{}  {:9}  {:10}  {:18}  {}",
            expectation.id, expectation.target, expectation.status, subject, expectation.title
        );
    }
    Ok(())
}

fn remove_expectation(args: &RemoveExpectation) -> Result<()> {
    let mut expectations = read_expectation_sidecar(&args.file)?;
    let original_len = expectations.len();
    expectations.retain(|expectation| expectation.id != args.id);
    if expectations.len() == original_len {
        bail!(
            "{} does not contain expectation {}",
            args.file.display(),
            args.id
        );
    }

    fs::write(&args.file, write_expectations(&expectations, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!(
        "removed expectation {} from {}",
        args.id,
        args.file.display()
    );
    Ok(())
}

fn add_verification(args: AddVerification) -> Result<()> {
    let status = VerificationStatus::from(args.status);
    let evidence = args.evidence.filter(|value| !value.trim().is_empty());
    let id = args.id.unwrap_or_else(|| {
        verification_id(
            &args.expectation,
            status,
            args.supersedes.as_deref(),
            &args.method,
            &args.source,
            evidence.as_deref(),
            &args.detail,
        )
    });
    let verification = Verification {
        id,
        expectation_id: args.expectation,
        status,
        supersedes: args.supersedes.filter(|value| !value.trim().is_empty()),
        method: args.method,
        source: args.source,
        evidence,
        basis: args.basis.filter(|value| !value.trim().is_empty()),
        detail: args.detail,
    };

    let id = verification.id.clone();
    write_verification_record(&args.file, verification, false)?;
    eprintln!("wrote verification {id} to {}", args.file.display());
    Ok(())
}

fn list_verifications(args: &ListVerifications) -> Result<()> {
    let verifications = read_verifications_file(&args.file)?;
    if verifications.is_empty() {
        println!("No verifications in {}", args.file.display());
        return Ok(());
    }

    for verification in verifications {
        println!(
            "{}  {:12}  {:18}  {}",
            verification.id, verification.status, verification.expectation_id, verification.method
        );
    }
    Ok(())
}

fn remove_verification(args: &RemoveVerification) -> Result<()> {
    bail!(
        "verification records are append-only; cannot remove {} from {}. Add a new verification with --supersedes {} and the replacement status",
        args.id,
        args.file.display(),
        args.id
    )
}

fn add_decision(args: AddDecision) -> Result<()> {
    let target = ExpectationTarget::from(args.target);
    let status = DecisionStatus::from(args.status);
    let subject = target_subject("decisions", target, args.subject)?;
    let id = args.id.unwrap_or_else(|| {
        decision_id(
            target,
            subject.as_deref(),
            status,
            &args.source,
            &args.title,
            &args.detail,
        )
    });
    let decision = Decision {
        id,
        target,
        subject,
        status,
        source: args.source,
        basis: args.basis.filter(|value| !value.trim().is_empty()),
        title: args.title,
        detail: args.detail,
    };

    let mut decisions = if args.file.exists() {
        read_decision_sidecar(&args.file)?
    } else {
        Vec::new()
    };

    let id = decision.id.clone();
    merge_decisions(&mut decisions, vec![decision]);
    fs::write(&args.file, write_decisions(&decisions, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("wrote decision {id} to {}", args.file.display());
    Ok(())
}

fn list_decisions(args: &ListDecisions) -> Result<()> {
    let decisions = read_decisions_file(&args.file)?;
    if decisions.is_empty() {
        println!("No decisions in {}", args.file.display());
        return Ok(());
    }

    for decision in decisions {
        let subject = decision.subject.as_deref().unwrap_or("-");
        println!(
            "{}  {:9}  {:10}  {:18}  {}",
            decision.id, decision.target, decision.status, subject, decision.title
        );
    }
    Ok(())
}

fn remove_decision(args: &RemoveDecision) -> Result<()> {
    let mut decisions = read_decision_sidecar(&args.file)?;
    let original_len = decisions.len();
    decisions.retain(|decision| decision.id != args.id);
    if decisions.len() == original_len {
        bail!(
            "{} does not contain decision {}",
            args.file.display(),
            args.id
        );
    }

    fs::write(&args.file, write_decisions(&decisions, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("removed decision {} from {}", args.id, args.file.display());
    Ok(())
}

fn add_work(args: AddWork) -> Result<()> {
    let target = ExpectationTarget::from(args.target);
    let subject = target_subject("work records", target, args.subject)?;
    let expectation = args.expectation.filter(|value| !value.trim().is_empty());
    let kind = WorkKind::from(args.kind);
    let status = WorkStatus::from(args.status);
    let evidence = args.evidence.filter(|value| !value.trim().is_empty());
    let id = args.id.unwrap_or_else(|| {
        work_id(
            target,
            subject.as_deref(),
            expectation.as_deref(),
            kind,
            status,
            &args.source,
            evidence.as_deref(),
            &args.title,
            &args.detail,
        )
    });
    let work = Work {
        id,
        target,
        subject,
        expectation_id: expectation,
        kind,
        status,
        source: args.source,
        evidence,
        title: args.title,
        detail: args.detail,
    };

    let mut works = if args.file.exists() {
        read_work_sidecar(&args.file)?
    } else {
        Vec::new()
    };

    let id = work.id.clone();
    merge_works(&mut works, vec![work]);
    fs::write(&args.file, write_works(&works, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("wrote work {id} to {}", args.file.display());
    Ok(())
}

fn list_works(args: &ListWorks) -> Result<()> {
    let works = read_works_file(&args.file)?;
    if works.is_empty() {
        println!("No work records in {}", args.file.display());
        return Ok(());
    }

    for work in works {
        let subject = work.subject.as_deref().unwrap_or("-");
        let expectation = work.expectation_id.as_deref().unwrap_or("-");
        println!(
            "{}  {:9}  {:14}  {:11}  {:18}  {:18}  {}",
            work.id, work.target, work.kind, work.status, subject, expectation, work.title
        );
    }
    Ok(())
}

fn remove_work(args: &RemoveWork) -> Result<()> {
    let mut works = read_work_sidecar(&args.file)?;
    let original_len = works.len();
    works.retain(|work| work.id != args.id);
    if works.len() == original_len {
        bail!("{} does not contain work {}", args.file.display(), args.id);
    }

    fs::write(&args.file, write_works(&works, false)?)
        .with_context(|| format!("could not write {}", args.file.display()))?;
    eprintln!("removed work {} from {}", args.id, args.file.display());
    Ok(())
}

fn git_connect(args: &GitConnectArgs) -> Result<()> {
    let artifact = read_analysis_artifact(&args.artifact)?;
    run_git_connect(args, &artifact)
}

fn run_git_connect(args: &GitConnectArgs, artifact: &ProjectAnalysis) -> Result<()> {
    let commits = git_commits_for(
        &args.repo,
        args.since.as_deref(),
        args.until.as_deref(),
        args.limit,
    )?;
    let report = build_git_connect_report(artifact, &commits);
    let export = export_git_connect_work(args, artifact, &report)?;
    if args.json {
        print_git_connect_json(args, &report, export.as_ref())?;
    } else {
        print_git_connect_report(args, &report, export.as_ref());
    }
    Ok(())
}

fn git_link(args: &GitLinkArgs) -> Result<()> {
    let artifact = read_analysis_artifact(&args.artifact)?;
    let expectation = artifact
        .expectations
        .iter()
        .find(|expectation| expectation.id == args.expectation)
        .with_context(|| {
            format!(
                "{} does not contain expectation {}",
                args.artifact.display(),
                args.expectation
            )
        })?;
    let commit = git_commit_for_ref(&args.repo, &args.commit)?;
    let work = work_from_git_link(&commit, expectation, args);
    let mut works = if args.output.exists() {
        read_work_sidecar(&args.output)?
    } else {
        Vec::new()
    };
    let id = work.id.clone();
    merge_works(&mut works, vec![work.clone()]);
    write_text_file(&args.output, &write_works(&works, args.minify)?)?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "output": args.output,
                "record": {
                    "id": work.id,
                    "commit": commit.hash,
                    "expectation": expectation.id,
                    "target": work.target.to_string(),
                    "subject": work.subject,
                    "kind": work.kind.to_string(),
                    "status": work.status.to_string(),
                    "evidence": work.evidence,
                    "title": work.title,
                }
            }))
            .context("could not serialize git link report")?
        );
    } else {
        println!("Susumu git link: {}", args.repo.display());
        println!("Commit: {}  {}", short_hash(&commit.hash), commit.subject);
        println!("Expectation: {}  {}", expectation.id, expectation.title);
        println!("Work: {id} -> {}", args.output.display());
    }

    Ok(())
}

fn git_snapshot_dir(repo: &Path, revision: &str) -> Result<PathBuf> {
    let snapshot_dir =
        env::temp_dir().join(format!("susumu-rewind-{}", git_snapshot_id(repo, revision)));
    fs::create_dir_all(&snapshot_dir)
        .with_context(|| format!("could not create {}", snapshot_dir.display()))?;

    let result = populate_git_snapshot(repo, revision, &snapshot_dir);
    if result.is_err() {
        let _ = fs::remove_dir_all(&snapshot_dir);
    }
    result?;
    Ok(snapshot_dir)
}

fn populate_git_snapshot(repo: &Path, revision: &str, snapshot_dir: &Path) -> Result<()> {
    for git_path in git_tree_paths(repo, revision)? {
        let output_path = safe_snapshot_path(snapshot_dir, &git_path)?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let bytes = git_file_bytes(repo, revision, &git_path)?;
        fs::write(&output_path, bytes)
            .with_context(|| format!("could not write {}", output_path.display()))?;
    }
    Ok(())
}

fn git_tree_paths(repo: &Path, revision: &str) -> Result<Vec<String>> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("ls-tree")
        .arg("-r")
        .arg("-z")
        .arg("--name-only")
        .arg(revision)
        .output()
        .with_context(|| format!("could not list files at Git ref {revision}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git ls-tree failed for {revision}: {}", stderr.trim());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect())
}

fn git_file_bytes(repo: &Path, revision: &str, git_path: &str) -> Result<Vec<u8>> {
    let spec = format!("{revision}:{git_path}");
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("show")
        .arg(spec)
        .output()
        .with_context(|| format!("could not read {git_path} at Git ref {revision}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git show failed for {git_path} at {revision}: {}",
            stderr.trim()
        );
    }
    Ok(output.stdout)
}

fn safe_snapshot_path(root: &Path, git_path: &str) -> Result<PathBuf> {
    let normalized = normalize_git_path(git_path);
    if normalized.is_empty() {
        bail!("Git snapshot path cannot be empty");
    }
    if looks_like_windows_absolute_path(&normalized) {
        bail!("refusing unsafe Git snapshot path: {git_path}");
    }

    let mut output = root.to_path_buf();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                bail!("refusing unsafe Git snapshot path: {git_path}")
            }
        }
    }
    Ok(output)
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

fn git_snapshot_id(repo: &Path, revision: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut hash = Sha256::new();
    hash.update(repo.display().to_string().as_bytes());
    hash.update([0]);
    hash.update(revision.as_bytes());
    hash.update([0]);
    hash.update(process::id().to_string().as_bytes());
    hash.update([0]);
    hash.update(timestamp.to_string().as_bytes());
    hex_prefix(&hash.finalize(), 8)
}

fn git_repo_label(repo: &Path) -> String {
    repo.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| repo.display().to_string(), str::to_owned)
}

fn import_git_work(args: &GitImportArgs) -> Result<()> {
    let commits = git_commits(args)?;
    let artifact = args
        .artifact
        .as_ref()
        .map(read_analysis_artifact)
        .transpose()?;
    let context = GitImportContext {
        artifact: artifact.as_ref(),
        target_depth: GitTargetDepth::from(args.target_depth.clone()),
    };
    let imported = commits
        .into_iter()
        .map(|commit| imported_git_work(&commit, &args.source, &context))
        .collect::<Vec<_>>();

    let mut works = if args.output.exists() {
        read_work_sidecar(&args.output)?
    } else {
        Vec::new()
    };
    let count = imported.len();
    merge_works(
        &mut works,
        imported
            .iter()
            .map(|imported| imported.work.clone())
            .collect(),
    );
    fs::write(&args.output, write_works(&works, args.minify)?)
        .with_context(|| format!("could not write {}", args.output.display()))?;
    if args.json {
        print_git_import_json(&args.output, &imported)?;
    } else {
        eprintln!(
            "imported {count} git commits into {}",
            args.output.display()
        );
    }
    Ok(())
}

#[derive(Debug)]
struct GitImportContext<'a> {
    artifact: Option<&'a ProjectAnalysis>,
    target_depth: GitTargetDepth,
}

#[derive(Debug, Clone)]
struct GitCommit {
    hash: String,
    author_name: String,
    author_email: String,
    author_date: String,
    subject: String,
    body: String,
    changed_files: Vec<String>,
}

#[derive(Debug)]
struct ImportedGitWork {
    work: Work,
    commit_hash: String,
    targeting: String,
    changed_files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct GitImportJson<'a> {
    output: String,
    imported: usize,
    records: Vec<GitImportRecordJson<'a>>,
}

#[derive(Debug, Serialize)]
struct GitImportRecordJson<'a> {
    id: &'a str,
    commit: &'a str,
    target: String,
    subject: Option<&'a str>,
    expectation: Option<&'a str>,
    title: &'a str,
    targeting: &'a str,
    changed_files: &'a [String],
}

#[derive(Debug, Serialize)]
struct GitConnectExport {
    path: String,
    written: usize,
    source: String,
}

#[derive(Debug, Serialize)]
struct GitConnectJson<'a> {
    repo: String,
    artifact: String,
    since: Option<&'a str>,
    until: Option<&'a str>,
    commits: usize,
    connected: usize,
    needs_record: usize,
    unconnected: usize,
    export: Option<&'a GitConnectExport>,
    records: &'a [GitConnection],
}

const GIT_LOG_FORMAT: &str = "%H%x1f%an%x1f%ae%x1f%aI%x1f%s%x1f%b%x1e";

fn git_commits(args: &GitImportArgs) -> Result<Vec<GitCommit>> {
    git_commits_for(
        &args.repo,
        args.since.as_deref(),
        args.until.as_deref(),
        args.limit,
    )
}

fn git_commits_for(
    repo: &Path,
    since: Option<&str>,
    until: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<GitCommit>> {
    let mut command = ProcessCommand::new("git");
    command
        .arg("-C")
        .arg(repo)
        .arg("log")
        .arg(format!("--format={GIT_LOG_FORMAT}"));
    if let Some(limit) = limit {
        command.arg("-n").arg(limit.to_string());
    }
    if let Some(revision) = git_revision_range(since, until) {
        command.arg(revision);
    }

    let output = command
        .output()
        .with_context(|| format!("could not run git in {}", repo.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("does not have any commits") {
            return Ok(Vec::new());
        }
        bail!("git log failed in {}: {}", repo.display(), stderr.trim());
    }
    let stdout = String::from_utf8(output.stdout).context("git log output was not UTF-8")?;
    let mut commits = parse_git_commits(&stdout);
    for commit in &mut commits {
        commit.changed_files = git_changed_files(repo, &commit.hash)?;
    }
    Ok(commits)
}

fn git_commit_for_ref(repo: &Path, revision: &str) -> Result<GitCommit> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("log")
        .arg("-n")
        .arg("1")
        .arg(format!("--format={GIT_LOG_FORMAT}"))
        .arg(revision)
        .output()
        .with_context(|| format!("could not read commit {revision} in {}", repo.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git log failed for commit {revision} in {}: {}",
            repo.display(),
            stderr.trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("git log output was not UTF-8")?;
    let mut commits = parse_git_commits(&stdout);
    let mut commit = commits
        .pop()
        .with_context(|| format!("could not find commit {revision} in {}", repo.display()))?;
    commit.changed_files = git_changed_files(repo, &commit.hash)?;
    Ok(commit)
}

fn git_changed_files(repo: &Path, commit_hash: &str) -> Result<Vec<String>> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("diff-tree")
        .arg("--no-commit-id")
        .arg("--name-only")
        .arg("-r")
        .arg("--root")
        .arg(commit_hash)
        .output()
        .with_context(|| format!("could not read changed files for commit {commit_hash}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git diff-tree failed for commit {commit_hash}: {}",
            stderr.trim()
        );
    }
    let stdout = String::from_utf8(output.stdout).context("git diff-tree output was not UTF-8")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(normalize_git_path)
        .collect())
}

fn git_revision_range(since: Option<&str>, until: Option<&str>) -> Option<String> {
    match (since, until) {
        (Some(since), Some(until)) => Some(format!("{since}..{until}")),
        (Some(since), None) => Some(format!("{since}..HEAD")),
        (None, Some(until)) => Some(until.to_owned()),
        (None, None) => None,
    }
}

fn parse_git_commits(source: &str) -> Vec<GitCommit> {
    source
        .split('\x1e')
        .filter_map(|record| {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                return None;
            }
            let mut fields = record.splitn(6, '\x1f');
            Some(GitCommit {
                hash: fields.next()?.to_owned(),
                author_name: fields.next()?.to_owned(),
                author_email: fields.next()?.to_owned(),
                author_date: fields.next()?.to_owned(),
                subject: fields.next()?.to_owned(),
                body: fields.next().unwrap_or_default().trim().to_owned(),
                changed_files: Vec::new(),
            })
        })
        .collect()
}

fn export_git_connect_work(
    args: &GitConnectArgs,
    artifact: &ProjectAnalysis,
    report: &GitConnectReport,
) -> Result<Option<GitConnectExport>> {
    let Some(output) = &args.export_work else {
        return Ok(None);
    };

    let exported = report
        .records
        .iter()
        .filter(|connection| connection.status == "needs_record")
        .flat_map(|connection| works_from_git_connection(artifact, connection, &args.source))
        .collect::<Vec<_>>();
    let mut works = if output.exists() {
        read_work_sidecar(output)?
    } else {
        Vec::new()
    };
    let written = exported.len();
    merge_works(&mut works, exported);
    write_text_file(output, &write_works(&works, args.minify)?)?;
    Ok(Some(GitConnectExport {
        path: output.display().to_string(),
        written,
        source: args.source.clone(),
    }))
}

fn works_from_git_connection(
    artifact: &ProjectAnalysis,
    connection: &GitConnection,
    source: &str,
) -> Vec<Work> {
    let missing_expectations =
        missing_expectation_work_records(artifact, &connection.commit, &connection.expectations);
    if !missing_expectations.is_empty() {
        let preserve_single_id = connection.expectations.len() == 1;
        return missing_expectations
            .iter()
            .map(|expectation| {
                work_from_git_connection(
                    artifact,
                    connection,
                    source,
                    Some(expectation.id.as_str()),
                    preserve_single_id,
                )
            })
            .collect();
    }
    vec![work_from_git_connection(
        artifact, connection, source, None, true,
    )]
}

fn work_from_git_connection(
    artifact: &ProjectAnalysis,
    connection: &GitConnection,
    source: &str,
    expectation_id: Option<&str>,
    preserve_single_id: bool,
) -> Work {
    let (target, subject) = work_target_from_connection(artifact, connection, expectation_id);
    Work {
        id: git_connection_work_id(&connection.commit, expectation_id, preserve_single_id),
        target,
        subject,
        expectation_id: expectation_id.map(str::to_owned),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: source.to_owned(),
        evidence: Some(format!("commit:{}", connection.commit)),
        title: connection.title.clone(),
        detail: git_connect_work_detail(connection),
    }
}

fn work_from_git_link(commit: &GitCommit, expectation: &Expectation, args: &GitLinkArgs) -> Work {
    Work {
        id: git_connection_work_id(&commit.hash, Some(expectation.id.as_str()), false),
        target: expectation.target,
        subject: expectation.subject.clone(),
        expectation_id: Some(expectation.id.clone()),
        kind: WorkKind::from(args.kind.clone()),
        status: WorkStatus::from(args.status.clone()),
        source: args.source.clone(),
        evidence: Some(format!("commit:{}", commit.hash)),
        title: args.title.clone().unwrap_or_else(|| commit.subject.clone()),
        detail: git_link_work_detail(commit, expectation, args.detail.as_deref()),
    }
}

fn work_target_from_connection(
    artifact: &ProjectAnalysis,
    connection: &GitConnection,
    expectation_id: Option<&str>,
) -> (ExpectationTarget, Option<String>) {
    if let Some(expectation_id) = expectation_id
        && let Some(expectation) = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == expectation_id)
        && (expectation.target == ExpectationTarget::Project || expectation.subject.is_some())
    {
        return (expectation.target, expectation.subject.clone());
    }
    if connection.workflows.len() == 1 {
        return (
            ExpectationTarget::Workflow,
            Some(connection.workflows[0].id.clone()),
        );
    }
    (ExpectationTarget::Project, None)
}

fn git_connect_work_detail(connection: &GitConnection) -> String {
    let mut detail = format!(
        "Generated by git connect.\nCommit: {}\nAuthor: {}\nDate: {}\nStatus before export: {}\nReasons: {}",
        connection.commit,
        connection.author,
        connection.date,
        connection.status,
        connection.reasons.join("; ")
    );
    append_connected_records(&mut detail, "Workflows", &connection.workflows);
    append_connected_records(&mut detail, "Expectations", &connection.expectations);
    append_connected_records(&mut detail, "Verifications", &connection.verifications);
    append_connected_records(&mut detail, "Decisions", &connection.decisions);
    if !connection.changed_files.is_empty() {
        detail.push_str("\nChanged files:");
        for path in &connection.changed_files {
            detail.push_str("\n- ");
            detail.push_str(path);
        }
    }
    detail
}

fn git_link_work_detail(
    commit: &GitCommit,
    expectation: &Expectation,
    note: Option<&str>,
) -> String {
    let mut detail = format!(
        "Generated by git link.\nCommit: {}\nAuthor: {} <{}>\nDate: {}\nExpectation: {} - {}",
        commit.hash,
        commit.author_name,
        commit.author_email,
        commit.author_date,
        expectation.id,
        expectation.title
    );
    if !commit.changed_files.is_empty() {
        detail.push_str("\nChanged files:");
        for path in &commit.changed_files {
            detail.push_str("\n- ");
            detail.push_str(path);
        }
    }
    if let Some(note) = note.filter(|value| !value.trim().is_empty()) {
        detail.push_str("\n\nNote:\n");
        detail.push_str(note);
    }
    detail
}

fn append_connected_records(detail: &mut String, title: &str, records: &[GitConnectedRecord]) {
    if records.is_empty() {
        return;
    }
    detail.push('\n');
    detail.push_str(title);
    detail.push(':');
    for record in records {
        detail.push_str("\n- ");
        detail.push_str(&record.id);
        detail.push_str(" - ");
        detail.push_str(&record.title);
        detail.push_str(" (");
        detail.push_str(&record.reason);
        detail.push(')');
    }
}

fn print_git_connect_report(
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

fn print_git_suggestions(record: &GitConnection) {
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

fn print_connected_section(title: &str, records: &[GitConnectedRecord]) {
    if records.is_empty() {
        return;
    }
    println!("  {title}:");
    for record in records {
        println!("    - {}  {} ({})", record.id, record.title, record.reason);
    }
}

fn print_git_connect_json(
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

fn imported_git_work(
    commit: &GitCommit,
    source: &str,
    context: &GitImportContext<'_>,
) -> ImportedGitWork {
    let target = git_work_target(commit, context);
    let linked_expectation = linked_git_expectation(commit, context.artifact);
    let target = git_target_with_expectation(target, linked_expectation.as_ref());
    let targeting = target.note.clone();
    let work = Work {
        id: git_work_id(&commit.hash),
        target: target.target,
        subject: target.subject,
        expectation_id: linked_expectation.map(|expectation| expectation.id.clone()),
        kind: WorkKind::Implementation,
        status: WorkStatus::Completed,
        source: source.to_owned(),
        evidence: Some(format!("commit:{}", commit.hash)),
        title: commit.subject.clone(),
        detail: git_work_detail(commit, &target.note),
    };
    ImportedGitWork {
        work,
        commit_hash: commit.hash.clone(),
        targeting,
        changed_files: commit.changed_files.clone(),
    }
}

fn print_git_import_json(output: &Path, imported: &[ImportedGitWork]) -> Result<()> {
    let output = build_git_import_json(output, imported);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).context("could not serialize git import report")?
    );
    Ok(())
}

fn build_git_import_json<'a>(output: &Path, imported: &'a [ImportedGitWork]) -> GitImportJson<'a> {
    let records = imported
        .iter()
        .map(|imported| GitImportRecordJson {
            id: &imported.work.id,
            commit: &imported.commit_hash,
            target: imported.work.target.to_string(),
            subject: imported.work.subject.as_deref(),
            expectation: imported.work.expectation_id.as_deref(),
            title: &imported.work.title,
            targeting: &imported.targeting,
            changed_files: &imported.changed_files,
        })
        .collect::<Vec<_>>();
    GitImportJson {
        output: output.display().to_string(),
        imported: imported.len(),
        records,
    }
}

#[derive(Debug)]
struct GitWorkTarget {
    target: ExpectationTarget,
    subject: Option<String>,
    note: String,
}

#[derive(Debug)]
struct GitExpectationLink {
    id: String,
    target: ExpectationTarget,
    subject: Option<String>,
}

fn git_target_with_expectation(
    target: GitWorkTarget,
    expectation: Option<&GitExpectationLink>,
) -> GitWorkTarget {
    let Some(expectation) = expectation else {
        return target;
    };
    if target.target == ExpectationTarget::Project
        && expectation.target != ExpectationTarget::Project
        && expectation.subject.is_some()
    {
        return GitWorkTarget {
            target: expectation.target,
            subject: expectation.subject.clone(),
            note: format!(
                "{} Linked exact expectation id `{}` and used its target.",
                target.note, expectation.id
            ),
        };
    }
    GitWorkTarget {
        note: format!(
            "{} Linked exact expectation id `{}`.",
            target.note, expectation.id
        ),
        ..target
    }
}

fn linked_git_expectation(
    commit: &GitCommit,
    artifact: Option<&ProjectAnalysis>,
) -> Option<GitExpectationLink> {
    let artifact = artifact?;
    let searchable = format!("{}\n{}", commit.subject, commit.body);
    let mut matches = explicitly_linked_git_expectations(artifact, &searchable);
    if matches.is_empty()
        && let Some(expectation_id) = single_language_matched_expectation(artifact, &searchable)
        && let Some(expectation) = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == expectation_id)
    {
        matches.push(GitExpectationLink {
            id: expectation.id.clone(),
            target: expectation.target,
            subject: expectation.subject.clone(),
        });
    }
    matches.sort_by(|left, right| left.id.cmp(&right.id));
    matches.dedup_by(|left, right| left.id == right.id);
    (matches.len() == 1).then(|| matches.remove(0))
}

fn explicitly_linked_git_expectations(
    artifact: &ProjectAnalysis,
    searchable: &str,
) -> Vec<GitExpectationLink> {
    artifact
        .expectations
        .iter()
        .filter(|expectation| contains_token(searchable, &expectation.id))
        .map(|expectation| GitExpectationLink {
            id: expectation.id.clone(),
            target: expectation.target,
            subject: expectation.subject.clone(),
        })
        .collect::<Vec<_>>()
}

fn git_work_target(commit: &GitCommit, context: &GitImportContext<'_>) -> GitWorkTarget {
    let Some(artifact) = context.artifact else {
        return project_git_target("No artifact supplied; imported as project-wide work.");
    };
    if context.target_depth == GitTargetDepth::Project {
        return project_git_target("Target depth is project.");
    }

    let matched_file_ids = matched_artifact_file_ids(artifact, &commit.changed_files);
    if context.target_depth == GitTargetDepth::Workflow
        && let Some(workflow_id) = single_workflow_for_files(artifact, &matched_file_ids)
    {
        return GitWorkTarget {
            target: ExpectationTarget::Workflow,
            subject: Some(workflow_id),
            note: "Matched exactly one workflow from changed files.".to_owned(),
        };
    }

    if matched_file_ids.len() == 1 {
        return GitWorkTarget {
            target: ExpectationTarget::File,
            subject: matched_file_ids.first().cloned(),
            note: "Matched exactly one artifact file from changed files.".to_owned(),
        };
    }

    project_git_target(if matched_file_ids.is_empty() {
        "Changed files did not match artifact files."
    } else {
        "Changed files matched multiple artifact files."
    })
}

fn project_git_target(note: &str) -> GitWorkTarget {
    GitWorkTarget {
        target: ExpectationTarget::Project,
        subject: None,
        note: note.to_owned(),
    }
}

fn single_workflow_for_files(artifact: &ProjectAnalysis, file_ids: &[String]) -> Option<String> {
    let mut workflows = artifact
        .workflows
        .iter()
        .filter(|workflow| file_ids.iter().any(|file_id| file_id == &workflow.file_id))
        .map(|workflow| workflow.id.clone())
        .collect::<Vec<_>>();
    workflows.sort();
    workflows.dedup();
    (workflows.len() == 1).then(|| workflows.remove(0))
}

fn git_work_id(hash: &str) -> String {
    let short = hash.chars().take(16).collect::<String>();
    format!("wk_git_{short}")
}

fn git_connection_work_id(
    hash: &str,
    expectation_id: Option<&str>,
    preserve_single_id: bool,
) -> String {
    if preserve_single_id {
        return git_work_id(hash);
    }
    let Some(expectation_id) = expectation_id else {
        return git_work_id(hash);
    };
    let mut digest = Sha256::new();
    digest.update(hash.as_bytes());
    digest.update([0]);
    digest.update(expectation_id.as_bytes());
    let suffix = hex_prefix(&digest.finalize(), 4);
    format!("{}_{}", git_work_id(hash), suffix)
}

fn short_hash(hash: &str) -> String {
    hash.chars().take(8).collect()
}

fn git_work_detail(commit: &GitCommit, target_note: &str) -> String {
    let mut detail = format!(
        "Author: {} <{}>\nDate: {}\nCommit: {}\nTargeting: {}",
        commit.author_name, commit.author_email, commit.author_date, commit.hash, target_note
    );
    if !commit.changed_files.is_empty() {
        detail.push_str("\nChanged files:");
        for path in &commit.changed_files {
            detail.push_str("\n- ");
            detail.push_str(path);
        }
    }
    if !commit.body.is_empty() {
        detail.push_str("\n\n");
        detail.push_str(&commit.body);
    }
    detail
}

fn normalize_git_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

fn expectation_subject(
    target: ExpectationTarget,
    subject: Option<String>,
) -> Result<Option<String>> {
    target_subject("expectations", target, subject)
}

fn target_subject(
    noun: &str,
    target: ExpectationTarget,
    subject: Option<String>,
) -> Result<Option<String>> {
    match (target, subject) {
        (ExpectationTarget::Project, None) => Ok(None),
        (ExpectationTarget::Project, Some(_)) => {
            bail!("project {noun} are project-wide; omit --subject")
        }
        (_, Some(subject)) => Ok(Some(subject)),
        (_, None) => bail!("{target} {noun} require --subject"),
    }
}

fn expectation_id(
    target: ExpectationTarget,
    subject: Option<&str>,
    status: ExpectationStatus,
    source: &str,
    title: &str,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        target.to_string(),
        subject.unwrap_or("-").to_owned(),
        status.to_string(),
        source.to_owned(),
        title.to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("e_{}", hex_prefix(&hash.finalize(), 8))
}

fn verification_id(
    expectation_id: &str,
    status: VerificationStatus,
    supersedes: Option<&str>,
    method: &str,
    source: &str,
    evidence: Option<&str>,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        expectation_id.to_owned(),
        status.to_string(),
        supersedes.unwrap_or("-").to_owned(),
        method.to_owned(),
        source.to_owned(),
        evidence.unwrap_or("-").to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("v_{}", hex_prefix(&hash.finalize(), 8))
}

fn decision_id(
    target: ExpectationTarget,
    subject: Option<&str>,
    status: DecisionStatus,
    source: &str,
    title: &str,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        target.to_string(),
        subject.unwrap_or("-").to_owned(),
        status.to_string(),
        source.to_owned(),
        title.to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("d_{}", hex_prefix(&hash.finalize(), 8))
}

#[allow(clippy::too_many_arguments)]
fn work_id(
    target: ExpectationTarget,
    subject: Option<&str>,
    expectation: Option<&str>,
    kind: WorkKind,
    status: WorkStatus,
    source: &str,
    evidence: Option<&str>,
    title: &str,
    detail: &str,
) -> String {
    let mut hash = Sha256::new();
    for part in [
        target.to_string(),
        subject.unwrap_or("-").to_owned(),
        expectation.unwrap_or("-").to_owned(),
        kind.to_string(),
        status.to_string(),
        source.to_owned(),
        evidence.unwrap_or("-").to_owned(),
        title.to_owned(),
        detail.to_owned(),
    ] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("w_{}", hex_prefix(&hash.finalize(), 8))
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    let mut output = String::with_capacity(count * 2);
    for byte in bytes.iter().take(count) {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn has_records_other_than(source: &str, allowed_kind: &str) -> bool {
    let mut statement_start = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in source.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }

        if character == '"' {
            in_string = true;
        } else if character == ';' {
            if statement_has_record_other_than(&source[statement_start..index], allowed_kind) {
                return true;
            }
            statement_start = index + character.len_utf8();
        }
    }

    statement_has_record_other_than(&source[statement_start..], allowed_kind)
}

fn statement_has_record_other_than(statement: &str, allowed_kind: &str) -> bool {
    statement
        .split_whitespace()
        .next()
        .is_some_and(|kind| kind != allowed_kind)
}

fn read_expectations_file(path: &PathBuf) -> Result<Vec<Expectation>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_expectations(&source)
        .with_context(|| format!("could not parse expectations from {}", path.display()))
}

fn read_expectation_sidecar(path: &PathBuf) -> Result<Vec<Expectation>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "expectation") {
        bail!(
            "{} looks like a full .susu artifact; use an expectation-only sidecar file",
            path.display()
        );
    }
    parse_expectations(&source)
        .with_context(|| format!("could not parse expectations from {}", path.display()))
}

fn read_verifications_file(path: &PathBuf) -> Result<Vec<Verification>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_verifications(&source)
        .with_context(|| format!("could not parse verifications from {}", path.display()))
}

fn read_verification_sidecar(path: &PathBuf) -> Result<Vec<Verification>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "verification") {
        bail!(
            "{} looks like a full .susu artifact; use a verification-only sidecar file",
            path.display()
        );
    }
    parse_verifications(&source)
        .with_context(|| format!("could not parse verifications from {}", path.display()))
}

fn read_decisions_file(path: &PathBuf) -> Result<Vec<Decision>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_decisions(&source)
        .with_context(|| format!("could not parse decisions from {}", path.display()))
}

fn read_decision_sidecar(path: &PathBuf) -> Result<Vec<Decision>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "decision") {
        bail!(
            "{} looks like a full .susu artifact; use a decision-only sidecar file",
            path.display()
        );
    }
    parse_decisions(&source)
        .with_context(|| format!("could not parse decisions from {}", path.display()))
}

fn read_works_file(path: &PathBuf) -> Result<Vec<Work>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_works(&source).with_context(|| format!("could not parse work from {}", path.display()))
}

fn read_work_sidecar(path: &PathBuf) -> Result<Vec<Work>> {
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    if has_records_other_than(&source, "work") {
        bail!(
            "{} looks like a full .susu artifact; use a work-only sidecar file",
            path.display()
        );
    }
    parse_works(&source).with_context(|| format!("could not parse work from {}", path.display()))
}

fn merge_expectations(existing: &mut Vec<Expectation>, imported: Vec<Expectation>) {
    for expectation in imported {
        existing.retain(|current| current.id != expectation.id);
        existing.push(expectation);
    }
}

fn merge_verifications(existing: &mut Vec<Verification>, imported: Vec<Verification>) {
    for verification in imported {
        existing.retain(|current| current.id != verification.id);
        existing.push(verification);
    }
}

fn merge_decisions(existing: &mut Vec<Decision>, imported: Vec<Decision>) {
    for decision in imported {
        existing.retain(|current| current.id != decision.id);
        existing.push(decision);
    }
}

fn merge_works(existing: &mut Vec<Work>, imported: Vec<Work>) {
    for work in imported {
        existing.retain(|current| current.id != work.id);
        existing.push(work);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use susumu::model::{
        Confidence, Finding, Language, Location, SCHEMA_VERSION, Severity, SourceFile, Workflow,
        WorkflowKind,
    };

    fn test_artifact() -> ProjectAnalysis {
        ProjectAnalysis {
            schema_version: SCHEMA_VERSION,
            project_name: "fixture".to_owned(),
            root: ".".to_owned(),
            generated_unix_seconds: 0,
            files: vec![
                SourceFile {
                    id: "f_api".to_owned(),
                    path: "src/api.ts".to_owned(),
                    language: Language::TypeScript,
                    lines: 10,
                    bytes: 100,
                    content_hash: Some("hash-api".to_owned()),
                },
                SourceFile {
                    id: "f_routes".to_owned(),
                    path: "src/routes.php".to_owned(),
                    language: Language::Php,
                    lines: 10,
                    bytes: 100,
                    content_hash: Some("hash-routes".to_owned()),
                },
            ],
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: vec![
                Workflow {
                    id: "w_checkout".to_owned(),
                    kind: WorkflowKind::Http,
                    framework: "express-compatible".to_owned(),
                    trigger: "POST /checkout".to_owned(),
                    handler: Some("checkout".to_owned()),
                    entry_symbol: Some("s_checkout".to_owned()),
                    file_id: "f_api".to_owned(),
                    confidence: Confidence::Exact,
                    location: test_location(),
                },
                Workflow {
                    id: "w_php_checkout".to_owned(),
                    kind: WorkflowKind::Http,
                    framework: "laravel".to_owned(),
                    trigger: "POST /php-checkout".to_owned(),
                    handler: Some("php_checkout".to_owned()),
                    entry_symbol: Some("s_php_checkout".to_owned()),
                    file_id: "f_routes".to_owned(),
                    confidence: Confidence::Exact,
                    location: test_location(),
                },
            ],
            workflow_priorities: Vec::new(),
            flows: Vec::new(),
            expectations: vec![Expectation {
                id: "e_checkout_sequence".to_owned(),
                target: ExpectationTarget::Workflow,
                subject: Some("w_checkout".to_owned()),
                status: ExpectationStatus::Accepted,
                source: "human:product".to_owned(),
                title: "Checkout reserves inventory before charging".to_owned(),
                detail: "Checkout must reserve inventory before payment capture.".to_owned(),
            }],
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            findings: Vec::new(),
        }
    }

    const fn test_location() -> Location {
        Location {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        }
    }

    fn test_commit(subject: &str, body: &str, changed_files: &[&str]) -> GitCommit {
        GitCommit {
            hash: "f240cd96a07f2ea7b14cc1932c58914ed0871575".to_owned(),
            author_name: "Codex".to_owned(),
            author_email: "codex@example.test".to_owned(),
            author_date: "2026-07-15T12:00:00-05:00".to_owned(),
            subject: subject.to_owned(),
            body: body.to_owned(),
            changed_files: changed_files
                .iter()
                .map(|path| (*path).to_owned())
                .collect(),
        }
    }

    #[test]
    fn git_link_command_parses_commit_and_expectation() {
        let cli = Cli::try_parse_from([
            "susumu",
            "git",
            "link",
            "abc123",
            "e_checkout_sequence",
            "--kind",
            "documentation",
        ])
        .expect("parse git link");

        match cli.command.expect("command") {
            Command::Git {
                command: Some(GitCommand::Link(args)),
                ..
            } => {
                assert_eq!(args.commit, "abc123");
                assert_eq!(args.expectation, "e_checkout_sequence");
                assert_eq!(WorkKind::from(args.kind), WorkKind::Documentation);
            }
            other => panic!("expected git link command, got {other:?}"),
        }
    }

    #[test]
    fn work_kind_accepts_infrastructure() {
        let kind = "infrastructure"
            .parse::<WorkKind>()
            .expect("parse infrastructure kind");

        assert_eq!(kind, WorkKind::Infrastructure);
        assert_eq!(kind.to_string(), "infrastructure");
    }

    #[test]
    fn git_link_command_parses_infrastructure_kind() {
        let cli = Cli::try_parse_from([
            "susumu",
            "git",
            "link",
            "abc123",
            "e_ci_artifacts",
            "--kind",
            "infrastructure",
        ])
        .expect("parse git link infrastructure kind");

        match cli.command.expect("command") {
            Command::Git {
                command: Some(GitCommand::Link(args)),
                ..
            } => {
                assert_eq!(args.commit, "abc123");
                assert_eq!(args.expectation, "e_ci_artifacts");
                assert_eq!(WorkKind::from(args.kind), WorkKind::Infrastructure);
            }
            other => panic!("expected git link command, got {other:?}"),
        }
    }

    #[test]
    fn git_link_work_targets_explicit_expectation() {
        let artifact = test_artifact();
        let expectation = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == "e_checkout_sequence")
            .expect("expectation");
        let commit = test_commit(
            "docs: explain checkout sequence",
            "",
            &["README.md", "docs/checkout.md"],
        );

        let work = work_from_git_link(
            &commit,
            expectation,
            &GitLinkArgs {
                repo: PathBuf::from("."),
                artifact: PathBuf::from(".susumu/project.susu"),
                output: PathBuf::from(".susumu/work.susu"),
                commit: commit.hash.clone(),
                expectation: expectation.id.clone(),
                source: "human:git-link".to_owned(),
                kind: WorkKindArg(WorkKind::Documentation),
                status: WorkStatusArg(WorkStatus::Completed),
                title: None,
                detail: Some("Explicitly linked after review.".to_owned()),
                minify: false,
                json: false,
            },
        );

        assert_eq!(
            work.id,
            git_connection_work_id(&commit.hash, Some(expectation.id.as_str()), false)
        );
        assert!(work.id.starts_with("wk_git_f240cd96a07f2ea7_"));
        assert_eq!(work.target, ExpectationTarget::Workflow);
        assert_eq!(work.subject.as_deref(), Some("w_checkout"));
        assert_eq!(work.expectation_id.as_deref(), Some("e_checkout_sequence"));
        assert_eq!(work.kind, WorkKind::Documentation);
        let expected_evidence = format!("commit:{}", commit.hash);
        assert_eq!(work.evidence.as_deref(), Some(expected_evidence.as_str()));
        assert!(work.detail.contains("Generated by git link."));
        assert!(work.detail.contains("README.md"));
        assert!(work.detail.contains("Explicitly linked after review."));
    }

    #[test]
    fn git_link_work_ids_include_expectation_to_avoid_overwriting_other_links() {
        let mut artifact = test_artifact();
        artifact.expectations.push(Expectation {
            id: "e_second_expectation".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Second expectation".to_owned(),
            detail: "A second expectation can be linked to the same commit.".to_owned(),
        });
        let first = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == "e_checkout_sequence")
            .expect("first expectation");
        let second = artifact
            .expectations
            .iter()
            .find(|expectation| expectation.id == "e_second_expectation")
            .expect("second expectation");
        let commit = test_commit("test: verify two expectations", "", &["README.md"]);
        let args = GitLinkArgs {
            repo: PathBuf::from("."),
            artifact: PathBuf::from(".susumu/project.susu"),
            output: PathBuf::from(".susumu/work.susu"),
            commit: commit.hash.clone(),
            expectation: first.id.clone(),
            source: "human:git-link".to_owned(),
            kind: WorkKindArg(WorkKind::Verification),
            status: WorkStatusArg(WorkStatus::Completed),
            title: None,
            detail: None,
            minify: false,
            json: false,
        };

        let first_work = work_from_git_link(&commit, first, &args);
        let second_work = work_from_git_link(
            &commit,
            second,
            &GitLinkArgs {
                expectation: second.id.clone(),
                ..args
            },
        );

        assert_ne!(first_work.id, second_work.id);
        assert!(first_work.id.starts_with("wk_git_f240cd96a07f2ea7_"));
        assert!(second_work.id.starts_with("wk_git_f240cd96a07f2ea7_"));
        assert_eq!(
            first_work.evidence.as_deref(),
            second_work.evidence.as_deref()
        );
        assert_eq!(
            first_work.expectation_id.as_deref(),
            Some("e_checkout_sequence")
        );
        assert_eq!(
            second_work.expectation_id.as_deref(),
            Some("e_second_expectation")
        );
    }

    #[test]
    fn init_writes_starter_expectations() {
        let temp = tempfile::tempdir().expect("tempdir");
        init_repository(&InitArgs {
            target: temp.path().to_path_buf(),
            file: PathBuf::from("expectations.susu"),
            name: Some("Acme Checkout".to_owned()),
            source: "human:test".to_owned(),
            force: false,
        })
        .expect("init should write expectations");

        let expectations =
            read_expectations_file(&temp.path().join("expectations.susu")).expect("expectations");
        assert_eq!(expectations.len(), 3);
        assert_eq!(expectations[0].target, ExpectationTarget::Project);
        assert_eq!(expectations[0].subject, None);
        assert_eq!(expectations[0].status, ExpectationStatus::Accepted);
        assert_eq!(expectations[0].source, "human:test");
        assert!(expectations[0].title.contains("Acme Checkout"));
        assert!(
            expectations[0]
                .detail
                .contains("authored expectations.susu sidecar")
        );
        assert_eq!(expectations[1].status, ExpectationStatus::Proposed);
        assert!(expectations[1].title.contains("primary workflows"));
        assert!(
            expectations[1]
                .detail
                .contains("business or product workflows")
        );
        assert_eq!(expectations[2].status, ExpectationStatus::Proposed);
        assert!(expectations[2].title.contains("verification evidence"));
        assert!(
            expectations[2]
                .detail
                .contains("tests, CI runs, manual reviews")
        );
    }

    #[test]
    fn init_refuses_to_overwrite_without_force() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("expectations.susu");
        fs::write(&file, "expectation e_existing target=project subject=- status=accepted source=\"human:test\" title=\"Existing\" detail=\"Existing expectation.\";\n")
            .expect("write existing sidecar");

        let result = init_repository(&InitArgs {
            target: temp.path().to_path_buf(),
            file: PathBuf::from("expectations.susu"),
            name: None,
            source: "human:test".to_owned(),
            force: false,
        });

        assert!(result.is_err());
        let existing = fs::read_to_string(file).expect("read existing sidecar");
        assert!(existing.contains("e_existing"));
    }

    #[test]
    fn directory_scans_auto_merge_expectations_sidecar() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("expectations.susu"),
            "expectation e_auto target=project subject=- status=accepted source=\"human:test\" title=\"Auto loaded\" detail=\"Directory scans should load this sidecar.\";\n",
        )
        .expect("write expectations sidecar");

        let analysis = load_analysis(&temp.path().to_path_buf(), None, None, None, None, false)
            .expect("load analysis");

        assert!(
            analysis
                .expectations
                .iter()
                .any(|expectation| expectation.id == "e_auto")
        );
    }

    #[test]
    fn directory_scans_auto_merge_verifications_sidecar() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("expectations.susu"),
            "expectation e_auto target=project subject=- status=accepted source=\"human:test\" title=\"Auto loaded\" detail=\"Directory scans should load this sidecar.\";\n",
        )
        .expect("write expectations sidecar");
        fs::write(
            temp.path().join("verifications.susu"),
            "verification v_auto expectation=e_auto status=passed method=\"manual review\" source=\"human:test\" evidence=- basis=- detail=\"Reviewed.\";\n",
        )
        .expect("write verifications sidecar");

        let analysis = load_analysis(&temp.path().to_path_buf(), None, None, None, None, false)
            .expect("load analysis");

        assert!(
            analysis
                .verifications
                .iter()
                .any(|verification| verification.id == "v_auto")
        );
    }

    #[test]
    fn shortcut_commands_parse_without_subcommands() {
        let review = Cli::try_parse_from(["susumu", "review"]).expect("parse review shortcut");
        match review.command.expect("review command") {
            Command::Review { command, .. } => assert!(command.is_none()),
            other => panic!("expected review shortcut, got {other:?}"),
        }

        let git = Cli::try_parse_from(["susumu", "git"]).expect("parse git shortcut");
        match git.command.expect("git command") {
            Command::Git { command, .. } => assert!(command.is_none()),
            other => panic!("expected git shortcut, got {other:?}"),
        }
    }

    #[test]
    fn expectations_command_parses_search_status_and_json() {
        let cli = Cli::try_parse_from([
            "susumu",
            "expectations",
            "--search",
            "git",
            "--status",
            "accepted",
            "--json",
        ])
        .expect("parse expectations shortcut");

        match cli.command.expect("command") {
            Command::Expectations(args) => {
                assert_eq!(args.search.as_deref(), Some("git"));
                assert_eq!(
                    args.status.map(ExpectationStatus::from),
                    Some(ExpectationStatus::Accepted)
                );
                assert!(args.json);
            }
            other => panic!("expected expectations shortcut, got {other:?}"),
        }
    }

    #[test]
    fn readiness_command_parses_packet_and_json() {
        let cli = Cli::try_parse_from([
            "susumu",
            "readiness",
            "--packet",
            "custom.review.susu",
            "--max-items",
            "5",
            "--bucket",
            "needs_verification",
            "--search",
            "checkout",
            "--json",
        ])
        .expect("parse readiness shortcut");

        match cli.command.expect("command") {
            Command::Readiness(args) => {
                assert_eq!(args.packet, PathBuf::from("custom.review.susu"));
                assert_eq!(args.max_items, 5);
                assert_eq!(args.bucket.as_deref(), Some("needs_verification"));
                assert_eq!(args.search.as_deref(), Some("checkout"));
                assert!(args.json);
            }
            other => panic!("expected readiness shortcut, got {other:?}"),
        }
    }

    #[test]
    fn expectation_rows_filter_by_search_and_status() {
        let rows = vec![
            ExpectationBrowseRow {
                id: "e_git_links".to_owned(),
                title: "Git links are easy".to_owned(),
                detail: "Users can find expectation ids for git link.".to_owned(),
                target: "project".to_owned(),
                subject: None,
                status: "accepted".to_owned(),
                source: "human:test".to_owned(),
                support_status: Some("partially_supported".to_owned()),
                target_observed: Some(true),
                verification: None,
                work: Some(1),
                decisions: Some(0),
                findings: Some(0),
            },
            ExpectationBrowseRow {
                id: "e_future".to_owned(),
                title: "Future idea".to_owned(),
                detail: "A proposed expectation.".to_owned(),
                target: "project".to_owned(),
                subject: None,
                status: "proposed".to_owned(),
                source: "human:test".to_owned(),
                support_status: Some("needs_support".to_owned()),
                target_observed: Some(true),
                verification: None,
                work: Some(0),
                decisions: Some(0),
                findings: Some(0),
            },
        ];

        let filtered = filter_expectation_rows(rows, Some("git"), Some("accepted"));

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "e_git_links");
    }

    #[test]
    fn verify_command_parses_passed_status() {
        let cli = Cli::try_parse_from([
            "susumu",
            "verify",
            "e_checkout_sequence",
            "--passed",
            "--method",
            "cargo test checkout",
            "--evidence",
            "run:123",
        ])
        .expect("parse verify shortcut");

        match cli.command.expect("command") {
            Command::Verify(args) => {
                assert_eq!(args.expectation, "e_checkout_sequence");
                assert!(args.passed);
                assert_eq!(args.method, "cargo test checkout");
                assert_eq!(args.evidence.as_deref(), Some("run:123"));
            }
            other => panic!("expected verify shortcut, got {other:?}"),
        }
    }

    #[test]
    fn verify_shortcut_writes_verification_sidecar() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");
        fs::write(
            temp.path().join("expectations.susu"),
            "expectation e_verify target=project subject=- status=accepted source=\"human:test\" title=\"Verify shortcut\" detail=\"The verify shortcut should write verification records.\";\n",
        )
        .expect("write expectations sidecar");
        let output = temp.path().join("verifications.susu");

        verify_shortcut(VerifyArgs {
            expectation: "e_verify".to_owned(),
            target: temp.path().to_path_buf(),
            file: output.clone(),
            id: None,
            supersedes: None,
            passed: true,
            failed: false,
            inconclusive: false,
            method: "cargo test".to_owned(),
            source: "human:test".to_owned(),
            evidence: Some("run:123".to_owned()),
            basis: None,
            detail: None,
            minify: false,
            json: false,
        })
        .expect("verify succeeds");

        let verifications = read_verification_sidecar(&output).expect("read verification sidecar");
        assert_eq!(verifications.len(), 1);
        assert_eq!(verifications[0].expectation_id, "e_verify");
        assert_eq!(verifications[0].status, VerificationStatus::Passed);
        assert_eq!(verifications[0].method, "cargo test");
        assert_eq!(verifications[0].evidence.as_deref(), Some("run:123"));
        assert!(
            verifications[0]
                .detail
                .contains("Recorded by susumu verify.")
        );
    }

    #[test]
    fn verify_requires_one_status_flag() {
        let result = verification_status_from_flags(&VerifyArgs {
            expectation: "e_verify".to_owned(),
            target: PathBuf::from("."),
            file: PathBuf::from("verifications.susu"),
            id: None,
            supersedes: None,
            passed: false,
            failed: false,
            inconclusive: false,
            method: "manual review".to_owned(),
            source: "human:test".to_owned(),
            evidence: None,
            basis: None,
            detail: None,
            minify: false,
            json: false,
        });

        assert!(result.is_err());
    }

    #[test]
    fn verification_remove_preserves_append_only_history() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("verifications.susu");
        fs::write(
            &file,
            "verification v_old expectation=e_verify status=passed supersedes=- method=\"cargo test\" source=\"human:test\" evidence=- basis=- detail=\"Passed.\";\n",
        )
        .expect("write verification sidecar");

        let result = remove_verification(&RemoveVerification {
            file: file.clone(),
            id: "v_old".to_owned(),
        });

        assert!(result.is_err());
        assert!(fs::read_to_string(file).unwrap().contains("v_old"));
    }

    #[test]
    fn review_shortcut_writes_convention_based_outputs() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");
        fs::write(
            temp.path().join("expectations.susu"),
            "expectation e_review target=project subject=- status=accepted source=\"human:test\" title=\"Review stays easy\" detail=\"Daily review should use convention-based outputs.\";\n",
        )
        .expect("write expectations sidecar");
        fs::write(
            temp.path().join("verifications.susu"),
            "verification v_review expectation=e_review status=passed method=\"manual smoke test\" source=\"human:test\" evidence=\"local:review\" detail=\"The daily review command wrote convention-based outputs.\";\n",
        )
        .expect("write verifications sidecar");
        fs::write(
            temp.path().join(PORTAL_CONFIG_FILE),
            "[portal]\ntitle = \"Daily Memory\"\naccent = \"#778899\"\n",
        )
        .expect("write portal config");

        review_shortcut(&ReviewShortcutArgs {
            target: temp.path().to_path_buf(),
            output_dir: PathBuf::from(".susumu"),
            work: None,
            strict: false,
            fail_on_check: false,
            no_html: false,
            serve: false,
            host: "127.0.0.1".to_owned(),
            port: 0,
            json: false,
        })
        .expect("run review shortcut");

        let artifact_path = temp.path().join(".susumu").join("project.susu");
        let packet_path = temp.path().join(".susumu").join("review.susu");
        let check_path = temp.path().join(".susumu").join("check.json");
        let html_path = temp.path().join(".susumu").join("review.html");
        assert!(artifact_path.exists());
        assert!(packet_path.exists());
        assert!(check_path.exists());
        assert!(html_path.exists());

        let artifact = read_analysis_artifact(&artifact_path).expect("read project artifact");
        assert!(
            artifact
                .expectations
                .iter()
                .any(|expectation| expectation.id == "e_review")
        );
        assert!(
            artifact
                .verifications
                .iter()
                .any(|verification| verification.id == "v_review")
        );

        let packet = read_review_packet(&packet_path).expect("read review packet");
        assert_eq!(packet.artifact.expectations.len(), 1);
        assert_eq!(packet.artifact.verifications.len(), 1);
        assert!(
            packet
                .expectation_readiness
                .iter()
                .any(|item| item.expectation_id == "e_review" && item.bucket == "verified")
        );

        let check_json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&check_path).expect("read check json"))
                .expect("check json should parse");
        assert_eq!(check_json["project"]["name"], artifact.project_name);
        let html = fs::read_to_string(&html_path).expect("read html portal");
        assert!(html.contains("Daily Memory &middot;"));
        assert!(html.contains(":root{--accent:#778899}"));
    }

    #[test]
    fn open_static_review_requires_review_export() {
        let temp = tempfile::tempdir().expect("tempdir");
        let packet = temp.path().join("review.susu");

        let error = open_static_review(&packet).expect_err("missing HTML should fail");

        assert!(error.to_string().contains("run `susumu review` first"));
        assert!(error.to_string().contains("review.html"));
    }

    #[test]
    fn sidecar_record_guard_ignores_semicolons_inside_strings() {
        let work = "work wk_one target=project subject=- expectation=- kind=implementation status=completed source=\"test\" evidence=\"commit:abc\" title=\"One\" detail=\"Generated by git connect. Reasons: 1 expectation link(s); no work record references this commit.\";\n";

        assert!(!has_records_other_than(work, "work"));
        assert!(has_records_other_than(work, "expectation"));
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn expectation_support_counts_expectation_specific_work_only_once() {
        let mut artifact = test_artifact();
        artifact.expectations = vec![
            Expectation {
                id: "e_project_one".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "First project expectation".to_owned(),
                detail: "First expectation.".to_owned(),
            },
            Expectation {
                id: "e_project_two".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Second project expectation".to_owned(),
                detail: "Second expectation.".to_owned(),
            },
        ];
        artifact.works.push(Work {
            id: "wk_one".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            expectation_id: Some("e_project_one".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "import:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Support first expectation".to_owned(),
            detail: "Only the explicitly linked expectation should count this work.".to_owned(),
        });
        artifact.verifications.push(Verification {
            id: "v_one_passed".to_owned(),
            expectation_id: "e_project_one".to_owned(),
            status: VerificationStatus::Passed,
            supersedes: None,
            method: "cargo test".to_owned(),
            source: "ci:test".to_owned(),
            evidence: Some("run:1".to_owned()),
            basis: None,
            detail: "Passed.".to_owned(),
        });
        artifact.verifications.push(Verification {
            id: "v_one_failed".to_owned(),
            expectation_id: "e_project_one".to_owned(),
            status: VerificationStatus::Failed,
            supersedes: None,
            method: "manual review".to_owned(),
            source: "human:test".to_owned(),
            evidence: Some("review:1".to_owned()),
            basis: None,
            detail: "Failed.".to_owned(),
        });
        artifact.verifications.push(Verification {
            id: "v_one_inconclusive".to_owned(),
            expectation_id: "e_project_one".to_owned(),
            status: VerificationStatus::Inconclusive,
            supersedes: None,
            method: "log review".to_owned(),
            source: "human:test".to_owned(),
            evidence: None,
            basis: None,
            detail: "Inconclusive.".to_owned(),
        });
        artifact.decisions.push(Decision {
            id: "d_project".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: DecisionStatus::Accepted,
            source: "human:test".to_owned(),
            basis: None,
            title: "Accept project direction".to_owned(),
            detail: "Project-wide decision context should count for project expectations."
                .to_owned(),
        });
        artifact.expectations.push(Expectation {
            id: "e_workflow_gap".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Workflow support includes findings".to_owned(),
            detail: "Findings tied to the workflow should appear in expectation support."
                .to_owned(),
        });
        artifact.findings.push(Finding {
            rule_id: "SUS999".to_owned(),
            source: "test".to_owned(),
            severity: Severity::Warning,
            title: "Workflow finding".to_owned(),
            detail: "A workflow finding should be counted.".to_owned(),
            file_id: Some("f_api".to_owned()),
            subject: Some("w_checkout".to_owned()),
            location: Some(test_location()),
        });

        let support = expectation_support(&artifact);
        let first = support
            .iter()
            .find(|item| item.expectation_id == "e_project_one")
            .expect("first support");
        let second = support
            .iter()
            .find(|item| item.expectation_id == "e_project_two")
            .expect("second support");
        let workflow = support
            .iter()
            .find(|item| item.expectation_id == "e_workflow_gap")
            .expect("workflow support");

        assert!(first.target_observed);
        assert_eq!(first.verification.passed, 1);
        assert_eq!(first.verification.failed, 1);
        assert_eq!(first.verification.inconclusive, 1);
        assert_eq!(first.work, 1);
        assert_eq!(first.decisions, 1);
        assert_eq!(first.findings, 0);
        assert_eq!(first.support_status, "failed_verification");
        assert!(
            first
                .reasons
                .iter()
                .any(|reason| reason == "target observed")
        );
        assert!(
            first
                .reasons
                .iter()
                .any(|reason| reason == "1 failed verification record(s)")
        );
        assert!(
            first
                .reasons
                .iter()
                .any(|reason| reason == "1 linked work record(s)")
        );
        assert!(
            first
                .reasons
                .iter()
                .any(|reason| reason == "1 linked decision record(s)")
        );
        assert_eq!(second.work, 0);
        assert_eq!(second.decisions, 1);
        assert_eq!(second.support_status, "partially_supported");
        assert_eq!(workflow.findings, 1);
        assert_eq!(workflow.support_status, "partially_supported");
        assert!(
            workflow
                .reasons
                .iter()
                .any(|reason| reason == "1 linked finding(s)")
        );
    }

    #[test]
    fn scanner_support_does_not_verify_expectations_without_passed_verification() {
        let mut artifact = test_artifact();
        refresh_derived_analysis(&mut artifact);
        artifact.works.push(Work {
            id: "wk_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement checkout sequence".to_owned(),
            detail: "Work supports the expectation but does not prove it.".to_owned(),
        });
        artifact.findings.push(Finding {
            rule_id: "SUS998".to_owned(),
            source: "scanner:test".to_owned(),
            severity: Severity::Info,
            title: "Observed checkout workflow".to_owned(),
            detail: "Scanner-observed evidence is support, not verification.".to_owned(),
            file_id: Some("f_api".to_owned()),
            subject: Some("w_checkout".to_owned()),
            location: Some(test_location()),
        });

        let support = expectation_support(&artifact);
        let readiness = crate::expectation_readiness::expectation_readiness(&artifact, &support);
        let checkout_support = support
            .iter()
            .find(|item| item.expectation_id == "e_checkout_sequence")
            .expect("checkout support");
        let checkout_readiness = readiness
            .iter()
            .find(|item| item.expectation_id == "e_checkout_sequence")
            .expect("checkout readiness");

        assert!(checkout_support.target_observed);
        assert_eq!(checkout_support.work, 1);
        assert_eq!(checkout_support.findings, 1);
        assert_eq!(checkout_support.verification.passed, 0);
        assert_eq!(checkout_support.support_status, "partially_supported");
        assert_eq!(checkout_readiness.bucket, "needs_verification");
        assert!(
            checkout_readiness
                .next_action
                .contains("susumu verify e_checkout_sequence")
        );

        artifact.verifications.push(Verification {
            id: "v_checkout".to_owned(),
            expectation_id: "e_checkout_sequence".to_owned(),
            status: VerificationStatus::Passed,
            supersedes: None,
            method: "cargo test checkout".to_owned(),
            source: "ci:test".to_owned(),
            evidence: Some("run:checkout".to_owned()),
            basis: None,
            detail: "Passed verification promotes support to verified.".to_owned(),
        });

        let verified_support = expectation_support(&artifact);
        let verified_readiness =
            crate::expectation_readiness::expectation_readiness(&artifact, &verified_support);
        let checkout_support = verified_support
            .iter()
            .find(|item| item.expectation_id == "e_checkout_sequence")
            .expect("verified support");
        let checkout_readiness = verified_readiness
            .iter()
            .find(|item| item.expectation_id == "e_checkout_sequence")
            .expect("verified readiness");

        assert_eq!(checkout_support.verification.passed, 1);
        assert_eq!(checkout_support.support_status, "verified");
        assert_eq!(checkout_readiness.bucket, "verified");
    }

    #[test]
    fn review_build_writes_artifact_packet_check_and_html() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("expectations.susu"),
            "expectation e_build target=project subject=- status=accepted source=\"human:test\" title=\"Build review\" detail=\"Review build should load this sidecar.\";\n",
        )
        .expect("write expectations sidecar");
        let artifact = temp.path().join("target").join("project.susu");
        let packet = temp.path().join("target").join("project.review.susu");
        let check = temp.path().join("target").join("project.check.json");
        let html = temp.path().join("target").join("project.review.html");

        build_review(&ReviewBuildArgs {
            target: temp.path().to_path_buf(),
            expectations: None,
            verifications: None,
            decisions: None,
            work: None,
            artifact_output: artifact.clone(),
            output: packet.clone(),
            check_json: Some(check.clone()),
            html: Some(html.clone()),
            strict: false,
            fail_on_check: false,
            json: false,
            serve: false,
            host: "127.0.0.1".to_owned(),
            port: 0,
        })
        .expect("review build should write outputs");

        assert!(artifact.exists());
        assert!(packet.exists());
        assert!(check.exists());
        assert!(html.exists());
        let built = read_analysis_artifact(&artifact).expect("read built artifact");
        assert!(
            built
                .expectations
                .iter()
                .any(|expectation| expectation.id == "e_build")
        );
    }

    #[test]
    fn core_review_build_has_no_ai_dependency() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");
        fs::write(
            temp.path().join("expectations.susu"),
            "expectation e_ai_optional target=project subject=- status=accepted source=\"human:test\" title=\"AI stays optional\" detail=\"Core scan, check, review packet, and portal output should work without AI configuration.\";\n",
        )
        .expect("write expectations sidecar");

        let artifact = temp.path().join("target").join("project.susu");
        let packet = temp.path().join("target").join("project.review.susu");
        let check = temp.path().join("target").join("project.check.json");
        let html = temp.path().join("target").join("project.review.html");

        build_review(&ReviewBuildArgs {
            target: temp.path().to_path_buf(),
            expectations: None,
            verifications: None,
            decisions: None,
            work: None,
            artifact_output: artifact.clone(),
            output: packet.clone(),
            check_json: Some(check.clone()),
            html: Some(html.clone()),
            strict: false,
            fail_on_check: false,
            json: false,
            serve: false,
            host: "127.0.0.1".to_owned(),
            port: 0,
        })
        .expect("core review build should not require AI configuration");

        let built = read_analysis_artifact(&artifact).expect("read built artifact");
        assert!(
            built
                .expectations
                .iter()
                .any(|expectation| expectation.id == "e_ai_optional")
        );
        assert!(check.exists());

        let packet = read_review_packet(&packet).expect("read review packet");
        assert!(
            packet
                .artifact
                .expectations
                .iter()
                .any(|expectation| expectation.id == "e_ai_optional")
        );
        assert!(
            fs::read_to_string(&html)
                .expect("read html portal")
                .contains("AI stays optional")
        );

        let readme = include_str!("../README.md");
        let vision = include_str!("../docs/vision.md");
        let vernacular = include_str!("../docs/vernacular.md");
        assert!(readme.contains("runs without AI keys"));
        assert!(vision.contains("The core product does not require AI."));
        assert!(vision.contains("Optional bring-your-own-key AI"));
        assert!(vision.contains("labeled as generated"));
        assert!(vision.contains("cite underlying evidence"));
        assert!(vision.contains("human acceptance"));
        assert!(vernacular.contains("source=\"ai:draft\""));
        assert!(vernacular.contains("status=proposed"));
    }

    #[test]
    fn handoff_flags_expectations_and_work_without_verification() {
        let mut artifact = test_artifact();
        refresh_derived_analysis(&mut artifact);
        artifact.works.push(Work {
            id: "wk_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement checkout sequence".to_owned(),
            detail: "Checkout implementation touched the workflow and now needs verification."
                .to_owned(),
        });

        let check = check_report(&artifact, false);
        let report = handoff_report(&artifact, &check);

        assert!(!report.top_workflows.is_empty());
        assert_eq!(report.top_workflows[0].id, "w_checkout");
        assert_eq!(
            report.expectations_without_verification[0].id,
            "e_checkout_sequence"
        );
        assert_eq!(report.work_needing_verification[0].id, "wk_checkout");
        assert!(
            report
                .next_actions
                .iter()
                .any(|action| action.contains("wk_checkout"))
        );
    }

    #[test]
    fn review_packet_embeds_artifact_and_handoff_state() {
        let mut artifact = test_artifact();
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("src")).expect("create source dir");
        fs::write(
            temp.path().join("src").join("api.ts"),
            "export function checkout() { return 'ok'; }\n",
        )
        .expect("write api source");
        fs::write(
            temp.path().join("src").join("routes.php"),
            "<?php function php_checkout() { return 'ok'; }\n",
        )
        .expect("write php source");
        artifact.root = temp.path().display().to_string();
        refresh_derived_analysis(&mut artifact);
        artifact.works.push(Work {
            id: "wk_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement checkout sequence".to_owned(),
            detail: "Checkout implementation touched the workflow and now needs verification."
                .to_owned(),
        });
        let check = check_report(&artifact, false);
        let handoff = handoff_report(&artifact, &check);

        let packet = review_packet("fixture.susu".to_owned(), 123, &artifact, &check, &handoff);
        let json = serde_json::to_value(packet).expect("packet serializes");

        assert_eq!(json["schema_version"], "susumu.review.v1");
        assert_eq!(json["created_unix_seconds"], 123);
        assert_eq!(json["source"]["input"], "fixture.susu");
        assert_eq!(json["project"]["name"], "fixture");
        assert_eq!(json["artifact"]["project_name"], "fixture");
        assert_eq!(json["artifact"]["workflows"].as_array().unwrap().len(), 2);
        let previews = json["source_previews"].as_array().expect("source previews");
        assert!(previews.iter().any(|preview| {
            preview["file_id"] == "f_api"
                && preview["path"] == "src/api.ts"
                && preview["lines"][0]["tokens"]
                    .as_array()
                    .is_some_and(|tokens| !tokens.is_empty())
        }));
        assert_eq!(json["top_workflows"][0]["id"], "w_checkout");
        assert_eq!(
            json["expectations_without_verification"][0]["id"],
            "e_checkout_sequence"
        );
        assert_eq!(
            json["expectation_support"][0]["expectation_id"],
            "e_checkout_sequence"
        );
        assert_eq!(
            json["expectation_support"][0]["support_status"],
            "partially_supported"
        );
        assert_eq!(json["expectation_support"][0]["target_observed"], true);
        assert_eq!(json["expectation_support"][0]["work"], 1);
        assert_eq!(
            json["expectation_readiness"][0]["expectation_id"],
            "e_checkout_sequence"
        );
        assert_eq!(
            json["expectation_readiness"][0]["bucket"],
            "needs_verification"
        );
        assert_eq!(
            json["expectation_readiness"][0]["label"],
            "Has work, needs verification"
        );
        assert!(
            json["expectation_readiness"][0]["next_action"]
                .as_str()
                .expect("next action")
                .contains("susumu verify e_checkout_sequence")
        );
        assert_eq!(json["work_needing_verification"][0]["id"], "wk_checkout");
    }

    fn write_portable_memory_sources(source_root: &Path) {
        fs::create_dir_all(source_root.join("src")).expect("create source dir");
        fs::write(
            source_root.join("src").join("api.ts"),
            "export function checkout() { return reserveInventory(); }\n",
        )
        .expect("write api source");
        fs::write(
            source_root.join("src").join("routes.php"),
            "<?php function php_checkout() { return 'ok'; }\n",
        )
        .expect("write php source");
    }

    fn portable_memory_artifact(source_root: &Path) -> ProjectAnalysis {
        let mut artifact = test_artifact();
        artifact.root = source_root.display().to_string();
        artifact.verifications.push(Verification {
            id: "v_checkout".to_owned(),
            expectation_id: "e_checkout_sequence".to_owned(),
            status: VerificationStatus::Passed,
            supersedes: None,
            method: "cargo test checkout".to_owned(),
            source: "ci:test".to_owned(),
            evidence: Some("run:checkout".to_owned()),
            basis: None,
            detail: "Checkout behavior was verified.".to_owned(),
        });
        artifact.decisions.push(Decision {
            id: "d_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            status: DecisionStatus::Accepted,
            source: "human:product".to_owned(),
            basis: None,
            title: "Keep checkout reservation first".to_owned(),
            detail: "The business accepted reserve-before-charge as durable intent.".to_owned(),
        });
        artifact.works.push(Work {
            id: "wk_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement checkout sequence".to_owned(),
            detail: "Checkout implementation touched the workflow.".to_owned(),
        });
        refresh_derived_analysis(&mut artifact);
        artifact
    }

    fn archive_project_memory(
        artifact: &ProjectAnalysis,
        archive_root: &Path,
    ) -> (PathBuf, PathBuf) {
        fs::create_dir_all(archive_root).expect("create archive dir");
        let check = check_report(artifact, false);
        let handoff = handoff_report(artifact, &check);
        let packet = review_packet(
            "source/project.susu".to_owned(),
            123,
            artifact,
            &check,
            &handoff,
        );
        let packet_json =
            serde_json::to_string_pretty(&packet).expect("packet should serialize to JSON");
        let archived_packet = archive_root.join("review.susu");
        let archived_artifact = archive_root.join("project.susu");
        fs::write(&archived_packet, packet_json).expect("write archived review packet");
        fs::write(
            &archived_artifact,
            write_susu(artifact, false).expect("write artifact"),
        )
        .expect("write archived artifact");
        (archived_packet, archived_artifact)
    }

    fn assert_portable_memory_records(
        loaded_packet: &ReviewPacketStored,
        loaded_artifact: &ProjectAnalysis,
    ) {
        assert_eq!(loaded_packet.artifact.files.len(), 2);
        assert_eq!(loaded_packet.artifact.workflows.len(), 2);
        assert_eq!(loaded_packet.artifact.expectations.len(), 1);
        assert_eq!(loaded_packet.artifact.verifications.len(), 1);
        assert_eq!(loaded_packet.artifact.decisions.len(), 1);
        assert_eq!(loaded_packet.artifact.works.len(), 1);
        assert!(
            loaded_packet
                .source_previews
                .iter()
                .any(|preview| preview.file_id == "f_api"
                    && preview.path == "src/api.ts"
                    && preview.lines.iter().any(|line| !line.tokens.is_empty()))
        );
        assert!(
            loaded_packet
                .expectation_support
                .iter()
                .any(|support| support.expectation_id == "e_checkout_sequence"
                    && support.support_status == "verified")
        );
        assert_eq!(
            loaded_artifact.expectations,
            loaded_packet.artifact.expectations
        );
        assert_eq!(
            loaded_artifact.verifications,
            loaded_packet.artifact.verifications
        );
        assert_eq!(loaded_artifact.decisions, loaded_packet.artifact.decisions);
        assert_eq!(loaded_artifact.works, loaded_packet.artifact.works);
        assert!(
            review_portal_html(loaded_packet)
                .expect("render archived portal")
                .contains("Checkout reserves inventory before charging")
        );
    }

    #[test]
    fn review_packet_and_artifact_are_portable_project_memory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_root = temp.path().join("source");
        let archive_root = temp.path().join("archive");
        write_portable_memory_sources(&source_root);
        let artifact = portable_memory_artifact(&source_root);
        let (archived_packet, archived_artifact) = archive_project_memory(&artifact, &archive_root);

        fs::remove_dir_all(source_root.join("src")).expect("remove original source files");

        let loaded_packet = read_review_packet(&archived_packet).expect("read archived packet");
        let loaded_artifact =
            read_analysis_artifact(&archived_artifact).expect("read archived artifact");
        assert_portable_memory_records(&loaded_packet, &loaded_artifact);
    }

    fn stored_review_packet(
        input: &str,
        created: u64,
        artifact: &ProjectAnalysis,
    ) -> ReviewPacketStored {
        let check = check_report(artifact, false);
        let handoff = handoff_report(artifact, &check);
        let packet = review_packet(input.to_owned(), created, artifact, &check, &handoff);
        serde_json::from_value(serde_json::to_value(packet).expect("packet serializes"))
            .expect("packet deserializes")
    }

    #[test]
    fn readiness_json_summarizes_packet_queue() {
        let mut artifact = test_artifact();
        refresh_derived_analysis(&mut artifact);
        artifact.works.push(Work {
            id: "wk_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement checkout sequence".to_owned(),
            detail: "Checkout implementation touched the workflow and now needs verification."
                .to_owned(),
        });
        let packet = stored_review_packet("fixture.review.susu", 1, &artifact);

        let items = filtered_readiness_items(
            &packet.expectation_readiness,
            Some("needs_verification"),
            Some("checkout"),
        );
        let json = readiness_json(
            Path::new("fixture.review.susu"),
            &packet,
            &items,
            Some("needs_verification"),
            Some("checkout"),
        );

        assert_eq!(json["packet"], "fixture.review.susu");
        assert_eq!(json["total"], 1);
        assert_eq!(json["shown"], 1);
        assert_eq!(json["filters"]["bucket"], "needs_verification");
        assert_eq!(json["filters"]["search"], "checkout");
        assert_eq!(json["items"][0]["bucket"], "needs_verification");
        assert_eq!(
            json["counts"]
                .as_array()
                .expect("counts")
                .iter()
                .find(|item| item["bucket"] == "needs_verification")
                .expect("needs verification count")["count"],
            1
        );
    }

    #[test]
    fn readiness_filters_by_bucket_label_and_search() {
        let mut artifact = test_artifact();
        refresh_derived_analysis(&mut artifact);
        artifact.works.push(Work {
            id: "wk_checkout".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "agent:test".to_owned(),
            evidence: Some("commit:abc123".to_owned()),
            title: "Implement checkout sequence".to_owned(),
            detail: "Checkout implementation touched the workflow and now needs verification."
                .to_owned(),
        });
        let packet = stored_review_packet("fixture.review.susu", 1, &artifact);
        let bucket =
            canonical_readiness_bucket(Some("Has work, needs verification")).expect("bucket");

        let filtered =
            filtered_readiness_items(&packet.expectation_readiness, bucket, Some("checkout"));

        assert_eq!(bucket, Some("needs_verification"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].expectation_id, "e_checkout_sequence");
        assert!(canonical_readiness_bucket(Some("unknown")).is_err());
    }

    #[test]
    fn ci_workflow_uploads_and_publishes_susumu_review_artifacts() {
        let workflow = include_str!("../.github/workflows/ci.yml");

        assert!(workflow.contains("pull_request:"));
        assert!(workflow.contains("Susumu self-review packet"));
        assert!(workflow.contains("cargo run --locked -- review build ."));
        assert!(workflow.contains("--artifact-output \"$SUSUMU_ARTIFACT_DIR/project.susu\""));
        assert!(workflow.contains("--check-json \"$SUSUMU_ARTIFACT_DIR/check.json\""));
        assert!(workflow.contains("--output \"$SUSUMU_ARTIFACT_DIR/review.susu\""));
        assert!(workflow.contains("--html \"$SUSUMU_ARTIFACT_DIR/review.html\""));
        assert!(workflow.contains("Verify Susumu artifacts exist"));
        assert!(workflow.contains("actions/upload-artifact@v4"));
        assert!(workflow.contains("if-no-files-found: error"));
        assert!(workflow.contains("retention-days: 14"));
        assert!(workflow.contains("target/susumu-review/project.susu"));
        assert!(workflow.contains("target/susumu-review/check.json"));
        assert!(workflow.contains("target/susumu-review/review.susu"));
        assert!(workflow.contains("target/susumu-review/review.html"));
        assert!(workflow.contains("Publish Susumu portal"));
        assert!(
            workflow.contains("github.event_name == 'push' && github.ref == 'refs/heads/main'")
        );
        assert!(workflow.contains("pages: write"));
        assert!(workflow.contains("id-token: write"));
        assert!(workflow.contains("actions/download-artifact@v4"));
        assert!(workflow.contains("cp susumu-pages/review.html susumu-pages/index.html"));
        assert!(workflow.contains("actions/configure-pages@v5"));
        assert!(workflow.contains("actions/upload-pages-artifact@v3"));
        assert!(workflow.contains("actions/deploy-pages@v4"));
    }

    #[test]
    fn readme_links_to_live_susumu_pages_portal() {
        let readme = include_str!("../README.md");

        assert!(readme.contains("https://speroleague.github.io/susumu/"));
        assert!(readme.contains("View the live Susumu review portal"));
    }

    #[test]
    fn portal_config_parses_branding_section() {
        let config = parse_portal_config(
            r##"
            [ignored]
            accent = "#ffffff"

            [portal]
            title = "Acme Project Memory"
            background = "#101820"
            accent = "#abc"
            ok = "#a1b2c3" # muted success
            "##,
        )
        .expect("parse portal config");

        assert_eq!(config.title.as_deref(), Some("Acme Project Memory"));
        assert_eq!(
            config.css_vars.get("--bg").map(String::as_str),
            Some("#101820")
        );
        assert_eq!(
            config.css_vars.get("--accent").map(String::as_str),
            Some("#abc")
        );
        assert_eq!(
            config.css_vars.get("--ok").map(String::as_str),
            Some("#a1b2c3")
        );
        assert!(parse_portal_config("[portal]\naccent = \"red\"\n").is_err());
    }

    #[test]
    fn exported_review_html_loads_portal_config_from_project_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join(PORTAL_CONFIG_FILE),
            "[portal]\ntitle = \"Acme Memory\"\naccent = \"#445566\"\nbackground = \"#101820\"\n",
        )
        .expect("write portal config");
        let mut artifact = test_artifact();
        artifact.root = temp.path().display().to_string();
        refresh_derived_analysis(&mut artifact);
        let packet = stored_review_packet("fixture.review.susu", 1, &artifact);
        let packet_json = serde_json::to_string_pretty(&packet).expect("packet serializes");
        let packet_path = temp.path().join("fixture.review.susu");
        let html_path = temp.path().join("fixture-review.html");
        fs::write(&packet_path, packet_json).expect("write packet");

        export_review_html(&ReviewExportHtmlArgs {
            packet: packet_path,
            output: html_path.clone(),
        })
        .expect("export succeeds");

        let html = fs::read_to_string(html_path).expect("read html");
        assert!(html.contains("Acme Memory &middot; fixture"));
        assert!(html.contains("<div class=\"eyebrow\">Acme Memory</div>"));
        assert!(html.contains(":root{--accent:#445566;--bg:#101820}"));
    }

    #[test]
    fn review_portal_html_embeds_packet_safely() {
        let mut artifact = test_artifact();
        artifact.project_name = "fixture </script>".to_owned();
        refresh_derived_analysis(&mut artifact);
        let packet = stored_review_packet("fixture.review.susu", 1, &artifact);

        let html = review_portal_html(&packet).expect("portal renders");

        assert!(html.contains("Susumu Review"));
        assert!(html.contains("fixture &lt;/script&gt;"));
        assert!(html.contains("<\\/script>"));
        assert!(html.contains("Support summary"));
        assert!(html.contains("Evidence ladder"));
        assert!(html.contains("Expectation readiness board"));
        assert!(html.contains("expectation_readiness"));
        assert!(html.contains("Dirty and stale evidence"));
        assert!(html.contains("data-evidence-ladder"));
        assert!(html.contains("traceability-layout"));
        assert!(html.contains("traceability-list"));
        assert!(html.contains("traceability-detail"));
        assert!(html.contains("overscroll-behavior:contain"));
        assert!(html.contains("padding:8px 6px 0 0"));
        assert!(html.contains("Support reasons"));
        assert!(html.contains(".workflow-layout>*{min-width:0}"));
        assert!(html.contains(".detail-pane{position:sticky;top:98px;align-self:start;min-width:0;max-width:100%;overflow:hidden}"));
        assert!(html.contains("--accent:#9eb7a0"));
        assert!(html.contains("Record verification with susumu verify"));
        assert!(html.contains("POST /checkout"));
        assert!(html.contains("const packet = "));
    }

    #[test]
    fn review_source_previews_embed_syntax_tokens() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_dir = temp.path().join("src");
        fs::create_dir_all(&source_dir).expect("create source dir");
        fs::write(
            source_dir.join("api.ts"),
            "export function checkout() {\n  reserveInventory();\n  capturePayment();\n}\n",
        )
        .expect("write source");
        let mut artifact = test_artifact();
        artifact.root = temp.path().display().to_string();
        refresh_derived_analysis(&mut artifact);
        artifact.findings.push(susumu::model::Finding {
            rule_id: "SUS023".to_owned(),
            source: "susumu:derived".to_owned(),
            severity: Severity::Warning,
            title: "Verification evidence changed".to_owned(),
            detail: "Evidence changed near checkout.".to_owned(),
            file_id: Some("f_api".to_owned()),
            subject: Some("v_checkout".to_owned()),
            location: Some(Location {
                start_line: 2,
                start_column: 3,
                end_line: 2,
                end_column: 15,
            }),
        });

        let previews = crate::review_packet::review_source_previews(&artifact);

        assert!(previews.len() >= 2);
        assert!(previews.iter().any(|preview| preview.path == "src/api.ts"
            && preview.highlight_start == 2
            && preview.highlight_end == 2));
        assert!(
            previews
                .iter()
                .flat_map(|preview| &preview.lines)
                .any(|line| line.text.contains("checkout"))
        );
        assert!(
            previews
                .iter()
                .flat_map(|preview| &preview.lines)
                .any(|line| {
                    line.tokens
                        .iter()
                        .any(|token| token.text.contains("checkout"))
                })
        );
        assert!(
            previews
                .iter()
                .flat_map(|preview| &preview.lines)
                .flat_map(|line| &line.tokens)
                .all(|token| token.color.starts_with('#'))
        );
    }

    #[test]
    fn export_review_html_writes_standalone_portal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut artifact = test_artifact();
        artifact.root = temp
            .path()
            .join("project-without-config")
            .display()
            .to_string();
        refresh_derived_analysis(&mut artifact);
        let packet = stored_review_packet("fixture.review.susu", 1, &artifact);
        let packet_json = serde_json::to_string_pretty(&packet).expect("packet serializes");
        let packet_path = temp.path().join("fixture.review.susu");
        let html_path = temp.path().join("fixture-review.html");
        fs::write(&packet_path, packet_json).expect("write packet");

        export_review_html(&ReviewExportHtmlArgs {
            packet: packet_path,
            output: html_path.clone(),
        })
        .expect("export succeeds");

        let html = fs::read_to_string(html_path).expect("read html");
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("Susumu Review &middot; fixture"));
        assert!(html.contains("const packet = "));
        assert!(html.contains("POST /checkout"));
        assert!(html.contains("Workflow evidence"));
        assert!(html.contains("data-workflow-id"));
        assert!(html.contains("Linked expectations"));
        assert!(html.contains("Expectation readiness board"));
        assert!(html.contains("Has work, needs verification"));
        assert!(html.contains("Expectation traceability"));
        assert!(html.contains("Evidence ladder"));
        assert!(html.contains("Suggested next action"));
        assert!(html.contains("data-expectation-id"));
        assert!(html.contains("data-evidence-ladder"));
        assert!(html.contains("Dirty and stale evidence"));
        assert!(html.contains("Decisions on same target"));
        assert!(html.contains("workflow-layout traceability-layout"));
        assert!(html.contains("class=\"traceability-list\""));
        assert!(html.contains("detail-pane traceability-detail"));
        assert!(html.contains("max-width:100%;min-height:0;overflow:auto"));
        assert!(html.contains("--bg:#11131a"));
    }

    #[test]
    fn review_diff_detects_review_and_artifact_changes() {
        let mut old_artifact = test_artifact();
        refresh_derived_analysis(&mut old_artifact);
        let old = stored_review_packet("old.review.susu", 1, &old_artifact);

        let mut new_artifact = test_artifact();
        refresh_derived_analysis(&mut new_artifact);
        new_artifact.verifications.push(Verification {
            id: "v_checkout_failed".to_owned(),
            expectation_id: "e_checkout_sequence".to_owned(),
            status: VerificationStatus::Failed,
            supersedes: None,
            method: "manual review".to_owned(),
            source: "human:qa".to_owned(),
            evidence: Some("review:1".to_owned()),
            basis: None,
            detail: "Checkout order is not acceptable.".to_owned(),
        });
        refresh_derived_analysis(&mut new_artifact);
        let new = stored_review_packet("new.review.susu", 2, &new_artifact);

        let report = review_diff_report(&old, &new);

        assert!(review_diff_regressed(&old, &new));
        assert!(
            report
                .artifact
                .verifications
                .added
                .iter()
                .any(|item| item.contains("v_checkout_failed"))
        );
        assert!(
            report
                .review_items
                .added
                .iter()
                .any(|item| item.contains("failed verification"))
        );
        assert!(
            report
                .next_actions
                .added
                .iter()
                .any(|item| item.contains("failed verification"))
        );
        assert!(
            report
                .top_workflows
                .changed
                .iter()
                .any(|item| item.contains("w_checkout"))
        );
    }

    #[test]
    fn git_connect_marks_workflow_context_without_work_record_as_needs_record() {
        let artifact = test_artifact();
        let commit = test_commit("Touch checkout route", "", &["src/api.ts"]);

        let report = build_git_connect_report(&artifact, &[commit]);

        assert_eq!(report.connected, 0);
        assert_eq!(report.needs_record, 1);
        assert_eq!(report.unconnected, 0);
        assert_eq!(report.records[0].status, "needs_record");
        assert_eq!(report.records[0].workflows[0].id, "w_checkout");
        assert_eq!(report.records[0].expectations[0].id, "e_checkout_sequence");
        assert!(report.records[0].works.is_empty());
    }

    #[test]
    fn git_connect_marks_commit_evidence_work_as_connected() {
        let mut artifact = test_artifact();
        artifact.works.push(Work {
            id: "wk_git_f240cd96a07f2ea7".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "import:git".to_owned(),
            evidence: Some("commit:f240cd96a07f2ea7".to_owned()),
            title: "Address checkout sequence".to_owned(),
            detail: "Imported from Git.".to_owned(),
        });
        let commit = test_commit("Address checkout sequence", "", &["src/api.ts"]);

        let report = build_git_connect_report(&artifact, std::slice::from_ref(&commit));

        assert_eq!(report.connected, 1);
        assert_eq!(report.needs_record, 0);
        assert_eq!(report.unconnected, 0);
        assert_eq!(report.records[0].status, "connected");
        assert_eq!(report.records[0].works[0].id, "wk_git_f240cd96a07f2ea7");
    }

    #[test]
    fn git_shortcut_merges_work_sidecar_before_connecting() {
        let temp = tempfile::tempdir().expect("tempdir");
        let artifact_path = temp.path().join("project.susu");
        let work_path = temp.path().join("work.susu");
        let artifact = test_artifact();
        let commit = test_commit("Address checkout sequence", "", &["src/api.ts"]);
        let work = Work {
            id: "wk_git_f240cd96a07f2ea7".to_owned(),
            target: ExpectationTarget::Workflow,
            subject: Some("w_checkout".to_owned()),
            expectation_id: Some("e_checkout_sequence".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "human:git-link".to_owned(),
            evidence: Some(format!("commit:{}", commit.hash)),
            title: "Address checkout sequence".to_owned(),
            detail: "Linked from git shortcut sidecar.".to_owned(),
        };
        fs::write(
            &artifact_path,
            write_susu(&artifact, false).expect("write artifact text"),
        )
        .expect("write artifact");
        fs::write(
            &work_path,
            write_works(&[work], false).expect("write work text"),
        )
        .expect("write work");
        let args = GitShortcutArgs {
            repo: temp.path().to_path_buf(),
            artifact: artifact_path,
            output: work_path,
            since: None,
            until: None,
            limit: 25,
            max_items: 20,
            no_export: false,
            source: "import:git-connect".to_owned(),
            minify: false,
            json: false,
        };

        let loaded = git_shortcut_artifact(&args).expect("load shortcut artifact");
        let report = build_git_connect_report(&loaded, &[commit]);

        assert_eq!(report.connected, 1);
        assert_eq!(report.records[0].status, "connected");
        assert_eq!(report.records[0].works[0].id, "wk_git_f240cd96a07f2ea7");
    }

    #[test]
    fn git_connect_suggests_link_commands_for_unconnected_commits() {
        let mut artifact = test_artifact();
        artifact.expectations.push(Expectation {
            id: "e_docs_workflow".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Docs guide daily project review".to_owned(),
            detail: "Documentation should explain routine review commands for project work."
                .to_owned(),
        });
        artifact.expectations.push(Expectation {
            id: "e_docs_commands".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            status: ExpectationStatus::Accepted,
            source: "human:test".to_owned(),
            title: "Docs guide routine commands".to_owned(),
            detail: "Docs should explain local commands.".to_owned(),
        });
        let commit = test_commit("docs: update guide commands", "", &["README.md"]);

        let report = build_git_connect_report(&artifact, &[commit]);

        assert_eq!(report.records[0].status, "unconnected");
        assert!(
            report.records[0]
                .suggestions
                .iter()
                .any(|suggestion| suggestion.expectation_id == "e_docs_workflow")
        );
        assert!(
            report.records[0]
                .suggestions
                .iter()
                .any(|suggestion| suggestion.command == "susumu git link f240cd96 e_docs_workflow")
        );
    }

    #[test]
    fn suggested_expectations_are_ranked_and_limited() {
        let mut artifact = test_artifact();
        artifact.expectations = vec![
            Expectation {
                id: "e_docs".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Docs guide daily workflow commands".to_owned(),
                detail: "Docs should explain review commands and link commands.".to_owned(),
            },
            Expectation {
                id: "e_git".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Guide suggests next commands".to_owned(),
                detail: "Output should suggest workflow commands.".to_owned(),
            },
            Expectation {
                id: "e_portal".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "Daily status guide is visible".to_owned(),
                detail: "Review context and commands should stay visible.".to_owned(),
            },
            Expectation {
                id: "e_ai".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:test".to_owned(),
                title: "AI output remains optional".to_owned(),
                detail: "AI generated summaries should be optional.".to_owned(),
            },
        ];

        let suggestions = crate::git_connect::suggested_expectations(
            &artifact,
            "docs: guide daily workflow commands",
        );

        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].id, "e_docs");
        assert!(
            suggestions[0].score >= suggestions[1].score,
            "suggestions should be score-ranked"
        );
    }

    #[test]
    fn git_connect_export_uses_single_expectation_target() {
        let artifact = test_artifact();
        let commit = test_commit("Address e_checkout_sequence", "", &["notes.txt"]);
        let report = build_git_connect_report(&artifact, &[commit]);

        let works = works_from_git_connection(&artifact, &report.records[0], "import:test");
        let work = &works[0];

        assert_eq!(works.len(), 1);
        assert_eq!(work.id, "wk_git_f240cd96a07f2ea7");
        assert_eq!(work.target, ExpectationTarget::Workflow);
        assert_eq!(work.subject.as_deref(), Some("w_checkout"));
        assert_eq!(work.expectation_id.as_deref(), Some("e_checkout_sequence"));
        assert_eq!(
            work.evidence.as_deref(),
            Some("commit:f240cd96a07f2ea7b14cc1932c58914ed0871575")
        );
    }

    #[test]
    fn git_connect_export_links_project_expectation_from_language_match() {
        let mut artifact = test_artifact();
        artifact.expectations = vec![
            Expectation {
                id: "e_expectation_support".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:maintainer".to_owned(),
                title: "Expectations show supporting evidence".to_owned(),
                detail: "Review packets should show support for expectations.".to_owned(),
            },
            Expectation {
                id: "e_git_work_support".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:maintainer".to_owned(),
                title: "Git work can support project expectations".to_owned(),
                detail: "Local Git commits should become work support for project expectations."
                    .to_owned(),
            },
        ];
        let commit = test_commit(
            "feat: connect git work to project expectations",
            "",
            &["src/main.rs"],
        );
        let report = build_git_connect_report(&artifact, &[commit]);

        assert_eq!(report.needs_record, 1);
        assert_eq!(report.records[0].expectations[0].id, "e_git_work_support");

        let works = works_from_git_connection(&artifact, &report.records[0], "import:test");
        let work = &works[0];

        assert_eq!(works.len(), 1);
        assert_eq!(work.target, ExpectationTarget::Project);
        assert_eq!(work.subject, None);
        assert_eq!(work.expectation_id.as_deref(), Some("e_git_work_support"));
        assert_eq!(work.source, "import:test");
        assert_eq!(
            work.evidence.as_deref(),
            Some("commit:f240cd96a07f2ea7b14cc1932c58914ed0871575")
        );
        assert!(work.detail.contains("Generated by git connect."));

        artifact.works.extend(works);
        let support = expectation_support(&artifact);
        let git_support = support
            .iter()
            .find(|item| item.expectation_id == "e_git_work_support")
            .expect("git work support");

        assert_eq!(git_support.work, 1);
        assert_eq!(git_support.support_status, "partially_supported");
        assert!(
            git_support
                .reasons
                .iter()
                .any(|reason| reason == "1 linked work record(s)")
        );
    }

    #[test]
    fn git_connect_export_writes_work_for_each_matched_expectation() {
        let mut artifact = test_artifact();
        artifact.expectations = vec![
            Expectation {
                id: "e_readiness".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:maintainer".to_owned(),
                title: "Readiness board exists".to_owned(),
                detail: "Portal should show readiness.".to_owned(),
            },
            Expectation {
                id: "e_dirty_links".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:maintainer".to_owned(),
                title: "Dirty source links exist".to_owned(),
                detail: "Portal should show dirty source links.".to_owned(),
            },
        ];
        let commit = test_commit(
            "feat: support e_readiness and e_dirty_links",
            "",
            &["src/main.rs"],
        );
        let report = build_git_connect_report(&artifact, std::slice::from_ref(&commit));

        assert_eq!(report.needs_record, 1);
        assert_eq!(report.records[0].expectations.len(), 2);
        assert!(
            report.records[0]
                .reasons
                .iter()
                .any(|reason| reason == "2 expectation work record(s) missing")
        );

        let works = works_from_git_connection(&artifact, &report.records[0], "import:test");

        assert_eq!(works.len(), 2);
        assert_ne!(works[0].id, works[1].id);
        assert_eq!(works[0].target, ExpectationTarget::Project);
        assert_eq!(works[1].target, ExpectationTarget::Project);
        assert_eq!(works[0].expectation_id.as_deref(), Some("e_dirty_links"));
        assert_eq!(works[1].expectation_id.as_deref(), Some("e_readiness"));

        artifact.works.push(Work {
            id: "wk_existing_dirty".to_owned(),
            target: ExpectationTarget::Project,
            subject: None,
            expectation_id: Some("e_dirty_links".to_owned()),
            kind: WorkKind::Implementation,
            status: WorkStatus::Completed,
            source: "human:test".to_owned(),
            evidence: Some("commit:f240cd96a07f2ea7b14cc1932c58914ed0871575".to_owned()),
            title: "Existing dirty links work".to_owned(),
            detail: "Already connected.".to_owned(),
        });
        let report = build_git_connect_report(&artifact, &[commit]);
        let works = works_from_git_connection(&artifact, &report.records[0], "import:test");

        assert_eq!(report.needs_record, 1);
        assert_eq!(works.len(), 1);
        assert_eq!(works[0].expectation_id.as_deref(), Some("e_readiness"));
    }

    #[test]
    fn git_import_links_project_expectation_from_language_match() {
        let mut artifact = test_artifact();
        artifact.expectations = vec![
            Expectation {
                id: "e_expectation_support".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:maintainer".to_owned(),
                title: "Expectations show supporting evidence".to_owned(),
                detail: "Review packets should show support for expectations.".to_owned(),
            },
            Expectation {
                id: "e_git_work_support".to_owned(),
                target: ExpectationTarget::Project,
                subject: None,
                status: ExpectationStatus::Accepted,
                source: "human:maintainer".to_owned(),
                title: "Git work can support project expectations".to_owned(),
                detail: "Local Git commits should become work support for project expectations."
                    .to_owned(),
            },
        ];
        let commit = test_commit(
            "feat: connect git work to project expectations",
            "",
            &["src/main.rs"],
        );

        let linked = linked_git_expectation(&commit, Some(&artifact))
            .expect("language match should link exactly one expectation");

        assert_eq!(linked.id, "e_git_work_support");
        assert_eq!(linked.target, ExpectationTarget::Project);
        assert_eq!(linked.subject, None);

        let imported = imported_git_work(
            &commit,
            "import:test",
            &GitImportContext {
                target_depth: GitTargetDepth::Project,
                artifact: Some(&artifact),
            },
        );

        assert_eq!(
            imported.work.expectation_id.as_deref(),
            Some("e_git_work_support")
        );
        assert_eq!(imported.work.target, ExpectationTarget::Project);
        assert!(imported.targeting.contains("Linked exact expectation id"));
    }

    #[test]
    fn git_connect_export_uses_single_workflow_when_expectation_is_ambiguous() {
        let artifact = test_artifact();
        let commit = test_commit("Touch checkout route", "", &["src/api.ts"]);
        let report = build_git_connect_report(&artifact, &[commit]);

        let works = works_from_git_connection(&artifact, &report.records[0], "import:test");
        let work = &works[0];

        assert_eq!(work.target, ExpectationTarget::Workflow);
        assert_eq!(work.target, ExpectationTarget::Workflow);
        assert_eq!(work.subject.as_deref(), Some("w_checkout"));
        assert_eq!(work.expectation_id.as_deref(), Some("e_checkout_sequence"));
        assert!(work.detail.contains("Generated by git connect."));
        assert!(work.detail.contains("Changed files:"));
    }

    #[test]
    fn git_import_targets_single_workflow_from_changed_files() {
        let artifact = test_artifact();
        let commit = test_commit("Touch checkout route", "", &["src/routes.php"]);
        let context = GitImportContext {
            artifact: Some(&artifact),
            target_depth: GitTargetDepth::Workflow,
        };

        let target = git_work_target(&commit, &context);

        assert_eq!(target.target, ExpectationTarget::Workflow);
        assert_eq!(target.subject.as_deref(), Some("w_php_checkout"));
        assert_eq!(
            target.note,
            "Matched exactly one workflow from changed files."
        );
    }

    #[test]
    fn git_import_file_depth_targets_single_file_from_changed_files() {
        let artifact = test_artifact();
        let commit = test_commit("Touch api file", "", &["src/api.ts"]);
        let context = GitImportContext {
            artifact: Some(&artifact),
            target_depth: GitTargetDepth::File,
        };

        let target = git_work_target(&commit, &context);

        assert_eq!(target.target, ExpectationTarget::File);
        assert_eq!(target.subject.as_deref(), Some("f_api"));
        assert_eq!(
            target.note,
            "Matched exactly one artifact file from changed files."
        );
    }

    #[test]
    fn git_import_uses_exact_expectation_id_when_files_do_not_match() {
        let artifact = test_artifact();
        let commit = test_commit("Address e_checkout_sequence", "", &["notes.txt"]);
        let context = GitImportContext {
            artifact: Some(&artifact),
            target_depth: GitTargetDepth::Workflow,
        };

        let imported = imported_git_work(&commit, "import:git", &context);

        assert_eq!(imported.work.target, ExpectationTarget::Workflow);
        assert_eq!(imported.work.subject.as_deref(), Some("w_checkout"));
        assert_eq!(
            imported.work.expectation_id.as_deref(),
            Some("e_checkout_sequence")
        );
        assert!(imported.targeting.contains("used its target"));
    }

    #[test]
    fn git_import_json_report_includes_agent_friendly_fields() {
        let artifact = test_artifact();
        let commit = test_commit(
            "Address e_checkout_sequence",
            "Implementation detail.",
            &["notes.txt"],
        );
        let context = GitImportContext {
            artifact: Some(&artifact),
            target_depth: GitTargetDepth::Workflow,
        };
        let imported = vec![imported_git_work(&commit, "import:git", &context)];

        let report = build_git_import_json(Path::new("work.susu"), &imported);

        assert_eq!(report.output, "work.susu");
        assert_eq!(report.imported, 1);
        assert_eq!(report.records[0].id, "wk_git_f240cd96a07f2ea7");
        assert_eq!(
            report.records[0].commit,
            "f240cd96a07f2ea7b14cc1932c58914ed0871575"
        );
        assert_eq!(report.records[0].target, "workflow");
        assert_eq!(report.records[0].subject, Some("w_checkout"));
        assert_eq!(report.records[0].expectation, Some("e_checkout_sequence"));
        assert_eq!(report.records[0].changed_files, &["notes.txt".to_owned()]);
    }

    #[test]
    fn safe_snapshot_paths_stay_under_snapshot_root() {
        let path = safe_snapshot_path(Path::new("snapshot"), "./src\\api.ts").unwrap();

        assert_eq!(path, PathBuf::from("snapshot").join("src").join("api.ts"));
    }

    #[test]
    fn safe_snapshot_paths_reject_traversal_and_absolute_paths() {
        assert!(safe_snapshot_path(Path::new("snapshot"), "../secret.rs").is_err());
        assert!(safe_snapshot_path(Path::new("snapshot"), "/secret.rs").is_err());
        assert!(safe_snapshot_path(Path::new("snapshot"), "C:/secret.rs").is_err());
        assert!(safe_snapshot_path(Path::new("snapshot"), "c:/secret.rs").is_err());
    }

    #[test]
    fn file_expectation_paths_resolve_to_scanner_ids() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("main.rs"), "fn main() {}\n").expect("write source");

        let subject = resolve_file_subject(
            temp.path(),
            ExpectationTarget::File,
            Some(".\\main.rs".to_owned()),
        )
        .expect("resolve file path");

        assert_eq!(subject, Some("f_a4075800b4a04993".to_owned()));
    }

    #[test]
    fn review_shortcut_accepts_output_shorthand() {
        let cli = Cli::try_parse_from(["susumu", "review", "-o", "build/review"])
            .expect("parse review output shorthand");
        let Command::Review {
            args,
            command: None,
        } = cli.command.expect("review command")
        else {
            panic!("expected review shortcut");
        };
        assert_eq!(args.output_dir, PathBuf::from("build/review"));
    }
}
