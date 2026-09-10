use tree_sitter::Node;

use crate::model::WorkflowKind;

use super::{ParsedWorkflow, location};

const MAX_SUBTREE_NODES: usize = 4000;
const MAX_EXPORT_DEPTH: usize = 6;

/// Detects a React component that is also a module entry point: a function or
/// arrow-assigned `const` that is exported at its declaration, is named in
/// `PascalCase`, and returns JSX. Component libraries with no router still get a
/// per-component workflow this way. Non-exported helper components and lowercase
/// render helpers stay ordinary function symbols.
pub(super) fn workflow(node: Node<'_>, source: &[u8]) -> Option<ParsedWorkflow> {
    let name = component_name(node, source)?;
    if !is_component_name(&name) || !is_exported(node) || !contains_jsx(node) {
        return None;
    }
    Some(ParsedWorkflow {
        kind: WorkflowKind::Component,
        framework: "react-component".to_owned(),
        method: "COMPONENT".to_owned(),
        path: name.clone(),
        handler: Some(name),
        location: location(node),
    })
}

fn component_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "function_declaration" | "generator_function_declaration" => node
            .child_by_field_name("name")?
            .utf8_text(source)
            .ok()
            .map(ToOwned::to_owned),
        "variable_declarator" => {
            let value = node.child_by_field_name("value")?;
            if !matches!(
                value.kind(),
                "arrow_function" | "function_expression" | "call_expression"
            ) {
                return None;
            }
            node.child_by_field_name("name")?
                .utf8_text(source)
                .ok()
                .map(ToOwned::to_owned)
        }
        _ => None,
    }
}

fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn is_exported(node: Node<'_>) -> bool {
    let mut current = node.parent();
    for _ in 0..MAX_EXPORT_DEPTH {
        let Some(ancestor) = current else {
            return false;
        };
        match ancestor.kind() {
            "export_statement" => return true,
            "program" | "statement_block" | "function_declaration" | "arrow_function" => {
                return false;
            }
            _ => current = ancestor.parent(),
        }
    }
    false
}

fn contains_jsx(node: Node<'_>) -> bool {
    let mut pending = vec![node];
    let mut visited = 0;
    while let Some(node) = pending.pop() {
        visited += 1;
        if visited > MAX_SUBTREE_NODES {
            return false;
        }
        if matches!(
            node.kind(),
            "jsx_element" | "jsx_self_closing_element" | "jsx_fragment"
        ) {
            return true;
        }
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
    false
}
