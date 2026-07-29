use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectAnalysis {
    pub schema_version: u32,
    pub project_name: String,
    pub root: String,
    pub generated_unix_seconds: u64,
    /// The resolved source-control revision used for this scan, when the project is in Git.
    #[serde(default)]
    pub source_revision: Option<String>,
    pub files: Vec<SourceFile>,
    pub symbols: Vec<Symbol>,
    pub dependencies: Vec<Dependency>,
    pub workflows: Vec<Workflow>,
    pub workflow_priorities: Vec<WorkflowPriority>,
    pub flows: Vec<FlowEdge>,
    pub expectations: Vec<Expectation>,
    pub verifications: Vec<Verification>,
    pub decisions: Vec<Decision>,
    pub works: Vec<Work>,
    #[serde(default)]
    pub review_threads: Vec<ReviewThread>,
    pub findings: Vec<Finding>,
}

impl ProjectAnalysis {
    #[must_use]
    pub fn language_counts(&self) -> BTreeMap<Language, usize> {
        let mut counts = BTreeMap::new();
        for file in &self.files {
            *counts.entry(file.language).or_default() += 1;
        }
        counts
    }

    #[must_use]
    pub fn resolved_flow_count(&self) -> usize {
        self.flows.iter().filter(|flow| flow.to.is_some()).count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceFile {
    pub id: String,
    pub path: String,
    pub language: Language,
    pub lines: usize,
    pub bytes: u64,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Php,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Vue,
}

impl Language {
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "php" => Some(Self::Php),
            "py" | "pyi" => Some(Self::Python),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "vue" => Some(Self::Vue),
            _ => None,
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Rust => "rust",
            Self::Php => "php",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Vue => "vue",
        };
        formatter.write_str(value)
    }
}

impl std::str::FromStr for Language {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "rust" => Ok(Self::Rust),
            "php" => Ok(Self::Php),
            "python" => Ok(Self::Python),
            "javascript" => Ok(Self::JavaScript),
            "typescript" => Ok(Self::TypeScript),
            "tsx" => Ok(Self::Tsx),
            "vue" => Ok(Self::Vue),
            _ => Err(format!("unknown language: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub kind: SymbolKind,
    pub file_id: String,
    /// Hash of the parsed symbol region when the scanner could determine it.
    /// Older artifacts may omit this and use the file-level fallback.
    pub content_hash: Option<String>,
    pub location: Location,
    pub entrypoint: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Function => "function",
            Self::Method => "method",
        })
    }
}

impl std::str::FromStr for SymbolKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "function" => Ok(Self::Function),
            "method" => Ok(Self::Method),
            _ => Err(format!("unknown symbol kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependency {
    pub file_id: String,
    pub name: String,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workflow {
    pub id: String,
    pub kind: WorkflowKind,
    pub framework: String,
    pub trigger: String,
    pub handler: Option<String>,
    pub entry_symbol: Option<String>,
    pub file_id: String,
    pub confidence: Confidence,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowPriority {
    pub workflow_id: String,
    pub source: String,
    pub score: u32,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowKind {
    Http,
}

impl fmt::Display for WorkflowKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Http => "http",
        })
    }
}

impl std::str::FromStr for WorkflowKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "http" => Ok(Self::Http),
            _ => Err(format!("unknown workflow kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowEdge {
    pub from: String,
    pub to: Option<String>,
    pub call: String,
    pub confidence: Confidence,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Expectation {
    pub id: String,
    pub target: ExpectationTarget,
    pub subject: Option<String>,
    pub status: ExpectationStatus,
    pub source: String,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Verification {
    pub id: String,
    pub expectation_id: String,
    pub status: VerificationStatus,
    pub supersedes: Option<String>,
    pub execution: Option<VerificationExecution>,
    pub chain: Option<String>,
    pub method: String,
    pub source: String,
    pub evidence: Option<String>,
    pub basis: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationExecution {
    pub result: String,
    pub exit_code: Option<i32>,
    pub run_id: Option<String>,
    pub issued_at: Option<String>,
    pub artifact_manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Decision {
    pub id: String,
    pub target: ExpectationTarget,
    pub subject: Option<String>,
    pub status: DecisionStatus,
    pub source: String,
    pub basis: Option<String>,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Work {
    pub id: String,
    pub target: ExpectationTarget,
    pub subject: Option<String>,
    pub expectation_id: Option<String>,
    pub kind: WorkKind,
    pub status: WorkStatus,
    pub source: String,
    pub evidence: Option<String>,
    pub title: String,
    pub detail: String,
}

/// An authored review thread anchored to a stable Susumu target.
///
/// Review records capture human discussion and ownership; they do not assert
/// that an expectation is verified or that a decision is accepted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewThread {
    pub id: String,
    pub target: ExpectationTarget,
    pub subject: Option<String>,
    /// Optional stable record or source anchor for this discussion.
    #[serde(default)]
    pub anchor: Option<ReviewAnchor>,
    pub parent: Option<String>,
    #[serde(default)]
    pub kind: ReviewCommentKind,
    pub status: ReviewStatus,
    pub owner: Option<String>,
    pub source: String,
    pub title: String,
    pub detail: String,
}

/// The authored purpose of a review contribution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewCommentKind {
    #[default]
    Comment,
    Question,
    Objection,
    Approval,
    Risk,
    Clarification,
    DecisionRequest,
}

impl fmt::Display for ReviewCommentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Comment => "comment",
            Self::Question => "question",
            Self::Objection => "objection",
            Self::Approval => "approval",
            Self::Risk => "risk",
            Self::Clarification => "clarification",
            Self::DecisionRequest => "decision_request",
        })
    }
}

impl std::str::FromStr for ReviewCommentKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "comment" => Ok(Self::Comment),
            "question" => Ok(Self::Question),
            "objection" => Ok(Self::Objection),
            "approval" => Ok(Self::Approval),
            "risk" => Ok(Self::Risk),
            "clarification" => Ok(Self::Clarification),
            "decision_request" => Ok(Self::DecisionRequest),
            _ => Err(format!("unknown review comment kind: {value}")),
        }
    }
}

/// A portable identity for the record or source location discussed by a
/// review thread. The generic target and subject remain available for older
/// review records and for scanner-facing locations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReviewAnchor {
    Expectation(String),
    Verification(String),
    Work(String),
    Decision(String),
    Finding(String),
    Source { path: String, line: Option<usize> },
}

impl fmt::Display for ReviewAnchor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expectation(id) => write!(formatter, "expectation:{id}"),
            Self::Verification(id) => write!(formatter, "verification:{id}"),
            Self::Work(id) => write!(formatter, "work:{id}"),
            Self::Decision(id) => write!(formatter, "decision:{id}"),
            Self::Finding(id) => write!(formatter, "finding:{id}"),
            Self::Source { path, line } => match line {
                Some(line) => write!(formatter, "source:{path}#{line}"),
                None => write!(formatter, "source:{path}"),
            },
        }
    }
}

impl std::str::FromStr for ReviewAnchor {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (kind, identity) = value
            .split_once(':')
            .ok_or_else(|| "review anchor must use kind:identity".to_owned())?;
        if identity.is_empty() {
            return Err("review anchor identity cannot be empty".to_owned());
        }
        match kind {
            "expectation" => Ok(Self::Expectation(identity.to_owned())),
            "verification" => Ok(Self::Verification(identity.to_owned())),
            "work" => Ok(Self::Work(identity.to_owned())),
            "decision" => Ok(Self::Decision(identity.to_owned())),
            "finding" => Ok(Self::Finding(identity.to_owned())),
            "source" => {
                let (path, line) = if let Some((path, line)) = identity.rsplit_once('#') {
                    let line = line
                        .parse::<usize>()
                        .map_err(|_| "source anchor line must be a positive integer".to_owned())?;
                    if line == 0 {
                        return Err("source anchor line must be a positive integer".to_owned());
                    }
                    (path, Some(line))
                } else {
                    (identity, None)
                };
                if path.is_empty() {
                    return Err("source anchor path cannot be empty".to_owned());
                }
                Ok(Self::Source {
                    path: path.to_owned(),
                    line,
                })
            }
            _ => Err(format!("unknown review anchor kind: {kind}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Open,
    Resolved,
    Accepted,
    Rejected,
}

impl fmt::Display for ReviewStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        })
    }
}

impl std::str::FromStr for ReviewStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            _ => Err(format!("unknown review status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkKind {
    Implementation,
    Verification,
    Documentation,
    Infrastructure,
    Review,
    Other,
}

impl fmt::Display for WorkKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Implementation => "implementation",
            Self::Verification => "verification",
            Self::Documentation => "documentation",
            Self::Infrastructure => "infrastructure",
            Self::Review => "review",
            Self::Other => "other",
        })
    }
}

impl std::str::FromStr for WorkKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "implementation" => Ok(Self::Implementation),
            "verification" => Ok(Self::Verification),
            "documentation" => Ok(Self::Documentation),
            "infrastructure" => Ok(Self::Infrastructure),
            "review" => Ok(Self::Review),
            "other" => Ok(Self::Other),
            _ => Err(format!("unknown work kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkStatus {
    Proposed,
    InProgress,
    Completed,
    Blocked,
    Superseded,
}

impl fmt::Display for WorkStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Proposed => "proposed",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Superseded => "superseded",
        })
    }
}

impl std::str::FromStr for WorkStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "superseded" => Ok(Self::Superseded),
            _ => Err(format!("unknown work status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DecisionStatus {
    Proposed,
    Accepted,
    Rejected,
    Superseded,
}

impl fmt::Display for DecisionStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
        })
    }
}

impl std::str::FromStr for DecisionStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "superseded" => Ok(Self::Superseded),
            _ => Err(format!("unknown decision status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    Passed,
    Failed,
    Inconclusive,
}

impl fmt::Display for VerificationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Inconclusive => "inconclusive",
        })
    }
}

impl std::str::FromStr for VerificationStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "inconclusive" => Ok(Self::Inconclusive),
            _ => Err(format!("unknown verification status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExpectationTarget {
    Project,
    File,
    Symbol,
    Workflow,
}

impl fmt::Display for ExpectationTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Project => "project",
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::Workflow => "workflow",
        })
    }
}

impl std::str::FromStr for ExpectationTarget {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "project" => Ok(Self::Project),
            "file" => Ok(Self::File),
            "symbol" => Ok(Self::Symbol),
            "workflow" => Ok(Self::Workflow),
            _ => Err(format!("unknown expectation target: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExpectationStatus {
    Proposed,
    Accepted,
    Superseded,
}

impl fmt::Display for ExpectationStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Superseded => "superseded",
        })
    }
}

impl std::str::FromStr for ExpectationStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "proposed" => Ok(Self::Proposed),
            "accepted" => Ok(Self::Accepted),
            "superseded" => Ok(Self::Superseded),
            _ => Err(format!("unknown expectation status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Exact,
    Likely,
    Ambiguous,
    External,
}

impl fmt::Display for Confidence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Exact => "exact",
            Self::Likely => "likely",
            Self::Ambiguous => "ambiguous",
            Self::External => "external",
        })
    }
}

impl std::str::FromStr for Confidence {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "exact" => Ok(Self::Exact),
            "likely" => Ok(Self::Likely),
            "ambiguous" => Ok(Self::Ambiguous),
            "external" => Ok(Self::External),
            _ => Err(format!("unknown confidence: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Finding {
    pub rule_id: String,
    pub source: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub file_id: Option<String>,
    pub subject: Option<String>,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "error" => Ok(Self::Error),
            _ => Err(format!("unknown severity: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Location {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Location {
    #[must_use]
    pub const fn line_span(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    #[must_use]
    pub fn start_token(&self) -> String {
        format!("{}:{}", self.start_line, self.start_column)
    }

    #[must_use]
    pub fn end_token(&self) -> String {
        format!("{}:{}", self.end_line, self.end_column)
    }
}
