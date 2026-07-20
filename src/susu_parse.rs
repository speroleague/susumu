use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::model::Location;

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
