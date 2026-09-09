use crate::model::ProjectAnalysis;

mod basis;
mod findings;
mod priorities;
mod relationships;

pub use basis::{anchor_decision_bases, anchor_verification_bases};
pub use priorities::refresh_workflow_priorities;
pub use relationships::refresh_relationship_findings;

pub(crate) fn add_findings(analysis: &mut ProjectAnalysis) {
    crate::derived_findings::add_static_findings(analysis);
    refresh_relationship_findings(analysis);
    refresh_workflow_priorities(analysis);
}

pub fn refresh_expectation_findings(analysis: &mut ProjectAnalysis) {
    refresh_relationship_findings(analysis);
}

pub fn refresh_derived_analysis(analysis: &mut ProjectAnalysis) {
    refresh_relationship_findings(analysis);
    refresh_workflow_priorities(analysis);
}

#[cfg(test)]
mod tests;
