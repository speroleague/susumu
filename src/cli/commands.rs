use crate::*;

#[derive(Debug, Parser)]
#[command(
    name = "susumu",
    version,
    about = "Make a codebase's workflows visible"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// Directory to scan, or an existing .susu artifact to open.
    #[arg(default_value = ".")]
    pub(crate) target: PathBuf,

    /// Write the analysis to this .susu file.
    #[arg(short, long)]
    pub(crate) output: Option<PathBuf>,

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

    /// Emit compact .susu syntax.
    #[arg(long)]
    pub(crate) minify: bool,

    /// Scan or load without opening the terminal interface.
    #[arg(long)]
    pub(crate) headless: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create a starter expectations sidecar for a repository.
    Init(InitArgs),

    /// Check an artifact or project for review blockers.
    Check(CheckArgs),

    /// Compare two .susu artifacts.
    Diff(DiffArgs),

    /// Review and explicitly resolve source migration candidates.
    Migrate(MigrateArgs),

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

    /// Inspect a provider-neutral attestation envelope without trusting it.
    Attestation {
        #[command(subcommand)]
        command: AttestationCommand,
    },

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

    /// Author review threads and ownership records.
    ReviewThread {
        #[command(subcommand)]
        command: ReviewThreadCommand,
    },

    /// Connect local Git history to Susumu work, or use advanced Git subcommands.
    Git {
        #[command(flatten)]
        args: GitShortcutArgs,

        #[command(subcommand)]
        command: Option<GitCommand>,
    },
}
