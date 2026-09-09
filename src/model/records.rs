use std::fmt;

use serde::{Deserialize, Serialize};

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
