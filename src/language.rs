mod adapters;

use adapters::adapter_for;
use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use crate::model::{Language, Location, SymbolKind};

#[derive(Debug)]
pub(crate) struct ParsedFile {
    pub symbols: Vec<ParsedSymbol>,
    pub calls: Vec<ParsedCall>,
    pub dependencies: Vec<ParsedDependency>,
    pub workflows: Vec<ParsedWorkflow>,
    pub has_parse_errors: bool,
}

#[derive(Debug)]
pub(crate) struct ParsedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub entrypoint: bool,
}

#[derive(Debug)]
pub(crate) struct ParsedCall {
    pub caller: usize,
    pub name: String,
    pub location: Location,
}

#[derive(Debug)]
pub(crate) struct ParsedDependency {
    pub name: String,
    pub location: Location,
}

#[derive(Debug)]
pub(crate) struct ParsedWorkflow {
    pub framework: String,
    pub method: String,
    pub path: String,
    pub handler: Option<String>,
    pub location: Location,
}

pub(crate) fn parse_file(
    language: Language,
    source: &str,
    module_entrypoint: bool,
) -> Result<ParsedFile> {
    let adapter = adapter_for(language);
    let mut parser = Parser::new();
    let grammar = adapter.grammar();
    parser
        .set_language(&grammar)
        .context("could not load the Tree-sitter grammar")?;
    let tree = parser
        .parse(source, None)
        .context("Tree-sitter did not return a syntax tree")?;

    let mut parsed = ParsedFile {
        symbols: vec![ParsedSymbol {
            name: "<module>".to_owned(),
            kind: SymbolKind::Function,
            location: location(tree.root_node()),
            entrypoint: module_entrypoint,
        }],
        calls: Vec::new(),
        dependencies: Vec::new(),
        workflows: Vec::new(),
        has_parse_errors: tree.root_node().has_error(),
    };
    walk(tree.root_node(), source.as_bytes(), adapter, 0, &mut parsed);
    Ok(parsed)
}

fn walk(
    node: Node<'_>,
    source: &[u8],
    adapter: &dyn adapters::LanguageAdapter,
    caller: usize,
    parsed: &mut ParsedFile,
) {
    if adapter.is_dependency(node.kind())
        && let Ok(text) = node.utf8_text(source)
    {
        parsed.dependencies.push(ParsedDependency {
            name: adapter.normalize_dependency(text),
            location: location(node),
        });
    }

    let mut active_caller = caller;
    if let Some((name, kind)) = adapter.symbol(node, source) {
        let entrypoint = name == "main" || name == "__main__";
        active_caller = parsed.symbols.len();
        parsed.symbols.push(ParsedSymbol {
            name,
            kind,
            location: location(node),
            entrypoint,
        });
    }

    if adapter.is_call(node.kind())
        && let Some(name) = adapter.call_name(node, source)
    {
        parsed.calls.push(ParsedCall {
            caller: active_caller,
            name,
            location: location(node),
        });
    }

    parsed.workflows.extend(adapter.workflows(node, source));

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, adapter, active_caller, parsed);
    }
}

pub(super) fn terminal_identifier(node: Node<'_>, source: &[u8]) -> Option<String> {
    if matches!(
        node.kind(),
        "identifier" | "field_identifier" | "property_identifier" | "type_identifier"
    ) {
        return node.utf8_text(source).ok().map(ToOwned::to_owned);
    }

    let mut cursor = node.walk();
    let children: Vec<_> = node.named_children(&mut cursor).collect();
    children
        .into_iter()
        .rev()
        .find_map(|child| terminal_identifier(child, source))
}

pub(super) fn location(node: Node<'_>) -> Location {
    let start = node.start_position();
    let end = node.end_position();
    Location {
        start_line: start.row + 1,
        start_column: start.column + 1,
        end_line: end.row + 1,
        end_column: end.column + 1,
    }
}

pub(super) const HTTP_METHODS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "options", "head", "any",
];

pub(super) fn quoted_value(node: &Node<'_>, source: &[u8]) -> Option<String> {
    let raw = node.utf8_text(source).ok()?;
    quoted_strings(raw).into_iter().next()
}

pub(super) fn quoted_strings(source: &str) -> Vec<String> {
    let characters: Vec<char> = source.chars().collect();
    let mut output = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let quote = characters[index];
        if !matches!(quote, '\'' | '"' | '`') {
            index += 1;
            continue;
        }
        index += 1;
        let mut value = String::new();
        let mut escaped = false;
        while index < characters.len() {
            let character = characters[index];
            index += 1;
            if escaped {
                value.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                output.push(value);
                break;
            } else {
                value.push(character);
            }
        }
    }
    output
}

pub(super) fn is_http_method(value: &str) -> bool {
    HTTP_METHODS.contains(&value.to_ascii_lowercase().as_str())
}
