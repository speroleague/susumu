use susumu::model::{
    DecisionStatus, ExpectationStatus, ExpectationTarget, ReviewStatus, VerificationStatus,
    WorkKind, WorkStatus,
};

#[derive(Debug, Clone)]
pub(crate) struct GitTargetDepthArg(pub(crate) GitTargetDepth);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitTargetDepth {
    Project,
    File,
    Workflow,
}

impl std::str::FromStr for GitTargetDepthArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "project" => Ok(Self(GitTargetDepth::Project)),
            "file" => Ok(Self(GitTargetDepth::File)),
            "workflow" => Ok(Self(GitTargetDepth::Workflow)),
            _ => Err(format!("unknown git target depth: {value}")),
        }
    }
}

impl From<GitTargetDepthArg> for GitTargetDepth {
    fn from(value: GitTargetDepthArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExpectationTargetArg(pub(crate) ExpectationTarget);

impl std::str::FromStr for ExpectationTargetArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<ExpectationTargetArg> for ExpectationTarget {
    fn from(value: ExpectationTargetArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExpectationStatusArg(pub(crate) ExpectationStatus);

impl std::str::FromStr for ExpectationStatusArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<ExpectationStatusArg> for ExpectationStatus {
    fn from(value: ExpectationStatusArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VerificationStatusArg(pub(crate) VerificationStatus);

impl std::str::FromStr for VerificationStatusArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<VerificationStatusArg> for VerificationStatus {
    fn from(value: VerificationStatusArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DecisionStatusArg(pub(crate) DecisionStatus);

impl std::str::FromStr for DecisionStatusArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<DecisionStatusArg> for DecisionStatus {
    fn from(value: DecisionStatusArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkKindArg(pub(crate) WorkKind);

impl std::str::FromStr for WorkKindArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<WorkKindArg> for WorkKind {
    fn from(value: WorkKindArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkStatusArg(pub(crate) WorkStatus);

impl std::str::FromStr for WorkStatusArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<WorkStatusArg> for WorkStatus {
    fn from(value: WorkStatusArg) -> Self {
        value.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ReviewStatusArg(pub(crate) ReviewStatus);

impl std::str::FromStr for ReviewStatusArg {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse().map(Self)
    }
}

impl From<ReviewStatusArg> for ReviewStatus {
    fn from(value: ReviewStatusArg) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_value_adapters_parse_domain_enums() {
        let depth: GitTargetDepth = "workflow"
            .parse::<GitTargetDepthArg>()
            .expect("parse git target depth")
            .into();
        let target: ExpectationTarget = "project"
            .parse::<ExpectationTargetArg>()
            .expect("parse expectation target")
            .into();
        let expectation_status: ExpectationStatus = "accepted"
            .parse::<ExpectationStatusArg>()
            .expect("parse expectation status")
            .into();
        let verification_status: VerificationStatus = "passed"
            .parse::<VerificationStatusArg>()
            .expect("parse verification status")
            .into();
        let decision_status: DecisionStatus = "accepted"
            .parse::<DecisionStatusArg>()
            .expect("parse decision status")
            .into();
        let work_kind: WorkKind = "infrastructure"
            .parse::<WorkKindArg>()
            .expect("parse work kind")
            .into();
        let work_status: WorkStatus = "completed"
            .parse::<WorkStatusArg>()
            .expect("parse work status")
            .into();

        assert_eq!(depth, GitTargetDepth::Workflow);
        assert_eq!(target, ExpectationTarget::Project);
        assert_eq!(expectation_status, ExpectationStatus::Accepted);
        assert_eq!(verification_status, VerificationStatus::Passed);
        assert_eq!(decision_status, DecisionStatus::Accepted);
        assert_eq!(work_kind, WorkKind::Infrastructure);
        assert_eq!(work_status, WorkStatus::Completed);
        assert!("repository".parse::<GitTargetDepthArg>().is_err());
    }
}
