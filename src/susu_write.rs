use std::fmt::Write as _;

use anyhow::Result;

use crate::model::{Decision, Expectation, ProjectAnalysis, Verification, Work};

/// Serializes an analysis using the versioned `.susu` grammar.
///
/// # Errors
///
/// Returns an error if a string field cannot be encoded.
pub fn write_susu(analysis: &ProjectAnalysis, minified: bool) -> Result<String> {
    let mut statements = vec![
        format!("susu version={}", analysis.schema_version),
        format!(
            "project name={} root={} generated={}",
            quote(&analysis.project_name)?,
            quote(&analysis.root)?,
            analysis.generated_unix_seconds
        ),
    ];
    write_source_statements(analysis, &mut statements)?;
    write_relationship_statements(analysis, &mut statements)?;
    write_record_statements(analysis, &mut statements)?;
    write_finding_statements(analysis, &mut statements)?;
    Ok(finish_statements(statements, minified, true))
}

fn write_source_statements(analysis: &ProjectAnalysis, statements: &mut Vec<String>) -> Result<()> {
    for file in &analysis.files {
        statements.push(format!(
            "file {} path={} language={} lines={} bytes={} hash={}",
            file.id,
            quote(&file.path)?,
            file.language,
            file.lines,
            file.bytes,
            file.content_hash.as_deref().unwrap_or("-")
        ));
    }
    for symbol in &analysis.symbols {
        statements.push(format!(
            "symbol {} name={} kind={} file={} start={} end={} entry={} hash={}",
            symbol.id,
            quote(&symbol.name)?,
            symbol.kind,
            symbol.file_id,
            symbol.location.start_token(),
            symbol.location.end_token(),
            symbol.entrypoint,
            symbol.content_hash.as_deref().unwrap_or("-")
        ));
    }
    for dependency in &analysis.dependencies {
        statements.push(format!(
            "dependency file={} name={} start={} end={}",
            dependency.file_id,
            quote(&dependency.name)?,
            dependency.location.start_token(),
            dependency.location.end_token()
        ));
    }
    Ok(())
}

fn write_relationship_statements(
    analysis: &ProjectAnalysis,
    statements: &mut Vec<String>,
) -> Result<()> {
    for workflow in &analysis.workflows {
        statements.push(format!(
            "workflow {} kind={} framework={} trigger={} handler={} entry={} file={} confidence={} start={} end={}",
            workflow.id, workflow.kind, quote(&workflow.framework)?, quote(&workflow.trigger)?,
            optional_quoted(workflow.handler.as_deref())?, workflow.entry_symbol.as_deref().unwrap_or("-"),
            workflow.file_id, workflow.confidence, workflow.location.start_token(), workflow.location.end_token()
        ));
    }
    for priority in &analysis.workflow_priorities {
        statements.push(format!(
            "attention workflow={} source={} score={} detail={}",
            priority.workflow_id,
            quote(&priority.source)?,
            priority.score,
            quote(&priority.detail)?
        ));
    }
    for flow in &analysis.flows {
        statements.push(format!(
            "flow {} -> {} call={} confidence={} start={} end={}",
            flow.from,
            flow.to.as_deref().unwrap_or("?"),
            quote(&flow.call)?,
            flow.confidence,
            flow.location.start_token(),
            flow.location.end_token()
        ));
    }
    Ok(())
}

fn write_record_statements(analysis: &ProjectAnalysis, statements: &mut Vec<String>) -> Result<()> {
    statements.extend(
        analysis
            .expectations
            .iter()
            .map(expectation_statement)
            .collect::<Result<Vec<_>>>()?,
    );
    statements.extend(
        analysis
            .verifications
            .iter()
            .map(verification_statement)
            .collect::<Result<Vec<_>>>()?,
    );
    statements.extend(
        analysis
            .decisions
            .iter()
            .map(decision_statement)
            .collect::<Result<Vec<_>>>()?,
    );
    statements.extend(
        analysis
            .works
            .iter()
            .map(work_statement)
            .collect::<Result<Vec<_>>>()?,
    );
    Ok(())
}

fn write_finding_statements(
    analysis: &ProjectAnalysis,
    statements: &mut Vec<String>,
) -> Result<()> {
    for finding in &analysis.findings {
        let mut statement = format!(
            "finding {} source={} severity={} title={} detail={} file={} subject={}",
            finding.rule_id,
            quote(&finding.source)?,
            finding.severity,
            quote(&finding.title)?,
            quote(&finding.detail)?,
            finding.file_id.as_deref().unwrap_or("-"),
            finding.subject.as_deref().unwrap_or("-")
        );
        if let Some(location) = &finding.location {
            write!(
                statement,
                " start={} end={}",
                location.start_token(),
                location.end_token()
            )?;
        }
        statements.push(statement);
    }
    Ok(())
}

/// Serializes authored expectations as an expectation-only sidecar.
///
/// # Errors
///
/// Returns an error if a string field cannot be encoded.
pub fn write_expectations(expectations: &[Expectation], minified: bool) -> Result<String> {
    write_records(expectations, minified, expectation_statement)
}

/// Serializes verification records as a verification-only sidecar.
///
/// # Errors
///
/// Returns an error if a string field cannot be encoded.
pub fn write_verifications(verifications: &[Verification], minified: bool) -> Result<String> {
    write_records(verifications, minified, verification_statement)
}

/// Serializes decision records as a decision-only sidecar.
///
/// # Errors
///
/// Returns an error if a string field cannot be encoded.
pub fn write_decisions(decisions: &[Decision], minified: bool) -> Result<String> {
    write_records(decisions, minified, decision_statement)
}

/// Serializes work records as a work-only sidecar.
///
/// # Errors
///
/// Returns an error if a string field cannot be encoded.
pub fn write_works(works: &[Work], minified: bool) -> Result<String> {
    write_records(works, minified, work_statement)
}

fn write_records<T>(
    records: &[T],
    minified: bool,
    statement: fn(&T) -> Result<String>,
) -> Result<String> {
    let statements = records.iter().map(statement).collect::<Result<Vec<_>>>()?;
    Ok(finish_statements(statements, minified, false))
}

fn expectation_statement(expectation: &Expectation) -> Result<String> {
    Ok(format!(
        "expectation {} target={} subject={} status={} source={} title={} detail={}",
        expectation.id,
        expectation.target,
        expectation.subject.as_deref().unwrap_or("-"),
        expectation.status,
        quote(&expectation.source)?,
        quote(&expectation.title)?,
        quote(&expectation.detail)?
    ))
}

fn verification_statement(verification: &Verification) -> Result<String> {
    let supersedes = verification
        .supersedes
        .as_deref()
        .map(|id| format!(" supersedes={id}"))
        .unwrap_or_default();
    Ok(format!(
        "verification {} expectation={} status={}{} method={} source={} evidence={} basis={} detail={}",
        verification.id,
        verification.expectation_id,
        verification.status,
        supersedes,
        quote(&verification.method)?,
        quote(&verification.source)?,
        optional_quoted(verification.evidence.as_deref())?,
        verification.basis.as_deref().unwrap_or("-"),
        quote(&verification.detail)?
    ))
}

fn decision_statement(decision: &Decision) -> Result<String> {
    Ok(format!(
        "decision {} target={} subject={} status={} source={} basis={} title={} detail={}",
        decision.id,
        decision.target,
        decision.subject.as_deref().unwrap_or("-"),
        decision.status,
        quote(&decision.source)?,
        decision.basis.as_deref().unwrap_or("-"),
        quote(&decision.title)?,
        quote(&decision.detail)?
    ))
}

fn work_statement(work: &Work) -> Result<String> {
    Ok(format!(
        "work {} target={} subject={} expectation={} kind={} status={} source={} evidence={} title={} detail={}",
        work.id,
        work.target,
        work.subject.as_deref().unwrap_or("-"),
        work.expectation_id.as_deref().unwrap_or("-"),
        work.kind,
        work.status,
        quote(&work.source)?,
        optional_quoted(work.evidence.as_deref())?,
        quote(&work.title)?,
        quote(&work.detail)?
    ))
}

fn optional_quoted(value: Option<&str>) -> Result<String> {
    value
        .map(quote)
        .transpose()
        .map(|value| value.unwrap_or_else(|| "-".to_owned()))
}

fn finish_statements(statements: Vec<String>, minified: bool, always_terminate: bool) -> String {
    let separator = if minified { ";" } else { ";\n" };
    let mut iterator = statements.into_iter();
    let mut output = iterator.next().unwrap_or_default();
    for statement in iterator {
        output.push_str(separator);
        output.push_str(&statement);
    }
    if always_terminate || !output.is_empty() {
        output.push(';');
    }
    if !minified {
        output.push('\n');
    }
    output
}

fn quote(value: &str) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
