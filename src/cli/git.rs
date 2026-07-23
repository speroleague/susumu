use std::path::PathBuf;

use clap::Args;

use crate::cli::values::{GitTargetDepthArg, WorkKindArg, WorkStatusArg};

#[derive(Debug, clap::Subcommand)]
pub(crate) enum GitCommand {
    /// Connect commits to workflows, records, and expectations.
    Connect(GitConnectArgs),

    /// Explicitly link one commit to one expectation as a work record.
    Link(GitLinkArgs),

    /// Import commits as work records.
    Import(GitImportArgs),

    /// Compare the current artifact to code evidence from an older Git ref.
    Rewind(GitRewindArgs),

    /// Inspect Git commit signature identity and integrity.
    Signature(GitSignatureArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GitSignatureArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,

    /// Commit to inspect.
    #[arg(long)]
    pub(crate) commit: String,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GitConnectArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,

    /// Current .susu artifact to connect against.
    #[arg(long)]
    pub(crate) artifact: PathBuf,

    /// Starting revision or ref, such as main or HEAD~10.
    #[arg(long)]
    pub(crate) since: Option<String>,

    /// Ending revision or ref. Defaults to HEAD when --since is used.
    #[arg(long)]
    pub(crate) until: Option<String>,

    /// Maximum number of commits to inspect.
    #[arg(long)]
    pub(crate) limit: Option<usize>,

    /// Maximum commit connections to print.
    #[arg(long, default_value_t = 20)]
    pub(crate) max_items: usize,

    /// Write work records for commits marked `needs_record`.
    #[arg(long)]
    pub(crate) export_work: Option<PathBuf>,

    /// Provenance label for exported work records.
    #[arg(long, default_value = "import:git-connect")]
    pub(crate) source: String,

    /// Emit compact .susu syntax when exporting work.
    #[arg(long)]
    pub(crate) minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GitShortcutArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,

    /// Current .susu artifact to connect against.
    #[arg(long, default_value = ".susumu/project.susu")]
    pub(crate) artifact: PathBuf,

    /// Work sidecar to update with connected commits.
    #[arg(short, long, default_value = ".susumu/work.susu")]
    pub(crate) output: PathBuf,

    /// Starting revision or ref, such as main or HEAD~10.
    #[arg(long)]
    pub(crate) since: Option<String>,

    /// Ending revision or ref. Defaults to HEAD when --since is used.
    #[arg(long)]
    pub(crate) until: Option<String>,

    /// Maximum number of commits to inspect.
    #[arg(long, default_value_t = 25)]
    pub(crate) limit: usize,

    /// Maximum commit connections to print.
    #[arg(long, default_value_t = 20)]
    pub(crate) max_items: usize,

    /// Do not write work records; only print the connections.
    #[arg(long)]
    pub(crate) no_export: bool,

    /// Provenance label for exported work records.
    #[arg(long, default_value = "import:git-connect")]
    pub(crate) source: String,

    /// Emit compact .susu syntax when exporting work.
    #[arg(long)]
    pub(crate) minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GitLinkArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,

    /// Current .susu artifact containing the expectation.
    #[arg(long, default_value = ".susumu/project.susu")]
    pub(crate) artifact: PathBuf,

    /// Work sidecar to update.
    #[arg(short, long, default_value = ".susumu/work.susu")]
    pub(crate) output: PathBuf,

    /// Commit hash or ref to link.
    pub(crate) commit: String,

    /// Expectation id this commit supports.
    pub(crate) expectation: String,

    /// Provenance label for the linked work record.
    #[arg(long, default_value = "human:git-link")]
    pub(crate) source: String,

    /// Kind: implementation, verification, documentation, infrastructure, review, or other.
    #[arg(long, default_value = "implementation")]
    pub(crate) kind: WorkKindArg,

    /// Status: proposed, `in_progress`, completed, blocked, or superseded.
    #[arg(long, default_value = "completed")]
    pub(crate) status: WorkStatusArg,

    /// Override the work record title. Defaults to the commit subject.
    #[arg(long)]
    pub(crate) title: Option<String>,

    /// Add a note to the generated work detail.
    #[arg(long)]
    pub(crate) detail: Option<String>,

    /// Emit compact .susu syntax.
    #[arg(long)]
    pub(crate) minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GitImportArgs {
    /// Git repository to read.
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,

    /// Work sidecar to update.
    #[arg(short, long, default_value = "work.susu")]
    pub(crate) output: PathBuf,

    /// Optional .susu artifact used to map changed files to evidence ids.
    #[arg(long)]
    pub(crate) artifact: Option<PathBuf>,

    /// How far imported commits should be targeted: project, file, or workflow.
    #[arg(long, default_value = "file")]
    pub(crate) target_depth: GitTargetDepthArg,

    /// Starting revision or ref, such as main or HEAD~10.
    #[arg(long)]
    pub(crate) since: Option<String>,

    /// Ending revision or ref. Defaults to HEAD when --since is used.
    #[arg(long)]
    pub(crate) until: Option<String>,

    /// Maximum number of commits to import.
    #[arg(long)]
    pub(crate) limit: Option<usize>,

    /// Provenance label for imported work records.
    #[arg(long, default_value = "import:git")]
    pub(crate) source: String,

    /// Emit compact .susu syntax.
    #[arg(long)]
    pub(crate) minify: bool,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GitRewindArgs {
    /// Git repository to inspect.
    #[arg(long, default_value = ".")]
    pub(crate) repo: PathBuf,

    /// Older revision or ref to scan, such as HEAD~1 or main.
    #[arg(long)]
    pub(crate) from: String,

    /// Current .susu artifact to compare against. If omitted, scan the repository now.
    #[arg(long)]
    pub(crate) artifact: Option<PathBuf>,

    /// Optionally write the generated old-ref artifact for inspection.
    #[arg(long)]
    pub(crate) old_output: Option<PathBuf>,

    /// Emit compact .susu syntax when writing --old-output.
    #[arg(long)]
    pub(crate) minify: bool,

    /// Exit nonzero when stale verification or decision evidence is present.
    #[arg(long)]
    pub(crate) fail_on_stale: bool,

    /// Maximum changed items to print per section.
    #[arg(long, default_value_t = 10)]
    pub(crate) max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}
