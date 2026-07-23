use std::path::PathBuf;

use clap::Args;

use super::values::ExpectationStatusArg;

#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
    /// Directory to scan, or an existing .susu artifact to check.
    #[arg(default_value = ".")]
    pub(crate) target: PathBuf,
    /// Directory for convention-based Susumu outputs.
    #[arg(long, default_value = ".susumu", value_name = "DIR")]
    pub(crate) output_dir: PathBuf,
    /// Fail on warnings as well as critical items.
    #[arg(long)]
    pub(crate) strict: bool,
    /// Maximum review items to print.
    #[arg(long, default_value_t = 10)]
    pub(crate) max_items: usize,
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ResolveArgs {
    /// Source path to resolve, relative to the project root.
    pub(crate) path: PathBuf,
    /// Project directory to scan.
    #[arg(long, default_value = ".")]
    pub(crate) target: PathBuf,
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ExpectationsArgs {
    /// Directory to scan, or an existing .susu artifact to inspect.
    #[arg(default_value = ".")]
    pub(crate) target: PathBuf,
    /// Read expectations from a specific sidecar or artifact instead of scanning/loading target.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) file: Option<PathBuf>,
    /// Search expectation id, title, detail, source, target, subject, or support status.
    #[arg(short, long)]
    pub(crate) search: Option<String>,
    /// Filter by expectation status: proposed, accepted, or superseded.
    #[arg(long)]
    pub(crate) status: Option<ExpectationStatusArg>,
    /// Maximum expectations to print.
    #[arg(long, default_value_t = 50)]
    pub(crate) max_items: usize,
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct VerifyArgs {
    /// Expectation id being checked.
    pub(crate) expectation: String,
    /// Directory or artifact used to validate the expectation id.
    #[arg(long, default_value = ".")]
    pub(crate) target: PathBuf,
    /// Verification sidecar to update.
    #[arg(short, long, default_value = "verifications.susu")]
    pub(crate) file: PathBuf,
    /// Optional explicit id. Omit to derive a stable id from the record.
    #[arg(long)]
    pub(crate) id: Option<String>,
    /// Verification id this record supersedes.
    #[arg(long)]
    pub(crate) supersedes: Option<String>,
    /// Mark the verification as passed.
    #[arg(long, conflicts_with_all = ["failed", "inconclusive"])]
    pub(crate) passed: bool,
    /// Mark the verification as failed.
    #[arg(long, conflicts_with_all = ["passed", "inconclusive"])]
    pub(crate) failed: bool,
    /// Mark the verification as inconclusive.
    #[arg(long, conflicts_with_all = ["passed", "failed"])]
    pub(crate) inconclusive: bool,
    /// Method used to check the expectation.
    #[arg(long)]
    pub(crate) method: String,
    /// Provenance label such as human:engineer or ci:github-actions.
    #[arg(long, default_value = "human:local")]
    pub(crate) source: String,
    /// Optional evidence id or external evidence reference.
    #[arg(long)]
    pub(crate) evidence: Option<String>,
    /// Local evidence artifact to hash as sha256:<digest>. The file is not copied into the record.
    #[arg(long, conflicts_with = "evidence")]
    pub(crate) evidence_file: Option<PathBuf>,
    /// JSON execution metadata to record without authenticating its claims.
    #[arg(long)]
    pub(crate) execution_file: Option<PathBuf>,
    /// Optional evidence fingerprint this verification was based on.
    #[arg(long)]
    pub(crate) basis: Option<String>,
    /// Verification detail. Defaults to a generated summary.
    #[arg(long)]
    pub(crate) detail: Option<String>,
    /// Emit compact .susu syntax.
    #[arg(long)]
    pub(crate) minify: bool,
    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}
