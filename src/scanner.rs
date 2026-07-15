use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use sha2::{Digest, Sha256};

use crate::{
    analysis::add_findings,
    language::{ParsedCall, ParsedWorkflow, parse_file},
    model::{
        Confidence, Dependency, Finding, FlowEdge, Language, Location, ProjectAnalysis,
        SCHEMA_VERSION, Severity, SourceFile, Symbol, Workflow, WorkflowKind,
    },
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
struct PendingCall {
    file_id: String,
    caller_id: String,
    call: ParsedCall,
}

#[derive(Debug)]
struct PendingWorkflow {
    file_id: String,
    workflow: ParsedWorkflow,
}

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

#[allow(clippy::too_many_lines)]
fn scan_file(
    root: &Path,
    path: &Path,
    analysis: &mut ProjectAnalysis,
    pending_calls: &mut Vec<PendingCall>,
    pending_workflows: &mut Vec<PendingWorkflow>,
) {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_string = relative.to_string_lossy().replace('\\', "/");
    let file_id = stable_id("f", &[&relative_string]);
    let Some(language) = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Language::from_extension)
    else {
        return;
    };
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() > MAX_FILE_BYTES {
        analysis.findings.push(Finding {
            rule_id: "SUS000".to_owned(),
            source: "susumu:scanner".to_owned(),
            severity: Severity::Info,
            title: "File skipped".to_owned(),
            detail: format!("{relative_string} exceeds the 2 MiB scan limit"),
            file_id: None,
            subject: None,
            location: None,
        });
        return;
    }

    let Ok(source) = fs::read_to_string(path) else {
        analysis.findings.push(Finding {
            rule_id: "SUS000".to_owned(),
            source: "susumu:scanner".to_owned(),
            severity: Severity::Info,
            title: "File skipped".to_owned(),
            detail: format!("{relative_string} is not readable UTF-8 text"),
            file_id: None,
            subject: None,
            location: None,
        });
        return;
    };
    let lines = source.lines().count().max(1);
    analysis.files.push(SourceFile {
        id: file_id.clone(),
        path: relative_string.clone(),
        language,
        lines,
        bytes: metadata.len(),
        content_hash: Some(content_hash(&source)),
    });

    let module_entrypoint = is_module_entrypoint(relative, language);
    let parsed = match parse_file(language, &source, module_entrypoint) {
        Ok(parsed) => parsed,
        Err(error) => {
            analysis.findings.push(Finding {
                rule_id: "SUS000".to_owned(),
                source: "susumu:scanner".to_owned(),
                severity: Severity::Warning,
                title: "Parser failed".to_owned(),
                detail: format!("{relative_string}: {error:#}"),
                file_id: Some(file_id),
                subject: None,
                location: None,
            });
            return;
        }
    };

    if parsed.has_parse_errors {
        analysis.findings.push(Finding {
            rule_id: "SUS006".to_owned(),
            source: "susumu:scanner".to_owned(),
            severity: Severity::Info,
            title: "Incomplete syntax tree".to_owned(),
            detail: "Tree-sitter recovered from syntax it could not fully parse; evidence from this file may be incomplete.".to_owned(),
            file_id: Some(file_id.clone()),
            subject: None,
            location: Some(Location {
                start_line: 1,
                start_column: 1,
                end_line: lines,
                end_column: 1,
            }),
        });
    }

    let mut occurrences = HashMap::new();
    let mut symbol_ids = Vec::with_capacity(parsed.symbols.len());
    for parsed_symbol in parsed.symbols {
        let occurrence = occurrences
            .entry((parsed_symbol.name.clone(), parsed_symbol.kind))
            .or_insert(0_usize);
        let kind = parsed_symbol.kind.to_string();
        let ordinal = occurrence.to_string();
        let id = stable_id(
            "s",
            &[&relative_string, &kind, &parsed_symbol.name, &ordinal],
        );
        *occurrence += 1;
        symbol_ids.push(id.clone());
        analysis.symbols.push(Symbol {
            id,
            name: parsed_symbol.name,
            kind: parsed_symbol.kind,
            file_id: file_id.clone(),
            location: parsed_symbol.location,
            entrypoint: parsed_symbol.entrypoint,
        });
    }
    for dependency in parsed.dependencies {
        analysis.dependencies.push(Dependency {
            file_id: file_id.clone(),
            name: dependency.name,
            location: dependency.location,
        });
    }
    for workflow in parsed.workflows {
        pending_workflows.push(PendingWorkflow {
            file_id: file_id.clone(),
            workflow,
        });
    }
    for call in parsed.calls {
        let Some(caller_id) = symbol_ids.get(call.caller).cloned() else {
            continue;
        };
        pending_calls.push(PendingCall {
            file_id: file_id.clone(),
            caller_id,
            call,
        });
    }
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

fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    let hash = digest.finalize();
    format!("{prefix}_{}", hex_prefix(&hash, 8))
}

fn content_hash(source: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(source.as_bytes());
    hex_prefix(&digest.finalize(), 16)
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
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
        let candidates = by_name
            .get(pending_call.call.name.as_str())
            .cloned()
            .unwrap_or_default();
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

fn is_module_entrypoint(relative: &Path, language: Language) -> bool {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    let file_name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match language {
        Language::Rust => normalized == "src/main.rs" || file_name == "main.rs",
        Language::Php => file_name == "index.php" || normalized.starts_with("routes/"),
        Language::Python => file_name == "__main__.py" || file_name == "main.py",
        Language::JavaScript | Language::TypeScript | Language::Tsx => matches!(
            file_name,
            "index.js" | "index.ts" | "index.tsx" | "main.js" | "main.ts" | "app.js" | "app.ts"
        ),
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
