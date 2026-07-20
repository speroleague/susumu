use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::review_types::ReviewPacketStored;

pub(crate) const PORTAL_CONFIG_FILE: &str = "susumu.toml";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PortalConfig {
    pub(crate) title: Option<String>,
    pub(crate) css_vars: BTreeMap<String, String>,
}

pub(crate) fn handle_review_request(
    mut stream: TcpStream,
    html: &str,
    packet_json: &str,
) -> Result<()> {
    let mut buffer = [0_u8; 8192];
    let bytes = stream
        .read(&mut buffer)
        .context("could not read HTTP request")?;
    let request = String::from_utf8_lossy(&buffer[..bytes]);
    let Some(request_line) = request.lines().next() else {
        return write_http_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "missing request line",
        );
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or("/");
    let path = path.split('?').next().unwrap_or(path);
    if !matches!(method, "GET" | "HEAD") {
        return write_http_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "method not allowed",
        );
    }
    let (status, content_type, body) = match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", html),
        "/review.json" => ("200 OK", "application/json; charset=utf-8", packet_json),
        "/healthz" => ("200 OK", "text/plain; charset=utf-8", "ok"),
        _ => ("404 Not Found", "text/plain; charset=utf-8", "not found"),
    };
    if method == "HEAD" {
        write_http_head(&mut stream, status, content_type, body.len())
    } else {
        write_http_response(&mut stream, status, content_type, body)
    }
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &str,
) -> Result<()> {
    write_http_head(stream, status, content_type, body.len())?;
    stream
        .write_all(body.as_bytes())
        .context("could not write HTTP response body")
}

fn write_http_head(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    content_length: usize,
) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {content_length}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(head.as_bytes())
        .context("could not write HTTP response head")
}

pub(crate) fn load_for_target(target: &Path) -> Result<PortalConfig> {
    let config_path = if target.is_dir() {
        target.join(PORTAL_CONFIG_FILE)
    } else {
        target
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PORTAL_CONFIG_FILE)
    };
    read_config(&config_path)
}

pub(crate) fn load_for_packet(
    packet: &ReviewPacketStored,
    packet_path: &Path,
) -> Result<PortalConfig> {
    let project_root = PathBuf::from(&packet.project.root);
    let config_path = if project_root.is_dir() {
        project_root.join(PORTAL_CONFIG_FILE)
    } else {
        packet_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PORTAL_CONFIG_FILE)
    };
    read_config(&config_path)
}

fn read_config(path: &Path) -> Result<PortalConfig> {
    if !path.exists() {
        return Ok(PortalConfig::default());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    parse_portal_config(&source).with_context(|| format!("could not parse {}", path.display()))
}

pub(crate) fn parse_portal_config(source: &str) -> Result<PortalConfig> {
    let mut config = PortalConfig::default();
    let mut in_portal = false;
    for (line_index, raw_line) in source.lines().enumerate() {
        let line_number = line_index + 1;
        let line = strip_toml_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_portal = line == "[portal]";
            continue;
        }
        if !in_portal {
            continue;
        }
        let (key, raw_value) = line
            .split_once('=')
            .with_context(|| format!("portal config line {line_number} must use key = value"))?;
        let key = key.trim();
        let value = parse_portal_config_value(raw_value.trim())
            .with_context(|| format!("invalid portal config value on line {line_number}"))?;
        apply_portal_config_value(&mut config, key, value)
            .with_context(|| format!("invalid portal config key `{key}` on line {line_number}"))?;
    }
    Ok(config)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if quote == Some('"') => escaped = true,
            '"' | '\'' if quote == Some(character) => quote = None,
            '"' | '\'' if quote.is_none() => quote = Some(character),
            '#' if quote.is_none() => return &line[..index],
            _ => {}
        }
    }
    line
}

fn parse_portal_config_value(raw: &str) -> Result<String> {
    if raw.starts_with('"') {
        return serde_json::from_str(raw).context("double-quoted values must be valid strings");
    }
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        return Ok(raw[1..raw.len() - 1].to_owned());
    }
    Ok(raw.to_owned())
}

fn apply_portal_config_value(config: &mut PortalConfig, key: &str, value: String) -> Result<()> {
    match key {
        "title" => config.title = (!value.trim().is_empty()).then_some(value),
        "background" | "bg" => config.set_color("--bg", &value)?,
        "panel" => config.set_color("--panel", &value)?,
        "panel2" => config.set_color("--panel2", &value)?,
        "text" => config.set_color("--text", &value)?,
        "muted" => config.set_color("--muted", &value)?,
        "line" => config.set_color("--line", &value)?,
        "accent" => config.set_color("--accent", &value)?,
        "accent2" => config.set_color("--accent2", &value)?,
        "bad" => config.set_color("--bad", &value)?,
        "warn" => config.set_color("--warn", &value)?,
        "ok" => config.set_color("--ok", &value)?,
        _ => bail!("supported keys are title and portal color names"),
    }
    Ok(())
}

impl PortalConfig {
    fn set_color(&mut self, css_var: &str, value: &str) -> Result<()> {
        if !is_hex_color(value) {
            bail!("portal colors must be #rgb or #rrggbb hex values");
        }
        self.css_vars.insert(css_var.to_owned(), value.to_owned());
        Ok(())
    }
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|character| character.is_ascii_hexdigit())
}

pub(crate) fn config_style(config: &PortalConfig) -> String {
    if config.css_vars.is_empty() {
        return String::new();
    }
    let declarations = config
        .css_vars
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(":root{{{declarations}}}")
}
