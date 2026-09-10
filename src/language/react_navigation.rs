use tree_sitter::Node;

use crate::model::WorkflowKind;

use super::{ParsedWorkflow, location, quoted_value, terminal_identifier};

/// Detects client-side navigation targets declared as JSX: React Router
/// `<Route>` elements and React Navigation `<*.Screen>` elements, including
/// their nested (`element={<Users />}`) and reference (`component={Users}`)
/// handler forms. Object-configuration routers such as `createBrowserRouter`
/// stay a visible gap rather than a guess.
pub(super) fn workflow(node: Node<'_>, source: &[u8]) -> Option<ParsedWorkflow> {
    if !matches!(
        node.kind(),
        "jsx_opening_element" | "jsx_self_closing_element"
    ) {
        return None;
    }
    let element_name = node
        .child_by_field_name("name")?
        .utf8_text(source)
        .ok()?
        .trim()
        .to_owned();

    let attributes = jsx_attributes(node, source);
    let attribute = |key: &str| -> Option<Option<String>> {
        attributes
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.clone())
    };

    let (framework, method, path) = if element_name == "Route" || element_name.ends_with(".Route") {
        let path = match attribute("path") {
            Some(Some(path)) => path,
            _ if attribute("index").is_some() => "(index)".to_owned(),
            _ => return None,
        };
        ("react-router", "ROUTE", path)
    } else if element_name == "Screen" || element_name.ends_with(".Screen") {
        ("react-navigation", "SCREEN", attribute("name").flatten()?)
    } else {
        return None;
    };

    let handler = attribute("component")
        .flatten()
        .or_else(|| attribute("element").flatten())
        .or_else(|| attribute("getComponent").flatten());

    Some(ParsedWorkflow {
        kind: WorkflowKind::Navigation,
        framework: framework.to_owned(),
        method: method.to_owned(),
        path,
        handler,
        location: location(node),
    })
}

fn jsx_attributes(node: Node<'_>, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() == "jsx_attribute")
        .filter_map(|attribute| {
            let mut inner = attribute.walk();
            let parts: Vec<_> = attribute.named_children(&mut inner).collect();
            let name = parts.first()?.utf8_text(source).ok()?.trim().to_owned();
            let value = parts
                .get(1)
                .and_then(|value| jsx_attribute_value(*value, source));
            Some((name, value))
        })
        .collect()
}

fn jsx_attribute_value(node: Node<'_>, source: &[u8]) -> Option<String> {
    if node.kind() == "string" {
        return quoted_value(&node, source);
    }
    terminal_identifier(node, source)
}
