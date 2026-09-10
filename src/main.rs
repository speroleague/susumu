use std::{
    collections::BTreeMap,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{self, Command as ProcessCommand},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Serialize;
use sha2::{Digest, Sha256};
use susumu::{
    analysis::{
        anchor_decision_bases, anchor_verification_bases, current_basis_for_decision,
        current_basis_for_verification, refresh_derived_analysis,
    },
    migration::{source_migration_findings, source_migrations},
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
mod migration_commands;
mod review;

#[allow(clippy::wildcard_imports)]
pub(crate) use cli::args::*;
#[allow(clippy::wildcard_imports)]
use cli::commands::*;
#[allow(clippy::wildcard_imports)]
use cli::daily::*;
use cli::daily_options::{ExpectationsArgs, ResolveArgs, StatusArgs, VerifyArgs};
use cli::dispatch::run_command;
use cli::loading::{load_analysis, load_for_stamp, stamp_decision, stamp_verification};
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
use git::staleness::{DEFAULT_HISTORY_LIMIT, enrich_stale_findings};
use git::types::{
    GitCommit, GitConnectExport, GitExpectationLink, GitImportContext, GitImportJson,
    GitImportRecordJson, GitWorkTarget, ImportedGitWork,
};
use review::checks::{
    check_item_jsons, check_json, check_report, print_check_json, print_check_report,
};
use review::digest::{self as digest_command, DigestArgs};
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
