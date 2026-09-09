use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    /// Repository directory to initialize.
    #[arg(default_value = ".")]
    pub(crate) target: PathBuf,

    /// Expectations sidecar to create. Relative paths are resolved under the target directory.
    #[arg(short, long, default_value = "expectations.susu")]
    pub(crate) file: PathBuf,

    /// Project name to use in starter expectation text.
    #[arg(long)]
    pub(crate) name: Option<String>,

    /// Provenance label for the starter expectations.
    #[arg(long, default_value = "human:maintainer")]
    pub(crate) source: String,

    /// Overwrite an existing sidecar.
    #[arg(long)]
    pub(crate) force: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Directory to scan, or an existing .susu artifact to check.
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
pub(crate) struct MigrateArgs {
    /// Older artifact containing the source identities to compare.
    pub(crate) old: PathBuf,

    /// Newer artifact containing the current source identities and records.
    pub(crate) new: PathBuf,

    /// Explicitly accept a mapping in `OLD_ID=NEW_ID` form. Repeat for multiple mappings.
    #[arg(long = "accept", value_name = "OLD_ID=NEW_ID")]
    pub(crate) accepts: Vec<String>,

    /// Explicitly reject a migration candidate by its old source id.
    #[arg(long = "reject", value_name = "OLD_ID")]
    pub(crate) rejects: Vec<String>,

    /// Explicitly defer a migration candidate by its old source id.
    #[arg(long = "defer", value_name = "OLD_ID")]
    pub(crate) defers: Vec<String>,

    /// Write the resolved analysis artifact after accepted mappings are applied.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Optional expectations sidecar to update when mappings are accepted.
    #[arg(long)]
    pub(crate) expectations: Option<PathBuf>,

    /// Optional decisions sidecar to update when mappings are accepted.
    #[arg(long)]
    pub(crate) decisions: Option<PathBuf>,

    /// Optional work sidecar to update and append the migration audit record.
    #[arg(long)]
    pub(crate) work: Option<PathBuf>,

    /// Optional review-thread sidecar to update when source anchors are accepted.
    #[arg(long)]
    pub(crate) reviews: Option<PathBuf>,

    /// Emit machine-readable migration candidates and dispositions.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct DiffArgs {
    /// Older .susu artifact.
    pub(crate) old: PathBuf,

    /// Newer .susu artifact.
    pub(crate) new: PathBuf,

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

#[derive(Debug, Args)]
pub(crate) struct HandoffArgs {
    /// Directory to scan, or an existing .susu artifact to summarize.
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

    /// Maximum items to print per section.
    #[arg(long, default_value_t = 8)]
    pub(crate) max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewCommand {
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
pub(crate) struct ReviewShortcutArgs {
    /// Directory to scan, or an existing .susu artifact to package.
    #[arg(default_value = ".")]
    pub(crate) target: PathBuf,

    /// Directory for convention-based Susumu outputs.
    #[arg(short = 'o', long, default_value = ".susumu", value_name = "DIR")]
    pub(crate) output_dir: PathBuf,

    /// Merge work records from a .susu artifact or work-only fragment.
    #[arg(long, value_name = "FILE")]
    pub(crate) work: Option<PathBuf>,

    /// Fail the embedded check result on warnings as well as critical items.
    #[arg(long)]
    pub(crate) strict: bool,

    /// Exit nonzero after writing outputs if the check result failed.
    #[arg(long)]
    pub(crate) fail_on_check: bool,

    /// Skip writing the standalone HTML portal.
    #[arg(long)]
    pub(crate) no_html: bool,

    /// Serve the built review packet as a local web portal after writing outputs.
    #[arg(long)]
    pub(crate) serve: bool,

    /// Host interface to bind when --serve is used.
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Port to bind when --serve is used. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    pub(crate) port: u16,

    /// Emit a machine-readable build summary.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ReviewBuildArgs {
    /// Directory to scan, or an existing .susu artifact to package.
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

    /// Write the generated .susu artifact to this file.
    #[arg(long, default_value = "target/susumu.susu", value_name = "FILE")]
    pub(crate) artifact_output: PathBuf,

    /// Write the review packet to this file.
    #[arg(
        short,
        long,
        default_value = "target/susumu.review.susu",
        value_name = "FILE"
    )]
    pub(crate) output: PathBuf,

    /// Optionally write the machine-readable check report JSON.
    #[arg(long, value_name = "FILE")]
    pub(crate) check_json: Option<PathBuf>,

    /// Optionally export the review portal as standalone HTML.
    #[arg(long, value_name = "FILE")]
    pub(crate) html: Option<PathBuf>,

    /// Fail the embedded check result on warnings as well as critical items.
    #[arg(long)]
    pub(crate) strict: bool,

    /// Exit nonzero after writing outputs if the check result failed.
    #[arg(long)]
    pub(crate) fail_on_check: bool,

    /// Emit a machine-readable build summary.
    #[arg(long)]
    pub(crate) json: bool,

    /// Serve the built review packet as a local web portal after writing outputs.
    #[arg(long)]
    pub(crate) serve: bool,

    /// Host interface to bind when --serve is used.
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Port to bind when --serve is used. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    pub(crate) port: u16,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewCreateArgs {
    /// Directory to scan, or an existing .susu artifact to package.
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

    /// Fail the embedded check result on warnings as well as critical items.
    #[arg(long)]
    pub(crate) strict: bool,

    /// Write the review packet to this file. If omitted, the packet is printed.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: Option<PathBuf>,

    /// Print the full review packet JSON even when --output is supplied.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewOpenArgs {
    /// Review packet created by `susumu review create`.
    pub(crate) packet: PathBuf,

    /// Maximum items to print per section.
    #[arg(long, default_value_t = 8)]
    pub(crate) max_items: usize,

    /// Emit the stored review packet JSON.
    #[arg(long)]
    pub(crate) json: bool,

    /// Open the embedded artifact in the Susumu TUI.
    #[arg(long)]
    pub(crate) tui: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewDiffArgs {
    /// Older review packet.
    pub(crate) old: PathBuf,

    /// Newer review packet.
    pub(crate) new: PathBuf,

    /// Maximum items to print per section.
    #[arg(long, default_value_t = 8)]
    pub(crate) max_items: usize,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,

    /// Exit nonzero when the newer packet has more critical items or newly fails.
    #[arg(long)]
    pub(crate) fail_on_regression: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewServeArgs {
    /// Review packet created by `susumu review create`.
    pub(crate) packet: PathBuf,

    /// Host interface to bind. Defaults to localhost.
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Port to bind. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    pub(crate) port: u16,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewExportHtmlArgs {
    /// Review packet created by `susumu review create`.
    pub(crate) packet: PathBuf,

    /// HTML file to write.
    #[arg(short, long, value_name = "FILE")]
    pub(crate) output: PathBuf,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args)]
pub(crate) struct OpenArgs {
    /// Review packet whose sibling review.html file should be opened.
    #[arg(default_value = ".susumu/review.susu")]
    pub(crate) packet: PathBuf,

    /// Host interface to bind.
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,

    /// Port to bind. Use 0 to ask the OS for an available port.
    #[arg(long, default_value_t = 7878)]
    pub(crate) port: u16,

    /// Serve the packet locally instead of opening its static HTML export.
    #[arg(long)]
    pub(crate) serve: bool,

    /// Print the review summary instead of opening the portal.
    #[arg(long)]
    pub(crate) summary: bool,

    /// Open the embedded artifact in the Susumu TUI instead of opening the portal.
    #[arg(long)]
    pub(crate) tui: bool,

    /// Maximum items to print when --summary is used.
    #[arg(long, default_value_t = 8)]
    pub(crate) max_items: usize,

    /// Emit the stored review packet JSON instead of opening the portal.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AttestationCommand {
    /// Parse and structurally inspect an attestation envelope.
    Inspect(InspectAttestationArgs),
}

#[derive(Debug, Args)]
pub(crate) struct InspectAttestationArgs {
    /// JSON attestation envelope to inspect.
    #[arg(short, long)]
    pub(crate) file: PathBuf,

    /// Emit machine-readable JSON.
    #[arg(long)]
    pub(crate) json: bool,
}
