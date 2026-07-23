use std::collections::HashMap;

use anyhow::{Context, Result, bail};

use crate::model::{Location, Work};

pub(crate) fn optional_id(value: &str) -> Option<String> {
    (value != "-").then(|| value.to_owned())
}

pub(crate) fn required<'a>(fields: &'a HashMap<String, String>, name: &str) -> Result<&'a str> {
    fields
        .get(name)
        .map(String::as_str)
        .with_context(|| format!("missing field {name}"))
}

pub(crate) fn parse_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    value
        .parse()
        .with_context(|| format!("invalid number for {field}: {value}"))
}

pub(crate) fn parse_location(values: &HashMap<String, String>) -> Result<Location> {
    let (start_line, start_column) = parse_point(required(values, "start")?)?;
    let (end_line, end_column) = parse_point(required(values, "end")?)?;
    Ok(Location {
        start_line,
        start_column,
        end_line,
        end_column,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Token {
    Atom(String),
    String(String),
    Equals,
    Arrow,
    Semicolon,
}

pub(crate) fn atom_at(tokens: &[Token], index: usize, label: &str) -> Result<String> {
    match tokens.get(index) {
        Some(Token::Atom(value) | Token::String(value)) => Ok(value.clone()),
        _ => bail!("missing or invalid {label}"),
    }
}

pub(crate) fn fields(tokens: &[Token], start: usize) -> Result<HashMap<String, String>> {
    let mut output = HashMap::new();
    let mut index = start;
    while index < tokens.len() {
        let key = match &tokens[index] {
            Token::Atom(value) => value.clone(),
            unexpected => bail!("expected field name, found {unexpected:?}"),
        };
        if tokens.get(index + 1) != Some(&Token::Equals) {
            bail!("expected = after field {key}");
        }
        let value = match tokens.get(index + 2) {
            Some(Token::Atom(value) | Token::String(value)) => value.clone(),
            unexpected => bail!("expected value after {key}=, found {unexpected:?}"),
        };
        output.insert(key, value);
        index += 3;
    }
    Ok(output)
}

pub(crate) fn tokenize(source: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            character if character.is_whitespace() => index += 1,
            '=' => {
                tokens.push(Token::Equals);
                index += 1;
            }
            ';' => {
                tokens.push(Token::Semicolon);
                index += 1;
            }
            '-' if chars.get(index + 1) == Some(&'>') => {
                tokens.push(Token::Arrow);
                index += 2;
            }
            '"' => {
                let start = index;
                index += 1;
                let mut escaped = false;
                while index < chars.len() {
                    let character = chars[index];
                    index += 1;
                    if escaped {
                        escaped = false;
                    } else if character == '\\' {
                        escaped = true;
                    } else if character == '"' {
                        break;
                    }
                }
                if chars.get(index.saturating_sub(1)) != Some(&'"') {
                    bail!("unterminated string in .susu file");
                }
                let raw: String = chars[start..index].iter().collect();
                tokens.push(Token::String(
                    serde_json::from_str(&raw).context("invalid string escape in .susu file")?,
                ));
            }
            _ => {
                let start = index;
                while index < chars.len() && !is_atom_boundary(&chars, index) {
                    index += 1;
                }
                let atom: String = chars[start..index].iter().collect();
                if atom.is_empty() {
                    bail!("unexpected character in .susu file");
                }
                tokens.push(Token::Atom(atom));
            }
        }
    }
    Ok(tokens)
}

fn is_atom_boundary(chars: &[char], index: usize) -> bool {
    chars[index].is_whitespace()
        || matches!(chars[index], '=' | ';' | '"')
        || (chars[index] == '-' && chars.get(index + 1) == Some(&'>'))
}

pub(crate) fn statements(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut output = Vec::new();
    let mut current = Vec::new();
    for token in tokens {
        if token == &Token::Semicolon {
            if !current.is_empty() {
                output.push(std::mem::take(&mut current));
            }
        } else {
            current.push(token.clone());
        }
    }
    if !current.is_empty() {
        output.push(current);
    }
    output
}

pub(crate) fn parse_work_statement(statement: &[Token]) -> Result<Work> {
    let id = atom_at(statement, 1, "work id")?;
    let values = fields(statement, 2)?;
    Ok(Work {
        id,
        target: required(&values, "target")?
            .parse()
            .map_err(|error: String| anyhow::anyhow!(error))?,
        subject: optional_id(required(&values, "subject")?),
        expectation_id: optional_id(required(&values, "expectation")?),
        kind: required(&values, "kind")?
            .parse()
            .map_err(|error: String| anyhow::anyhow!(error))?,
        status: required(&values, "status")?
            .parse()
            .map_err(|error: String| anyhow::anyhow!(error))?,
        source: required(&values, "source")?.to_owned(),
        evidence: optional_id(required(&values, "evidence")?),
        title: required(&values, "title")?.to_owned(),
        detail: required(&values, "detail")?.to_owned(),
    })
}

fn parse_point(value: &str) -> Result<(usize, usize)> {
    let (line, column) = value
        .split_once(':')
        .with_context(|| format!("invalid source point: {value}"))?;
    Ok((parse_number(line, "line")?, parse_number(column, "column")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_primitives_preserve_optional_ids_and_locations() {
        let fields = HashMap::from([
            ("start".to_owned(), "2:3".to_owned()),
            ("end".to_owned(), "4:5".to_owned()),
        ]);

        assert_eq!(optional_id("-"), None);
        assert_eq!(optional_id("s_main"), Some("s_main".to_owned()));
        assert_eq!(parse_location(&fields).unwrap().start_line, 2);
        assert!(required(&fields, "missing").is_err());
        assert!(parse_number::<usize>("not-a-number", "line").is_err());
    }
}
