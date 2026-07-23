use anyhow::{Context, Result, anyhow, bail};

use crate::model::{
    Decision, Dependency, Expectation, Finding, FlowEdge, ProjectAnalysis, SourceFile, Symbol,
    Verification, Work, Workflow, WorkflowPriority,
};

use crate::susu_parse::{
    Token, atom_at, fields, optional_id, parse_location, parse_number, parse_work_statement,
    required, statements, tokenize,
};
pub use crate::susu_write::{
    write_decisions, write_expectations, write_susu, write_verifications, write_works,
};

#[cfg(test)]
use crate::model::{
    Confidence, DecisionStatus, ExpectationStatus, ExpectationTarget, Language, Location, Severity,
    SymbolKind, VerificationStatus, WorkKind, WorkStatus, WorkflowKind,
};

/// Parses readable or minified `.susu` syntax into an analysis model.
///
/// # Errors
///
/// Returns an error for malformed syntax, missing required fields, unknown
/// record kinds, or values that do not match the declared schema.
pub fn parse_susu(source: &str) -> Result<ProjectAnalysis> {
    let mut parsed = ParsedAnalysis::default();
    for statement in statements(&tokenize(source)?) {
        parse_statement(&statement, &mut parsed)?;
    }
    parsed.into_analysis()
}

#[derive(Default)]
struct ParsedAnalysis {
    schema_version: Option<u32>,
    project_name: Option<String>,
    root: Option<String>,
    generated: Option<u64>,
    files: Vec<SourceFile>,
    symbols: Vec<Symbol>,
    dependencies: Vec<Dependency>,
    workflows: Vec<Workflow>,
    workflow_priorities: Vec<WorkflowPriority>,
    flows: Vec<FlowEdge>,
    expectations: Vec<Expectation>,
    verifications: Vec<Verification>,
    decisions: Vec<Decision>,
    works: Vec<Work>,
    findings: Vec<Finding>,
}

impl ParsedAnalysis {
    fn into_analysis(self) -> Result<ProjectAnalysis> {
        Ok(ProjectAnalysis {
            schema_version: self.schema_version.context("missing susu statement")?,
            project_name: self.project_name.context("missing project name")?,
            root: self.root.context("missing project root")?,
            generated_unix_seconds: self.generated.context("missing project timestamp")?,
            files: self.files,
            symbols: self.symbols,
            dependencies: self.dependencies,
            workflows: self.workflows,
            workflow_priorities: self.workflow_priorities,
            flows: self.flows,
            expectations: self.expectations,
            verifications: self.verifications,
            decisions: self.decisions,
            works: self.works,
            findings: self.findings,
        })
    }
}

fn parse_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let Some(Token::Atom(kind)) = statement.first() else {
        return Ok(());
    };
    match kind.as_str() {
        "susu" | "project" => parse_metadata_statement(kind, statement, parsed),
        "file" | "symbol" | "dependency" | "workflow" | "attention" | "priority" | "flow" => {
            parse_evidence_statement(kind, statement, parsed)
        }
        "expectation" | "verification" | "decision" | "work" | "finding" => {
            parse_record_statement(kind, statement, parsed)
        }
        other => bail!("unknown .susu statement: {other}"),
    }
}

fn parse_metadata_statement(
    kind: &str,
    statement: &[Token],
    parsed: &mut ParsedAnalysis,
) -> Result<()> {
    let values = fields(statement, 1)?;
    match kind {
        "susu" => {
            parsed.schema_version = Some(parse_number(required(&values, "version")?, "version")?);
        }
        "project" => {
            parsed.project_name = Some(required(&values, "name")?.to_owned());
            parsed.root = Some(required(&values, "root")?.to_owned());
            parsed.generated = Some(parse_number(required(&values, "generated")?, "generated")?);
        }
        _ => unreachable!("non-metadata statement routed to metadata parser"),
    }
    Ok(())
}

fn parse_evidence_statement(
    kind: &str,
    statement: &[Token],
    parsed: &mut ParsedAnalysis,
) -> Result<()> {
    match kind {
        "file" => parse_file_statement(statement, parsed),
        "symbol" => parse_symbol_statement(statement, parsed),
        "dependency" => parse_dependency_statement(statement, parsed),
        "workflow" => parse_workflow_statement(statement, parsed),
        "attention" | "priority" => parse_priority_statement(statement, parsed),
        "flow" => parse_flow_statement(statement, parsed),
        _ => unreachable!("non-evidence statement routed to evidence parser"),
    }
}

fn parse_file_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let id = atom_at(statement, 1, "file id")?;
    let values = fields(statement, 2)?;
    parsed.files.push(SourceFile {
        id,
        path: required(&values, "path")?.to_owned(),
        language: required(&values, "language")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        lines: parse_number(required(&values, "lines")?, "lines")?,
        bytes: parse_number(required(&values, "bytes")?, "bytes")?,
        content_hash: optional_id(values.get("hash").map_or("-", String::as_str)),
    });
    Ok(())
}

fn parse_symbol_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let id = atom_at(statement, 1, "symbol id")?;
    let values = fields(statement, 2)?;
    parsed.symbols.push(Symbol {
        id,
        name: required(&values, "name")?.to_owned(),
        kind: required(&values, "kind")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        file_id: required(&values, "file")?.to_owned(),
        content_hash: optional_id(values.get("hash").map_or("-", String::as_str)),
        location: parse_location(&values)?,
        entrypoint: required(&values, "entry")?
            .parse()
            .context("entry must be true or false")?,
    });
    Ok(())
}

fn parse_dependency_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let values = fields(statement, 1)?;
    parsed.dependencies.push(Dependency {
        file_id: required(&values, "file")?.to_owned(),
        name: required(&values, "name")?.to_owned(),
        location: parse_location(&values)?,
    });
    Ok(())
}

fn parse_workflow_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let id = atom_at(statement, 1, "workflow id")?;
    let values = fields(statement, 2)?;
    parsed.workflows.push(Workflow {
        id,
        kind: required(&values, "kind")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        framework: required(&values, "framework")?.to_owned(),
        trigger: required(&values, "trigger")?.to_owned(),
        handler: optional_id(required(&values, "handler")?),
        entry_symbol: optional_id(required(&values, "entry")?),
        file_id: required(&values, "file")?.to_owned(),
        confidence: required(&values, "confidence")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        location: parse_location(&values)?,
    });
    Ok(())
}

fn parse_priority_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let values = fields(statement, 1)?;
    parsed.workflow_priorities.push(WorkflowPriority {
        workflow_id: required(&values, "workflow")?.to_owned(),
        source: values
            .get("source")
            .cloned()
            .unwrap_or_else(|| "susumu:derived".to_owned()),
        score: parse_number(required(&values, "score")?, "score")?,
        detail: required(&values, "detail")?.to_owned(),
    });
    Ok(())
}

fn parse_flow_statement(statement: &[Token], parsed: &mut ParsedAnalysis) -> Result<()> {
    let from = atom_at(statement, 1, "flow source")?;
    if statement.get(2) != Some(&Token::Arrow) {
        bail!("flow is missing -> after {from}");
    }
    let target = atom_at(statement, 3, "flow target")?;
    let values = fields(statement, 4)?;
    parsed.flows.push(FlowEdge {
        from,
        to: (target != "?").then_some(target),
        call: required(&values, "call")?.to_owned(),
        confidence: required(&values, "confidence")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        location: parse_location(&values)?,
    });
    Ok(())
}

fn parse_record_statement(
    kind: &str,
    statement: &[Token],
    parsed: &mut ParsedAnalysis,
) -> Result<()> {
    match kind {
        "expectation" => parsed
            .expectations
            .push(parse_expectation_statement(statement)?),
        "verification" => parsed
            .verifications
            .push(parse_verification_statement(statement)?),
        "decision" => parsed.decisions.push(parse_decision_statement(statement)?),
        "work" => parsed.works.push(parse_work_statement(statement)?),
        "finding" => parsed.findings.push(parse_finding_statement(statement)?),
        _ => unreachable!("non-record statement routed to record parser"),
    }
    Ok(())
}

fn parse_finding_statement(statement: &[Token]) -> Result<Finding> {
    let rule_id = atom_at(statement, 1, "finding id")?;
    let values = fields(statement, 2)?;
    Ok(Finding {
        rule_id,
        source: values
            .get("source")
            .cloned()
            .unwrap_or_else(|| "susumu:scanner".to_owned()),
        severity: required(&values, "severity")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
        file_id: optional_id(required(&values, "file")?),
        subject: optional_id(required(&values, "subject")?),
        location: if values.contains_key("start") {
            Some(parse_location(&values)?)
        } else {
            None
        },
    })
}

/// Parses authored `expectation` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when an expectation statement is malformed. Complete `.susu`
/// artifacts are accepted; non-expectation records are ignored.
pub fn parse_expectations(source: &str) -> Result<Vec<Expectation>> {
    let statements = statements(&tokenize(source)?);
    let mut expectations = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "expectation" {
            expectations.push(parse_expectation_statement(&statement)?);
        }
    }
    Ok(expectations)
}

/// Parses authored `verification` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when a verification statement is malformed. Complete
/// `.susu` artifacts are accepted; non-verification records are ignored.
pub fn parse_verifications(source: &str) -> Result<Vec<Verification>> {
    let statements = statements(&tokenize(source)?);
    let mut verifications = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "verification" {
            verifications.push(parse_verification_statement(&statement)?);
        }
    }
    Ok(verifications)
}

/// Parses authored `decision` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when a decision statement is malformed. Complete `.susu`
/// artifacts are accepted; non-decision records are ignored.
pub fn parse_decisions(source: &str) -> Result<Vec<Decision>> {
    let statements = statements(&tokenize(source)?);
    let mut decisions = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "decision" {
            decisions.push(parse_decision_statement(&statement)?);
        }
    }
    Ok(decisions)
}

/// Parses authored `work` statements from a fragment or complete artifact.
///
/// # Errors
///
/// Returns an error when a work statement is malformed. Complete `.susu`
/// artifacts are accepted; non-work records are ignored.
pub fn parse_works(source: &str) -> Result<Vec<Work>> {
    let statements = statements(&tokenize(source)?);
    let mut works = Vec::new();
    for statement in statements {
        let Some(Token::Atom(kind)) = statement.first() else {
            continue;
        };
        if kind == "work" {
            works.push(parse_work_statement(&statement)?);
        }
    }
    Ok(works)
}

fn parse_expectation_statement(statement: &[Token]) -> Result<Expectation> {
    let id = atom_at(statement, 1, "expectation id")?;
    let values = fields(statement, 2)?;
    Ok(Expectation {
        id,
        target: required(&values, "target")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        subject: optional_id(required(&values, "subject")?),
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        source: required(&values, "source")?.to_owned(),
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn parse_decision_statement(statement: &[Token]) -> Result<Decision> {
    let id = atom_at(statement, 1, "decision id")?;
    let values = fields(statement, 2)?;
    Ok(Decision {
        id,
        target: required(&values, "target")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        subject: optional_id(required(&values, "subject")?),
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        source: required(&values, "source")?.to_owned(),
        basis: optional_id(values.get("basis").map_or("-", String::as_str)),
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn parse_verification_statement(statement: &[Token]) -> Result<Verification> {
    let id = atom_at(statement, 1, "verification id")?;
    let values = fields(statement, 2)?;
    Ok(Verification {
        id,
        expectation_id: required(&values, "expectation")?.to_owned(),
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow!(error))?,
        supersedes: optional_id(values.get("supersedes").map_or("-", String::as_str)),
        execution: values
            .get("execution")
            .filter(|value| value.as_str() != "-")
            .map(|value| serde_json::from_str(value))
            .transpose()
            .context("invalid verification execution metadata")?,
        chain: optional_id(values.get("chain").map_or("-", String::as_str)),
        method: required(&values, "method")?.to_owned(),
        source: required(&values, "source")?.to_owned(),
        evidence: optional_id(required(&values, "evidence")?),
        basis: optional_id(values.get("basis").map_or("-", String::as_str)),
        detail: required(&values, "detail")?.to_owned(),
    })
}

#[cfg(test)]
#[path = "susu_tests.rs"]
mod tests;
