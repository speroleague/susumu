use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

mod records;

pub use records::{
    Decision, DecisionStatus, Expectation, ExpectationStatus, ExpectationTarget, ReviewAnchor,
    ReviewCommentKind, ReviewStatus, ReviewThread, Verification, VerificationExecution,
    VerificationStatus, Work, WorkKind, WorkStatus,
};

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
    /// Client-side route or screen registration, such as a React Router route or
    /// a React Navigation screen. The trigger names the route path or screen name.
    Navigation,
}

impl fmt::Display for WorkflowKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Http => "http",
            Self::Navigation => "navigation",
        })
    }
}

impl std::str::FromStr for WorkflowKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "http" => Ok(Self::Http),
            "navigation" => Ok(Self::Navigation),
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

/// Finding rule ids that mean "a verification or decision was recorded against
/// evidence that has since changed" — the dirty-evidence findings.
pub const DIRTY_FINDING_RULES: [&str; 2] = ["SUS023", "SUS033"];

impl Finding {
    /// Whether this finding reports changed verification or decision evidence.
    #[must_use]
    pub fn is_dirty_evidence(&self) -> bool {
        DIRTY_FINDING_RULES.contains(&self.rule_id.as_str())
    }
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
