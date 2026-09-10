use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

mod file;

use crate::{
    analysis::add_findings,
    model::{Confidence, FlowEdge, Language, ProjectAnalysis, SCHEMA_VERSION, Symbol, Workflow},
    scanner::file::{PendingCall, PendingWorkflow, scan_file},
};

/// Scans supported source files below `root` into a deterministic evidence model.
///
/// # Errors
///
/// Returns an error when the root path cannot be resolved. Individual unreadable
/// or unparsable files are retained as findings so one file cannot abort a scan.
pub fn scan_project(root: &Path) -> Result<ProjectAnalysis> {
    let root = root
        .canonicalize()
        .with_context(|| format!("could not resolve {}", root.display()))?;
    let project_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_owned();

    let mut analysis = ProjectAnalysis {
        schema_version: SCHEMA_VERSION,
        project_name,
        root: root.to_string_lossy().into_owned(),
        generated_unix_seconds: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        source_revision: source_revision(&root),
        files: Vec::new(),
        symbols: Vec::new(),
        dependencies: Vec::new(),
        workflows: Vec::new(),
        workflow_priorities: Vec::new(),
        flows: Vec::new(),
        expectations: Vec::new(),
        verifications: Vec::new(),
        decisions: Vec::new(),
        works: Vec::new(),
        review_threads: Vec::new(),
        findings: Vec::new(),
    };
    let mut pending_calls = Vec::new();
    let mut pending_workflows = Vec::new();

    let mut paths = supported_paths(&root);
    paths.sort();
    for path in paths {
        scan_file(
            &root,
            &path,
            &mut analysis,
            &mut pending_calls,
            &mut pending_workflows,
        );
    }

    resolve_calls(&mut analysis, pending_calls);
    resolve_workflows(&mut analysis, pending_workflows);
    add_findings(&mut analysis);
    Ok(analysis)
}

fn source_revision(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn supported_paths(root: &Path) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .standard_filters(true)
        .follow_links(false)
        .same_file_system(true)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let path = entry.into_path();
            let extension = path.extension()?.to_str()?;
            Language::from_extension(extension).map(|_| path)
        })
        .collect()
}

fn resolve_workflows(analysis: &mut ProjectAnalysis, pending: Vec<PendingWorkflow>) {
    let mut by_name: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in &analysis.symbols {
        if symbol.name != "<module>" {
            by_name.entry(&symbol.name).or_default().push(symbol);
        }
    }
    let mut occurrences = HashMap::new();
    for pending_workflow in pending {
        let (entry_symbol, confidence) = pending_workflow.workflow.handler.as_deref().map_or(
            (None, Confidence::External),
            |handler| {
                let candidates = by_name.get(handler).cloned().unwrap_or_default();
                let local = candidates
                    .iter()
                    .filter(|symbol| symbol.file_id == pending_workflow.file_id)
                    .copied()
                    .collect::<Vec<_>>();
                match (local.as_slice(), candidates.as_slice()) {
                    ([symbol], _) => (Some(symbol.id.clone()), Confidence::Exact),
                    ([], [symbol]) => (Some(symbol.id.clone()), Confidence::Likely),
                    ([], []) => (None, Confidence::External),
                    _ => (None, Confidence::Ambiguous),
                }
            },
        );
        let key = format!(
            "{}:{}:{}:{}",
            pending_workflow.file_id,
            pending_workflow.workflow.framework,
            pending_workflow.workflow.method,
            pending_workflow.workflow.path
        );
        let occurrence = occurrences.entry(key.clone()).or_insert(0_usize);
        let ordinal = occurrence.to_string();
        let id = stable_id("w", &[&key, &ordinal]);
        *occurrence += 1;
        analysis.workflows.push(Workflow {
            id,
            kind: pending_workflow.workflow.kind,
            framework: pending_workflow.workflow.framework,
            trigger: format!(
                "{} {}",
                pending_workflow.workflow.method, pending_workflow.workflow.path
            ),
            handler: pending_workflow.workflow.handler,
            entry_symbol,
            file_id: pending_workflow.file_id,
            confidence,
            location: pending_workflow.workflow.location,
        });
    }
}

pub(crate) fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    let hash = digest.finalize();
    format!("{prefix}_{}", hex_prefix(&hash, 8))
}

pub(crate) fn hex_prefix(bytes: &[u8], count: usize) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(count * 2);
    for byte in bytes.iter().take(count) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn resolve_calls(analysis: &mut ProjectAnalysis, pending: Vec<PendingCall>) {
    let mut by_name: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for symbol in &analysis.symbols {
        if symbol.name != "<module>" {
            by_name.entry(&symbol.name).or_default().push(symbol);
        }
    }

    for pending_call in pending {
        let locally_scoped = pending_call
            .call
            .receiver
            .as_deref()
            .is_none_or(|receiver| {
                receiver.starts_with("self.")
                    || receiver.starts_with("this.")
                    || receiver.starts_with("Self::")
            });
        let candidates = if locally_scoped {
            by_name
                .get(pending_call.call.name.as_str())
                .cloned()
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let local: Vec<_> = candidates
            .iter()
            .filter(|symbol| symbol.file_id == pending_call.file_id)
            .copied()
            .collect();
        let (to, confidence) = match (local.as_slice(), candidates.as_slice()) {
            ([symbol], _) => (Some(symbol.id.clone()), Confidence::Exact),
            ([], [symbol]) => (Some(symbol.id.clone()), Confidence::Likely),
            ([], []) => (None, Confidence::External),
            _ => (None, Confidence::Ambiguous),
        };
        analysis.flows.push(FlowEdge {
            from: pending_call.caller_id,
            to,
            call: pending_call.call.name,
            confidence,
            location: pending_call.call.location,
        });
    }
}

#[cfg(test)]
mod tests;
