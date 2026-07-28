use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

mod file;

use crate::{
    analysis::add_findings,
    model::{
        Confidence, FlowEdge, Language, ProjectAnalysis, SCHEMA_VERSION, Symbol, Workflow,
        WorkflowKind,
    },
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
            kind: WorkflowKind::Http,
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
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn resolves_local_calls_and_preserves_external_gaps() {
        let directory = tempdir().unwrap();
        let source_directory = directory.path().join("src");
        fs::create_dir(&source_directory).unwrap();
        fs::write(
            source_directory.join("main.rs"),
            r"
fn main() {
    load_order();
    charge_gateway();
}

fn load_order() {
    normalize();
}
",
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();
        let main = analysis
            .symbols
            .iter()
            .find(|symbol| symbol.name == "main")
            .unwrap();
        let load_order = analysis
            .symbols
            .iter()
            .find(|symbol| symbol.name == "load_order")
            .unwrap();

        assert!(main.entrypoint);
        assert!(analysis.flows.iter().any(|flow| {
            flow.from == main.id
                && flow.to.as_deref() == Some(load_order.id.as_str())
                && flow.confidence == Confidence::Exact
        }));
        assert!(analysis.flows.iter().any(|flow| {
            flow.from == main.id
                && flow.call == "charge_gateway"
                && flow.to.is_none()
                && flow.confidence == Confidence::External
        }));
    }

    #[test]
    fn qualified_external_methods_do_not_look_recursive() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("main.rs"),
            r"
struct State;

fn verify_password() {
    Argon2::default().verify_password();
}

fn health(state: State) {
    state.health();
}
",
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();

        assert!(
            !analysis
                .findings
                .iter()
                .any(|finding| finding.rule_id == "SUS005")
        );
        assert!(analysis.flows.iter().any(|flow| {
            flow.call == "verify_password"
                && flow.to.is_none()
                && flow.confidence == Confidence::External
        }));
    }

    #[test]
    fn symbol_fingerprints_ignore_unrelated_file_edits() {
        let directory = tempdir().unwrap();
        let source_directory = directory.path().join("src");
        fs::create_dir(&source_directory).unwrap();
        let source_path = source_directory.join("main.rs");
        fs::write(
            &source_path,
            "fn first() {\n    1;\n}\n\nfn second() {\n    2;\n}\n",
        )
        .unwrap();

        let before = scan_project(directory.path()).unwrap();
        let first_before = before
            .symbols
            .iter()
            .find(|symbol| symbol.name == "first")
            .unwrap()
            .content_hash
            .clone();
        let second_before = before
            .symbols
            .iter()
            .find(|symbol| symbol.name == "second")
            .unwrap()
            .content_hash
            .clone();

        fs::write(
            source_path,
            "fn first() {\n    1;\n}\n\nfn second() {\n    3;\n}\n",
        )
        .unwrap();
        let after = scan_project(directory.path()).unwrap();
        let first_after = after
            .symbols
            .iter()
            .find(|symbol| symbol.name == "first")
            .unwrap()
            .content_hash
            .clone();
        let second_after = after
            .symbols
            .iter()
            .find(|symbol| symbol.name == "second")
            .unwrap()
            .content_hash
            .clone();

        assert_eq!(first_before, first_after);
        assert_ne!(second_before, second_after);
    }

    #[test]
    fn scans_the_initial_language_set() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("main.py"),
            "def load():\n    save()\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("index.ts"),
            "const handle = () => { publish(); };\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("legacy.js"),
            "function migrate() { archive(); }\n",
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();
        let languages = analysis.language_counts();

        assert_eq!(languages.get(&Language::Python), Some(&1));
        assert_eq!(languages.get(&Language::TypeScript), Some(&1));
        assert_eq!(languages.get(&Language::JavaScript), Some(&1));
        assert!(analysis.symbols.iter().any(|symbol| symbol.name == "load"));
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.name == "handle")
        );
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.name == "migrate")
        );
    }

    #[test]
    fn scans_vue_script_blocks_without_treating_template_or_style_as_code() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("App.vue"),
            r#"<template>
  <button @click="loadUsers">Load</button>
</template>

<script setup lang="ts">
import { ref } from "vue";

function loadUsers() {
    return fetchUsers();
}
</script>

<style>
function not_source_code() {}
</style>
"#,
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();

        assert_eq!(analysis.language_counts().get(&Language::Vue), Some(&1));
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.name == "loadUsers")
        );
        assert!(
            !analysis
                .symbols
                .iter()
                .any(|symbol| symbol.name == "not_source_code")
        );
        assert!(
            analysis
                .dependencies
                .iter()
                .any(|dependency| dependency.name == "import { ref } from \"vue\"")
        );
        assert!(analysis.flows.iter().any(|flow| flow.call == "fetchUsers"));
        assert!(
            !analysis
                .findings
                .iter()
                .any(|finding| finding.rule_id == "SUS006")
        );
    }

    #[test]
    fn evidence_ids_are_stable_across_scans() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("main.py"),
            "def load():\n    save()\n",
        )
        .unwrap();

        let first = scan_project(directory.path()).unwrap();
        let second = scan_project(directory.path()).unwrap();

        assert_eq!(first.files[0].id, second.files[0].id);
        assert_eq!(
            first
                .symbols
                .iter()
                .map(|symbol| &symbol.id)
                .collect::<Vec<_>>(),
            second
                .symbols
                .iter()
                .map(|symbol| &symbol.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scans_php_functions_methods_and_calls() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("index.php"),
            r"<?php
use App\Services\Checkout;

function checkout() {
    validateCart();
}

class OrderController {
    public function store() {
        checkout();
    }
}
",
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();

        assert_eq!(analysis.language_counts().get(&Language::Php), Some(&1));
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.name == "checkout"
                    && symbol.kind == crate::model::SymbolKind::Function)
        );
        assert!(analysis.symbols.iter().any(
            |symbol| symbol.name == "store" && symbol.kind == crate::model::SymbolKind::Method
        ));
        assert!(analysis.flows.iter().any(|flow| flow.call == "checkout"));
    }

    #[test]
    fn detects_express_and_fastapi_workflows() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("index.ts"),
            r#"
function listUsers() { return []; }
app.get("/users", listUsers);
"#,
        )
        .unwrap();
        fs::write(
            directory.path().join("api.py"),
            r#"
@app.post("/orders")
def create_order():
    return save_order()
"#,
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();

        assert!(analysis.workflows.iter().any(|workflow| {
            workflow.trigger == "GET /users"
                && workflow.framework == "express-compatible"
                && workflow.entry_symbol.is_some()
        }));
        assert!(analysis.workflows.iter().any(|workflow| {
            workflow.trigger == "POST /orders"
                && workflow.framework == "fastapi-compatible"
                && workflow.entry_symbol.is_some()
        }));
    }

    #[test]
    fn detects_laravel_and_rust_http_workflows() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("routes.php"),
            r"<?php
function store() { return true; }
Route::post('/orders', 'store');

class UserController {
    #[Route('/customers', methods: ['GET'])]
    public function customers() {}
}
",
        )
        .unwrap();
        fs::write(
            directory.path().join("main.rs"),
            r#"
fn list_users() {}
#[get("/health")]
async fn health() {}
fn router() {
    Router::new().route("/users", get(list_users));
}
"#,
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();

        assert!(analysis.workflows.iter().any(|workflow| {
            workflow.trigger == "POST /orders"
                && workflow.framework == "laravel"
                && workflow.entry_symbol.is_some()
        }));
        assert!(analysis.workflows.iter().any(|workflow| {
            workflow.trigger == "GET /users"
                && workflow.framework == "axum-compatible"
                && workflow.entry_symbol.is_some()
        }));
        assert!(analysis.workflows.iter().any(|workflow| {
            workflow.trigger == "GET /customers"
                && workflow.framework == "symfony"
                && workflow.entry_symbol.is_some()
        }));
        assert!(analysis.workflows.iter().any(|workflow| {
            workflow.trigger == "GET /health"
                && workflow.framework == "actix-web"
                && workflow.entry_symbol.is_some()
        }));
        assert_eq!(analysis.workflow_priorities.len(), analysis.workflows.len());
        assert!(
            analysis
                .workflow_priorities
                .iter()
                .any(|priority| priority.score > 0
                    && priority.detail.contains("workflow trigger observed"))
        );
    }

    #[test]
    fn rust_workflows_ignore_route_shapes_inside_string_literals() {
        let directory = tempdir().unwrap();
        fs::write(
            directory.path().join("scanner_test.rs"),
            r##"
fn writes_fixture_files() {
    let php = r"<?php
class UserController {
    #[Route('/customers', methods: ['GET'])]
    public function customers() {}
}
";
    let rust = r#"
#[get("/health")]
async fn health() {}
Router::new().route("/users", get(list_users));
"#;
    let _ = (php, rust);
}
"##,
        )
        .unwrap();

        let analysis = scan_project(directory.path()).unwrap();

        assert!(
            analysis.workflows.is_empty(),
            "string fixture contents must not be treated as executable workflows"
        );
    }
}
