use tree_sitter::{Language as TreeLanguage, Node};

use crate::model::{Language, SymbolKind};

use super::{
    HTTP_METHODS, ParsedWorkflow, is_http_method, location, quoted_strings, quoted_value,
    terminal_identifier,
};

pub(super) trait LanguageAdapter {
    fn grammar(&self) -> TreeLanguage;

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)>;

    fn is_call(&self, kind: &str) -> bool;

    fn is_dependency(&self, kind: &str) -> bool;

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow>;

    fn call_name(&self, node: Node<'_>, source: &[u8]) -> Option<String> {
        default_call_name(node, source)
    }

    fn normalize_dependency(&self, text: &str) -> String {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_end_matches(';')
            .to_owned()
    }
}

pub(super) fn adapter_for(language: Language) -> &'static dyn LanguageAdapter {
    match language {
        Language::Rust => &RUST,
        Language::Php => &PHP,
        Language::Python => &PYTHON,
        Language::JavaScript => &JAVASCRIPT,
        Language::TypeScript => &TYPESCRIPT,
        Language::Tsx => &TSX,
        Language::Vue => &VUE,
    }
}

static RUST: RustAdapter = RustAdapter;
static PHP: PhpAdapter = PhpAdapter;
static PYTHON: PythonAdapter = PythonAdapter;
static JAVASCRIPT: JavaScriptAdapter = JavaScriptAdapter;
static TYPESCRIPT: TypeScriptAdapter = TypeScriptAdapter;
static TSX: TsxAdapter = TsxAdapter;
static VUE: VueAdapter = VueAdapter;

struct RustAdapter;
struct PhpAdapter;
struct PythonAdapter;
struct JavaScriptAdapter;
struct TypeScriptAdapter;
struct TsxAdapter;
struct VueAdapter;

impl LanguageAdapter for RustAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        if node.kind() != "function_item" {
            return None;
        }
        symbol_from_name_field(node, source, rust_symbol_kind(node))
    }

    fn is_call(&self, kind: &str) -> bool {
        kind == "call_expression"
    }

    fn is_dependency(&self, kind: &str) -> bool {
        kind == "use_declaration"
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        rust_workflows(node, source)
    }

    fn normalize_dependency(&self, text: &str) -> String {
        let normalized = normalized_dependency(text);
        normalized
            .strip_prefix("use ")
            .unwrap_or(&normalized)
            .trim_end_matches(';')
            .to_owned()
    }
}

impl LanguageAdapter for PhpAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_php::LANGUAGE_PHP.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        match node.kind() {
            "function_definition" => symbol_from_name_field(node, source, SymbolKind::Function),
            "method_declaration" => symbol_from_name_field(node, source, SymbolKind::Method),
            _ => None,
        }
    }

    fn is_call(&self, kind: &str) -> bool {
        matches!(
            kind,
            "function_call_expression"
                | "member_call_expression"
                | "scoped_call_expression"
                | "object_creation_expression"
        )
    }

    fn is_dependency(&self, kind: &str) -> bool {
        matches!(
            kind,
            "namespace_use_declaration"
                | "include_expression"
                | "include_once_expression"
                | "require_expression"
                | "require_once_expression"
        )
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        php_workflows(node, source)
    }
}

impl LanguageAdapter for PythonAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_python::LANGUAGE.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        if node.kind() != "function_definition" {
            return None;
        }
        let kind = if node
            .parent()
            .is_some_and(|parent| parent.kind() == "class_definition")
        {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        };
        symbol_from_name_field(node, source, kind)
    }

    fn is_call(&self, kind: &str) -> bool {
        kind == "call"
    }

    fn is_dependency(&self, kind: &str) -> bool {
        matches!(kind, "import_statement" | "import_from_statement")
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        python_workflows(node, source)
    }
}

impl LanguageAdapter for JavaScriptAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_javascript::LANGUAGE.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        javascript_symbol(node, source)
    }

    fn is_call(&self, kind: &str) -> bool {
        javascript_is_call(kind)
    }

    fn is_dependency(&self, kind: &str) -> bool {
        kind == "import_statement"
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        javascript_workflow(node, source).into_iter().collect()
    }
}

impl LanguageAdapter for TypeScriptAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        javascript_symbol(node, source)
    }

    fn is_call(&self, kind: &str) -> bool {
        javascript_is_call(kind)
    }

    fn is_dependency(&self, kind: &str) -> bool {
        kind == "import_statement"
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        javascript_workflow(node, source).into_iter().collect()
    }
}

impl LanguageAdapter for TsxAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        javascript_symbol(node, source)
    }

    fn is_call(&self, kind: &str) -> bool {
        javascript_is_call(kind)
    }

    fn is_dependency(&self, kind: &str) -> bool {
        kind == "import_statement"
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        javascript_workflow(node, source).into_iter().collect()
    }
}

impl LanguageAdapter for VueAdapter {
    fn grammar(&self) -> TreeLanguage {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    }

    fn symbol(&self, node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
        javascript_symbol(node, source)
    }

    fn is_call(&self, kind: &str) -> bool {
        javascript_is_call(kind)
    }

    fn is_dependency(&self, kind: &str) -> bool {
        kind == "import_statement"
    }

    fn workflows(&self, node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
        javascript_workflow(node, source).into_iter().collect()
    }
}

fn symbol_from_name_field(
    node: Node<'_>,
    source: &[u8],
    kind: SymbolKind,
) -> Option<(String, SymbolKind)> {
    let name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .trim();
    (!name.is_empty()).then(|| (name.to_owned(), kind))
}

fn rust_symbol_kind(node: Node<'_>) -> SymbolKind {
    let mut parent = node.parent();
    while let Some(ancestor) = parent {
        if ancestor.kind() == "impl_item" {
            return SymbolKind::Method;
        }
        if ancestor.kind() == "source_file" || ancestor.kind() == "function_item" {
            break;
        }
        parent = ancestor.parent();
    }
    SymbolKind::Function
}

fn javascript_symbol(node: Node<'_>, source: &[u8]) -> Option<(String, SymbolKind)> {
    let kind = match node.kind() {
        "function_declaration" | "generator_function_declaration" => SymbolKind::Function,
        "method_definition" => SymbolKind::Method,
        "variable_declarator" => {
            let value = node.child_by_field_name("value")?;
            if !matches!(value.kind(), "arrow_function" | "function_expression") {
                return None;
            }
            SymbolKind::Function
        }
        _ => return None,
    };
    symbol_from_name_field(node, source, kind)
}

fn javascript_is_call(kind: &str) -> bool {
    matches!(kind, "call_expression" | "new_expression")
}

fn default_call_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let callee = node
        .child_by_field_name("function")
        .or_else(|| node.child_by_field_name("constructor"))
        .or_else(|| node.child_by_field_name("name"))?;
    terminal_identifier(callee, source).or_else(|| {
        let raw = callee.utf8_text(source).ok()?.trim();
        (!raw.is_empty() && raw.len() <= 80).then(|| raw.to_owned())
    })
}

fn normalized_dependency(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn javascript_workflow(node: Node<'_>, source: &[u8]) -> Option<ParsedWorkflow> {
    if node.kind() != "call_expression" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    let callee = function.utf8_text(source).ok()?.trim();
    let method = callee.rsplit('.').next()?.to_ascii_lowercase();
    if !HTTP_METHODS.contains(&method.as_str()) || !callee.contains('.') {
        return None;
    }
    let arguments = node.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let values: Vec<_> = arguments.named_children(&mut cursor).collect();
    let path = values
        .first()
        .and_then(|value| quoted_value(value, source))?;
    let handler = values
        .last()
        .filter(|_| values.len() > 1)
        .and_then(|value| terminal_identifier(*value, source));
    Some(ParsedWorkflow {
        framework: "express-compatible".to_owned(),
        method: method.to_ascii_uppercase(),
        path,
        handler,
        location: location(node),
    })
}

fn python_workflows(node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
    if node.kind() != "decorated_definition" {
        return Vec::new();
    }
    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    let handler = children
        .iter()
        .find(|child| child.kind() == "function_definition")
        .and_then(|function| function.child_by_field_name("name"))
        .and_then(|name| name.utf8_text(source).ok())
        .map(ToOwned::to_owned);
    let mut workflows = Vec::new();
    for decorator in children
        .into_iter()
        .filter(|child| child.kind() == "decorator")
    {
        let Ok(raw) = decorator.utf8_text(source) else {
            continue;
        };
        let Some(open) = raw.find('(') else {
            continue;
        };
        let decorator_name = raw[..open].trim().trim_start_matches('@');
        let method_name = decorator_name.rsplit('.').next().unwrap_or_default();
        let strings = quoted_strings(&raw[open..]);
        let Some(path) = strings.first().cloned() else {
            continue;
        };
        if method_name == "route" {
            let methods = strings[1..]
                .iter()
                .filter(|value| is_http_method(value))
                .cloned()
                .collect::<Vec<_>>();
            for method in if methods.is_empty() {
                vec!["GET".to_owned()]
            } else {
                methods
            } {
                workflows.push(ParsedWorkflow {
                    framework: "flask-compatible".to_owned(),
                    method: method.to_ascii_uppercase(),
                    path: path.clone(),
                    handler: handler.clone(),
                    location: location(decorator),
                });
            }
        } else if HTTP_METHODS.contains(&method_name) {
            workflows.push(ParsedWorkflow {
                framework: "fastapi-compatible".to_owned(),
                method: method_name.to_ascii_uppercase(),
                path,
                handler: handler.clone(),
                location: location(decorator),
            });
        }
    }
    workflows
}

fn php_workflows(node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
    let Ok(raw) = node.utf8_text(source) else {
        return Vec::new();
    };
    if node.kind() == "scoped_call_expression"
        && let Some(route_start) = raw.find("Route::")
    {
        let after_route = &raw[route_start + "Route::".len()..];
        let method = after_route
            .split(['(', ':'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let strings = quoted_strings(after_route);
        if HTTP_METHODS.contains(&method.as_str())
            && let Some(path) = strings.first()
        {
            return vec![ParsedWorkflow {
                framework: "laravel".to_owned(),
                method: method.to_ascii_uppercase(),
                path: path.clone(),
                handler: strings.get(1).cloned(),
                location: location(node),
            }];
        }
    }
    if node.kind() == "method_declaration" && raw.contains("#[Route(") {
        let strings = quoted_strings(raw);
        if let Some(path) = strings.first() {
            let method = strings
                .iter()
                .skip(1)
                .find(|value| is_http_method(value))
                .cloned()
                .unwrap_or_else(|| "GET".to_owned());
            let handler = node
                .child_by_field_name("name")
                .and_then(|name| name.utf8_text(source).ok())
                .map(ToOwned::to_owned);
            return vec![ParsedWorkflow {
                framework: "symfony".to_owned(),
                method: method.to_ascii_uppercase(),
                path: path.clone(),
                handler,
                location: location(node),
            }];
        }
    }
    Vec::new()
}

fn rust_workflows(node: Node<'_>, source: &[u8]) -> Vec<ParsedWorkflow> {
    let Ok(raw) = node.utf8_text(source) else {
        return Vec::new();
    };
    if node.kind() == "attribute_item" {
        for method in HTTP_METHODS {
            let marker = format!("#[{method}(");
            if raw.contains(&marker)
                && let Some(path) = quoted_strings(raw).first()
            {
                let handler = node
                    .next_named_sibling()
                    .filter(|sibling| sibling.kind() == "function_item")
                    .and_then(|function| function.child_by_field_name("name"))
                    .and_then(|name| name.utf8_text(source).ok())
                    .map(ToOwned::to_owned);
                return vec![ParsedWorkflow {
                    framework: "actix-web".to_owned(),
                    method: method.to_ascii_uppercase(),
                    path: path.clone(),
                    handler,
                    location: location(node),
                }];
            }
        }
    }
    if node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .and_then(|function| terminal_identifier(function, source))
            .as_deref()
            == Some("route")
        && let Some(arguments) = node.child_by_field_name("arguments")
    {
        let mut cursor = arguments.walk();
        let values = arguments.named_children(&mut cursor).collect::<Vec<_>>();
        let Some(path) = values.first().and_then(|value| quoted_value(value, source)) else {
            return Vec::new();
        };
        let Some(router) = values.get(1) else {
            return Vec::new();
        };
        let Some(method) = router
            .child_by_field_name("function")
            .and_then(|function| terminal_identifier(function, source))
            .filter(|method| HTTP_METHODS.contains(&method.as_str()))
        else {
            return Vec::new();
        };
        let handler = router
            .child_by_field_name("arguments")
            .and_then(|arguments| {
                let mut cursor = arguments.walk();
                arguments.named_children(&mut cursor).next()
            })
            .and_then(|handler| terminal_identifier(handler, source));
        return vec![ParsedWorkflow {
            framework: "axum-compatible".to_owned(),
            method: method.to_ascii_uppercase(),
            path,
            handler,
            location: location(node),
        }];
    }
    Vec::new()
}
