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
    assert!(analysis.symbols.iter().any(
        |symbol| symbol.name == "checkout" && symbol.kind == crate::model::SymbolKind::Function
    ));
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
fn detects_react_router_and_react_navigation_targets() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("routes.jsx"),
        r#"
function Users() { return null; }

export function AppRoutes() {
    return (
        <Routes>
            <Route path="/users" element={<Users />} />
            <Route index element={<Home />} />
        </Routes>
    );
}
"#,
    )
    .unwrap();
    fs::write(
        directory.path().join("navigator.tsx"),
        r#"
function HomeScreen() { return null; }

export function RootNavigator() {
    return (
        <Stack.Navigator>
            <Stack.Screen name="Home" component={HomeScreen} />
        </Stack.Navigator>
    );
}
"#,
    )
    .unwrap();

    let analysis = scan_project(directory.path()).unwrap();

    assert!(analysis.workflows.iter().any(|workflow| {
        workflow.trigger == "ROUTE /users"
            && workflow.kind == crate::model::WorkflowKind::Navigation
            && workflow.framework == "react-router"
            && workflow.entry_symbol.is_some()
    }));
    assert!(analysis.workflows.iter().any(|workflow| {
        workflow.trigger == "ROUTE (index)" && workflow.framework == "react-router"
    }));
    assert!(analysis.workflows.iter().any(|workflow| {
        workflow.trigger == "SCREEN Home"
            && workflow.kind == crate::model::WorkflowKind::Navigation
            && workflow.framework == "react-navigation"
            && workflow.handler.as_deref() == Some("HomeScreen")
            && workflow.entry_symbol.is_some()
    }));
    assert!(
        analysis
            .workflow_priorities
            .iter()
            .any(|priority| priority.detail.contains("navigation route observed"))
    );
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
    assert!(analysis.workflow_priorities.iter().any(
        |priority| priority.score > 0 && priority.detail.contains("workflow trigger observed")
    ));
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
