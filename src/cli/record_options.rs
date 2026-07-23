use std::path::PathBuf;

use clap::{Args, Subcommand};

use super::values::{
    DecisionStatusArg, ExpectationStatusArg, ExpectationTargetArg, VerificationStatusArg,
    WorkKindArg, WorkStatusArg,
};

#[derive(Debug, Subcommand)]
pub(crate) enum ExpectationCommand {
    /// Add or replace one expectation in an expectation-only sidecar.
    Add(AddExpectation),
    /// List expectations from a sidecar or artifact.
    List(ListExpectations),
    /// Remove one expectation from an expectation-only sidecar.
    Remove(RemoveExpectation),
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum VerificationCommand {
    /// Add or replace one verification in a verification-only sidecar.
    Add(AddVerification),
    /// List verifications from a sidecar or artifact.
    List(ListVerifications),
    /// Remove one verification from a verification-only sidecar.
    Remove(RemoveVerification),
    /// Inspect or initialize the verification hash chain.
    Chain(ChainVerificationArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ChainVerificationArgs {
    /// Verification sidecar to inspect or initialize.
    #[arg(short, long, default_value = "verifications.susu")]
    pub(crate) file: PathBuf,
    /// Add chain hashes to an unchained sidecar.
    #[arg(long)]
    pub(crate) initialize: bool,
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DecisionCommand {
    /// Add or replace one decision in a decision-only sidecar.
    Add(AddDecision),
    /// List decisions from a sidecar or artifact.
    List(ListDecisions),
    /// Remove one decision from a decision-only sidecar.
    Remove(RemoveDecision),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkCommand {
    /// Add or replace one work record in a work-only sidecar.
    Add(AddWork),
    /// List work records from a sidecar or artifact.
    List(ListWorks),
    /// Remove one work record from a work-only sidecar.
    Remove(RemoveWork),
}

#[derive(Debug, Args)]
pub(crate) struct AddExpectation {
    /// Expectation sidecar to update.
    #[arg(short, long, default_value = "expectations.susu")]
    pub(crate) file: PathBuf,
    #[arg(long)]
    pub(crate) id: Option<String>,
    #[arg(long)]
    pub(crate) target: ExpectationTargetArg,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long, default_value = ".")]
    pub(crate) target_root: PathBuf,
    #[arg(long, default_value = "proposed")]
    pub(crate) status: ExpectationStatusArg,
    #[arg(long, default_value = "human:local")]
    pub(crate) source: String,
    #[arg(long)]
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) detail: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListExpectations {
    /// Expectation sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "expectations.susu")]
    pub(crate) file: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct RemoveExpectation {
    /// Expectation sidecar to update.
    #[arg(short, long, default_value = "expectations.susu")]
    pub(crate) file: PathBuf,
    pub(crate) id: String,
}

#[derive(Debug, Args)]
pub(crate) struct AddVerification {
    /// Verification sidecar to update.
    #[arg(short, long, default_value = "verifications.susu")]
    pub(crate) file: PathBuf,
    #[arg(long)]
    pub(crate) id: Option<String>,
    #[arg(long)]
    pub(crate) supersedes: Option<String>,
    #[arg(long)]
    pub(crate) expectation: String,
    #[arg(long)]
    pub(crate) status: VerificationStatusArg,
    #[arg(long)]
    pub(crate) method: String,
    #[arg(long, default_value = "human:local")]
    pub(crate) source: String,
    #[arg(long)]
    pub(crate) evidence: Option<String>,
    #[arg(long, conflicts_with = "evidence")]
    pub(crate) evidence_file: Option<PathBuf>,
    #[arg(long)]
    pub(crate) execution_file: Option<PathBuf>,
    #[arg(long)]
    pub(crate) basis: Option<String>,
    #[arg(long)]
    pub(crate) detail: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListVerifications {
    /// Verification sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "verifications.susu")]
    pub(crate) file: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct RemoveVerification {
    /// Verification sidecar to update.
    #[arg(short, long, default_value = "verifications.susu")]
    pub(crate) file: PathBuf,
    pub(crate) id: String,
}

#[derive(Debug, Args)]
pub(crate) struct AddDecision {
    /// Decision sidecar to update.
    #[arg(short, long, default_value = "decisions.susu")]
    pub(crate) file: PathBuf,
    #[arg(long)]
    pub(crate) id: Option<String>,
    #[arg(long)]
    pub(crate) target: ExpectationTargetArg,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long, default_value = "proposed")]
    pub(crate) status: DecisionStatusArg,
    #[arg(long, default_value = "human:local")]
    pub(crate) source: String,
    #[arg(long)]
    pub(crate) basis: Option<String>,
    #[arg(long)]
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) detail: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListDecisions {
    /// Decision sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "decisions.susu")]
    pub(crate) file: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct RemoveDecision {
    /// Decision sidecar to update.
    #[arg(short, long, default_value = "decisions.susu")]
    pub(crate) file: PathBuf,
    pub(crate) id: String,
}

#[derive(Debug, Args)]
pub(crate) struct AddWork {
    /// Work sidecar to update.
    #[arg(short, long, default_value = "work.susu")]
    pub(crate) file: PathBuf,
    #[arg(long)]
    pub(crate) id: Option<String>,
    #[arg(long)]
    pub(crate) target: ExpectationTargetArg,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long)]
    pub(crate) expectation: Option<String>,
    #[arg(long, default_value = "implementation")]
    pub(crate) kind: WorkKindArg,
    #[arg(long, default_value = "completed")]
    pub(crate) status: WorkStatusArg,
    #[arg(long, default_value = "human:local")]
    pub(crate) source: String,
    #[arg(long)]
    pub(crate) evidence: Option<String>,
    #[arg(long)]
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) detail: String,
}

#[derive(Debug, Args)]
pub(crate) struct ListWorks {
    /// Work sidecar or full .susu artifact to read.
    #[arg(short, long, default_value = "work.susu")]
    pub(crate) file: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct RemoveWork {
    /// Work sidecar to update.
    #[arg(short, long, default_value = "work.susu")]
    pub(crate) file: PathBuf,
    pub(crate) id: String,
}
