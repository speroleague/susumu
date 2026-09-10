use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use super::types::ReviewPacketStored;

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
    // Branding customizes the default (dark) palette. The built-in light theme
    // — `@media (prefers-color-scheme: light)` and the manual `[data-theme=light]`
    // switch — has higher selector specificity and keeps its tuned values, so a
    // dark-tuned brand palette can never bleed into the light theme.
    format!(":root{{{declarations}}}")
}

#[cfg(test)]
pub(crate) fn review_portal_html(packet: &ReviewPacketStored) -> Result<String> {
    review_portal_html_with_config(packet, &PortalConfig::default())
}

pub(crate) fn review_portal_html_with_config(
    packet: &ReviewPacketStored,
    config: &PortalConfig,
) -> Result<String> {
    let packet_json = serde_json::to_string(packet)
        .context("could not serialize packet for review portal")?
        .replace("</", "<\\/");
    let portal_title = config.title.as_deref().unwrap_or("Susumu Review");
    let portal_eyebrow = config.title.as_deref().unwrap_or("Susumu review packet");
    Ok(review_portal_template()
        .replace("__SUSUMU_PORTAL_TITLE__", &html_escape(portal_title))
        .replace("__SUSUMU_PORTAL_EYEBROW__", &html_escape(portal_eyebrow))
        .replace("__SUSUMU_PORTAL_THEME__", &config_style(config))
        .replace(
            "__SUSUMU_REVIEW_TITLE__",
            &html_escape(&packet.project.name),
        )
        .replace("__SUSUMU_REVIEW_DATA__", &packet_json))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// The `ICONS` map in the template embeds path data from Phosphor Icons
// (https://phosphoricons.com), MIT licensed, so the exported portal stays a
// single self-contained file with no icon font or network request.
const REVIEW_PORTAL_TEMPLATE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>__SUSUMU_PORTAL_TITLE__ &middot; __SUSUMU_REVIEW_TITLE__</title>
<script>try{var t=localStorage.getItem('susumu-theme');if(t==='light'||t==='dark')document.documentElement.dataset.theme=t;}catch(e){}</script>
<style>
:root{color-scheme:light dark;--tint:255,255,255;--bg:#11131a;--panel:#1a1f2b;--panel2:#202638;--text:#e8e2d7;--muted:#aaa292;--line:#363b49;--accent:#9eb7a0;--accent2:#aaa2bf;--bad:#cc8e8a;--warn:#c8aa72;--ok:#91ad86;--detail:#d5ccbd;--glow:#282f3f;--shadow:rgba(0,0,0,.22);--btn:#242c3f;--field:#171b26;--code:#131821;--code-line:#32394a;--ln:#777f8f;--tag:#252b3b;--tag-fg:#ddd6ca;--toolbar-a:rgba(17,19,26,.98);--toolbar-b:rgba(17,19,26,.78)}
@media(prefers-color-scheme:light){:root:not([data-theme]){--tint:22,26,34;--bg:#f6f5f2;--panel:#ffffff;--panel2:#eef0f4;--text:#23272f;--muted:#5c6270;--line:#d9dce3;--accent:#4f7a5a;--accent2:#6b6486;--bad:#b4524d;--warn:#8a6415;--ok:#4f7a5a;--detail:#3a3f4a;--glow:#ffffff;--shadow:rgba(20,24,33,.12);--btn:#eef0f4;--field:#ffffff;--code:#131821;--code-line:#32394a;--ln:#8b93a5;--tag:#e6e8ee;--tag-fg:#3a3f4a;--toolbar-a:rgba(246,245,242,.98);--toolbar-b:rgba(246,245,242,.80)}}
:root[data-theme="light"]{--tint:22,26,34;--bg:#f6f5f2;--panel:#ffffff;--panel2:#eef0f4;--text:#23272f;--muted:#5c6270;--line:#d9dce3;--accent:#4f7a5a;--accent2:#6b6486;--bad:#b4524d;--warn:#8a6415;--ok:#4f7a5a;--detail:#3a3f4a;--glow:#ffffff;--shadow:rgba(20,24,33,.12);--btn:#eef0f4;--field:#ffffff;--code:#131821;--code-line:#32394a;--ln:#8b93a5;--tag:#e6e8ee;--tag-fg:#3a3f4a;--toolbar-a:rgba(246,245,242,.98);--toolbar-b:rgba(246,245,242,.80)}
__SUSUMU_PORTAL_THEME__
*{box-sizing:border-box}body{margin:0;font-family:Inter,ui-sans-serif,system-ui,-apple-system,Segoe UI,sans-serif;background:radial-gradient(circle at 20% -10%,var(--glow) 0,var(--bg) 38%),var(--bg);color:var(--text)}
.shell{max-width:1220px;margin:0 auto;padding:40px 22px 70px}.hero{display:grid;grid-template-columns:1.4fr .8fr;gap:20px;align-items:stretch}.card{min-width:0;max-width:100%;overflow:hidden;background:linear-gradient(180deg,rgba(var(--tint),.045),rgba(var(--tint),.02)),var(--panel);border:1px solid var(--line);border-radius:24px;box-shadow:0 24px 70px var(--shadow);padding:24px;backdrop-filter:blur(12px)}
.eyebrow{color:var(--accent);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{font-size:clamp(34px,6vw,68px);line-height:.94;margin:12px 0}.sub{color:var(--muted);font-size:16px;line-height:1.6}.pill{display:inline-flex;gap:8px;align-items:center;border:1px solid var(--line);border-radius:999px;padding:7px 11px;color:var(--muted);font-size:13px;margin:4px 4px 0 0}.pill strong{color:var(--text)}
.result{font-size:28px;font-weight:850}.failed{color:var(--bad)}.passed{color:var(--ok)}.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-top:18px}.metric{background:rgba(var(--tint),.045);border:1px solid var(--line);border-radius:18px;padding:16px}.metric b{display:block;font-size:28px}.metric span{color:var(--muted);font-size:13px}
.toolbar{position:sticky;top:0;z-index:3;margin:26px -8px 20px;padding:10px 8px;display:flex;flex-wrap:wrap;align-items:center;gap:2px;background:linear-gradient(180deg,var(--toolbar-a),var(--toolbar-b));backdrop-filter:blur(12px)}#tabs{display:contents}button{appearance:none;border:1px solid var(--line);border-radius:999px;background:var(--btn);color:var(--text);font:inherit;font-size:14px;font-weight:600;padding:10px 14px;margin:4px;cursor:pointer;transition:.18s ease}button:hover{border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.12);transform:translateY(-1px)}button.active{border-color:var(--accent);color:var(--text);background:color-mix(in srgb,var(--accent) 26%,var(--btn));font-weight:750}.theme-toggle{margin-left:auto;display:inline-flex;align-items:center;gap:6px}.theme-toggle svg,button svg{width:16px;height:16px;flex:none;vertical-align:-3px}.tab-btn{display:inline-flex;align-items:center;gap:7px}
.section{display:none;animation:rise .28s ease}.section.active{display:block}@keyframes rise{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:none}}h2{font-size:28px;margin:0 0 14px}.list{display:grid;gap:12px;min-width:0}.item{min-width:0;overflow-wrap:anywhere;border:1px solid var(--line);border-radius:18px;background:rgba(var(--tint),.03);padding:16px}.item.clickable{cursor:pointer;transition:.18s ease}.item.clickable:hover,.item.selected{border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.11);transform:translateY(-1px)}.item h3{margin:0 0 8px;font-size:17px}.meta{color:var(--muted);font-size:13px;line-height:1.5}.detail{color:var(--detail);line-height:1.55}.tag{display:inline-flex;align-items:center;gap:5px;border-radius:999px;padding:4px 9px;margin-right:6px;font-size:12px;font-weight:600;background:var(--tag);color:var(--tag-fg)}.tag svg{width:13px;height:13px;flex:none}.critical,.tag.critical,.tag.failed{background:color-mix(in srgb,var(--bad) 24%,var(--panel));color:var(--text)}.warning,.tag.warning{background:color-mix(in srgb,var(--warn) 24%,var(--panel));color:var(--text)}.attention,.tag.attention{background:color-mix(in srgb,var(--accent2) 24%,var(--panel));color:var(--text)}.tag.passed,.tag.ok{background:color-mix(in srgb,var(--ok) 24%,var(--panel));color:var(--text)}.workflow-score{font-size:24px;color:var(--accent);font-weight:850}.cols{display:grid;grid-template-columns:1fr 1fr;gap:16px}.workflow-layout{display:grid;grid-template-columns:minmax(280px,.8fr) minmax(0,1.2fr);gap:16px;align-items:start;min-width:0;max-width:100%}.workflow-layout>*{min-width:0}.detail-pane{position:sticky;top:98px;align-self:start;min-width:0;max-width:100%;overflow:hidden}.traceability-layout{height:calc(100vh - 180px);min-height:540px;align-items:stretch}.traceability-list,.traceability-detail{min-width:0;max-width:100%;min-height:0;overflow:auto;overscroll-behavior:contain;padding:8px 6px 0 0}.traceability-detail{position:static;align-self:stretch}.mini{display:grid;gap:8px;min-width:0}.mini .item{padding:12px}.ladder{display:grid;gap:10px;margin:10px 0 16px}.ladder-step{position:relative;border:1px solid var(--line);border-radius:16px;background:rgba(var(--tint),.03);padding:13px 14px 13px 46px}.ladder-step:before{content:'';position:absolute;left:17px;top:18px;width:12px;height:12px;border-radius:999px;background:var(--muted);box-shadow:0 0 0 5px rgba(170,162,146,.09)}.ladder-step:after{content:'';position:absolute;left:22px;top:36px;bottom:-18px;width:2px;background:var(--line)}.ladder-step:last-child:after{display:none}.ladder-step.good{border-color:rgba(145,173,134,.45)}.ladder-step.good:before{background:var(--ok);box-shadow:0 0 0 5px rgba(145,173,134,.12)}.ladder-step.warn{border-color:rgba(200,170,114,.48)}.ladder-step.warn:before{background:var(--warn);box-shadow:0 0 0 5px rgba(200,170,114,.12)}.ladder-step.bad{border-color:rgba(204,142,138,.5)}.ladder-step.bad:before{background:var(--bad);box-shadow:0 0 0 5px rgba(204,142,138,.12)}.ladder-label{display:block;color:var(--muted);font-size:12px;font-weight:800;letter-spacing:.08em;text-transform:uppercase}.ladder-step strong{display:block;margin-top:3px}.ladder-step small{display:block;color:var(--detail);line-height:1.45;margin-top:4px}.next-action{border-color:rgba(158,183,160,.45);background:linear-gradient(135deg,rgba(158,183,160,.12),rgba(170,162,191,.08))}.search{width:100%;border:1px solid var(--line);border-radius:16px;background:var(--field);color:var(--text);font:inherit;font-size:14px;padding:13px 15px;margin:0 0 14px}.search::placeholder{color:var(--muted);opacity:1}.search:focus{outline:none;border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.14)}.search option{background:var(--panel);color:var(--text)}.empty{color:var(--muted);border:1px dashed var(--line);border-radius:18px;padding:22px;text-align:center}.code{max-width:100%;overflow:auto;background:var(--code);border:1px solid var(--code-line);border-radius:16px;padding:12px;font:13px/1.55 ui-monospace,SFMono-Regular,Consolas,Menlo,monospace}.code-line{display:grid;grid-template-columns:64px minmax(0,1fr);min-width:max-content}.code-line.mark{background:rgba(158,183,160,.09);border-left:3px solid var(--accent)}.ln{color:var(--ln);text-align:right;padding-right:14px;user-select:none}.src{white-space:pre}
@media(max-width:850px){.hero,.cols,.workflow-layout{grid-template-columns:1fr}.grid{grid-template-columns:repeat(2,1fr)}.detail-pane{position:static}.traceability-layout{height:auto;min-height:0}.traceability-list,.traceability-detail{overflow:visible;padding-right:0}}
</style>
</head>
<body>
<div class="shell">
  <header class="hero">
    <div class="card">
      <div class="eyebrow">__SUSUMU_PORTAL_EYEBROW__</div>
      <h1 id="projectName"></h1>
      <p class="sub" id="projectSub"></p>
      <div id="pills"></div>
    </div>
    <div class="card">
      <div class="eyebrow">Current result</div>
      <div id="result" class="result"></div>
      <p class="sub" id="resultReason"></p>
      <div class="grid">
        <div class="metric"><b id="critical"></b><span>critical</span></div>
        <div class="metric"><b id="warning"></b><span>warnings</span></div>
        <div class="metric"><b id="attention"></b><span>attention</span></div>
        <div class="metric"><b id="workflows"></b><span>workflows</span></div>
      </div>
    </div>
  </header>
  <nav class="toolbar"><span id="tabs"></span><button id="themeToggle" class="theme-toggle" type="button" aria-label="Switch between light and dark theme"></button></nav>
  <input class="search" id="search" placeholder="Filter visible section&hellip;">
  <main id="sections"></main>
</div>
<script>
const packet = __SUSUMU_REVIEW_DATA__;
const $ = (id) => document.getElementById(id);
const esc = (v) => String(v ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const list = (items, render, empty='Nothing here yet.') => items && items.length ? `<div class="list">${items.map(render).join('')}</div>` : `<div class="empty">${empty}</div>`;
const severity = (s) => s === 'critical' ? 'critical' : s === 'warning' ? 'warning' : 'attention';
const ICONS = {
 sun:'<path d="M120,40V16a8,8,0,0,1,16,0V40a8,8,0,0,1-16,0Zm72,88a64,64,0,1,1-64-64A64.07,64.07,0,0,1,192,128Zm-16,0a48,48,0,1,0-48,48A48.05,48.05,0,0,0,176,128ZM58.34,69.66A8,8,0,0,0,69.66,58.34l-16-16A8,8,0,0,0,42.34,53.66Zm0,116.68-16,16a8,8,0,0,0,11.32,11.32l16-16a8,8,0,0,0-11.32-11.32ZM192,72a8,8,0,0,0,5.66-2.34l16-16a8,8,0,0,0-11.32-11.32l-16,16A8,8,0,0,0,192,72Zm5.66,114.34a8,8,0,0,0-11.32,11.32l16,16a8,8,0,0,0,11.32-11.32ZM48,128a8,8,0,0,0-8-8H16a8,8,0,0,0,0,16H40A8,8,0,0,0,48,128Zm80,80a8,8,0,0,0-8,8v24a8,8,0,0,0,16,0V216A8,8,0,0,0,128,208Zm112-88H216a8,8,0,0,0,0,16h24a8,8,0,0,0,0-16Z"/>',
 moon:'<path d="M233.54,142.23a8,8,0,0,0-8-2,88.08,88.08,0,0,1-109.8-109.8,8,8,0,0,0-10-10,104.84,104.84,0,0,0-52.91,37A104,104,0,0,0,136,224a103.09,103.09,0,0,0,62.52-20.88,104.84,104.84,0,0,0,37-52.91A8,8,0,0,0,233.54,142.23ZM188.9,190.34A88,88,0,0,1,65.66,67.11a89,89,0,0,1,31.4-26A106,106,0,0,0,96,56,104.11,104.11,0,0,0,200,160a106,106,0,0,0,14.92-1.06A89,89,0,0,1,188.9,190.34Z"/>',
 warning:'<path d="M236.8,188.09,149.35,36.22h0a24.76,24.76,0,0,0-42.7,0L19.2,188.09a23.51,23.51,0,0,0,0,23.72A24.35,24.35,0,0,0,40.55,224h174.9a24.35,24.35,0,0,0,21.33-12.19A23.51,23.51,0,0,0,236.8,188.09ZM222.93,203.8a8.5,8.5,0,0,1-7.48,4.2H40.55a8.5,8.5,0,0,1-7.48-4.2,7.59,7.59,0,0,1,0-7.72L120.52,44.21a8.75,8.75,0,0,1,15,0l87.45,151.87A7.59,7.59,0,0,1,222.93,203.8ZM120,144V104a8,8,0,0,1,16,0v40a8,8,0,0,1-16,0Zm20,36a12,12,0,1,1-12-12A12,12,0,0,1,140,180Z"/>',
 house:'<path d="M219.31,108.68l-80-80a16,16,0,0,0-22.62,0l-80,80A15.87,15.87,0,0,0,32,120v96a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V160h32v56a8,8,0,0,0,8,8h64a8,8,0,0,0,8-8V120A15.87,15.87,0,0,0,219.31,108.68ZM208,208H160V152a8,8,0,0,0-8-8H104a8,8,0,0,0-8,8v56H48V120l80-80,80,80Z"/>',
 gauge:'<path d="M207.06,72.67A111.24,111.24,0,0,0,128,40h-.4C66.07,40.21,16,91,16,153.13V176a16,16,0,0,0,16,16H224a16,16,0,0,0,16-16V152A111.25,111.25,0,0,0,207.06,72.67ZM224,176H119.71l54.76-75.3a8,8,0,0,0-12.94-9.42L99.92,176H32V153.13c0-3.08.15-6.12.43-9.13H56a8,8,0,0,0,0-16H35.27c10.32-38.86,44-68.24,84.73-71.66V80a8,8,0,0,0,16,0V56.33A96.14,96.14,0,0,1,221,128H200a8,8,0,0,0,0,16h23.67c.21,2.65.33,5.31.33,8Z"/>',
 clipboard:'<path d="M168,152a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,152Zm-8-40H96a8,8,0,0,0,0,16h64a8,8,0,0,0,0-16Zm56-64V216a16,16,0,0,1-16,16H56a16,16,0,0,1-16-16V48A16,16,0,0,1,56,32H92.26a47.92,47.92,0,0,1,71.48,0H200A16,16,0,0,1,216,48ZM96,64h64a32,32,0,0,0-64,0ZM200,48H173.25A47.93,47.93,0,0,1,176,64v8a8,8,0,0,1-8,8H88a8,8,0,0,1-8-8V64a47.93,47.93,0,0,1,2.75-16H56V216H200Z"/>',
 chats:'<path d="M232.07,186.76a80,80,0,0,0-62.5-114.17A80,80,0,1,0,23.93,138.76l-7.27,24.71a16,16,0,0,0,19.87,19.87l24.71-7.27a80.39,80.39,0,0,0,25.18,7.35,80,80,0,0,0,108.34,40.65l24.71,7.27a16,16,0,0,0,19.87-19.86ZM62,159.5a8.28,8.28,0,0,0-2.26.32L32,168l8.17-27.76a8,8,0,0,0-.63-6,64,64,0,1,1,26.26,26.26A8,8,0,0,0,62,159.5Zm153.79,28.73L224,216l-27.76-8.17a8,8,0,0,0-6,.63,64.05,64.05,0,0,1-85.87-24.88A79.93,79.93,0,0,0,174.7,89.71a64,64,0,0,1,41.75,92.48A8,8,0,0,0,215.82,188.23Z"/>',
 flow:'<path d="M245.66,74.34l-32-32a8,8,0,0,0-11.32,11.32L220.69,72H208c-49.33,0-61.05,28.12-71.38,52.92-9.38,22.51-16.92,40.59-49.48,42.84a40,40,0,1,0,.1,16c43.26-2.65,54.34-29.15,64.14-52.69C161.41,107,169.33,88,208,88h12.69l-18.35,18.34a8,8,0,0,0,11.32,11.32l32-32A8,8,0,0,0,245.66,74.34ZM48,200a24,24,0,1,1,24-24A24,24,0,0,1,48,200Z"/>',
 tree:'<path d="M160,112h48a16,16,0,0,0,16-16V48a16,16,0,0,0-16-16H160a16,16,0,0,0-16,16V64H128a24,24,0,0,0-24,24v32H72v-8A16,16,0,0,0,56,96H24A16,16,0,0,0,8,112v32a16,16,0,0,0,16,16H56a16,16,0,0,0,16-16v-8h32v32a24,24,0,0,0,24,24h16v16a16,16,0,0,0,16,16h48a16,16,0,0,0,16-16V160a16,16,0,0,0-16-16H160a16,16,0,0,0-16,16v16H128a8,8,0,0,1-8-8V88a8,8,0,0,1,8-8h16V96A16,16,0,0,0,160,112ZM56,144H24V112H56v32Zm104,16h48v48H160Zm0-112h48V96H160Z"/>',
 code:'<path d="M69.12,94.15,28.5,128l40.62,33.85a8,8,0,1,1-10.24,12.29l-48-40a8,8,0,0,1,0-12.29l48-40a8,8,0,0,1,10.24,12.3Zm176,27.7-48-40a8,8,0,1,0-10.24,12.3L227.5,128l-40.62,33.85a8,8,0,1,0,10.24,12.29l48-40a8,8,0,0,0,0-12.29ZM162.73,32.48a8,8,0,0,0-10.25,4.79l-64,176a8,8,0,0,0,4.79,10.26A8.14,8.14,0,0,0,96,224a8,8,0,0,0,7.52-5.27l64-176A8,8,0,0,0,162.73,32.48Z"/>',
 checks:'<path d="M224,128a8,8,0,0,1-8,8H128a8,8,0,0,1,0-16h88A8,8,0,0,1,224,128ZM128,72h88a8,8,0,0,0,0-16H128a8,8,0,0,0,0,16Zm88,112H128a8,8,0,0,0,0,16h88a8,8,0,0,0,0-16ZM82.34,42.34,56,68.69,45.66,58.34A8,8,0,0,0,34.34,69.66l16,16a8,8,0,0,0,11.32,0l32-32A8,8,0,0,0,82.34,42.34Zm0,64L56,132.69,45.66,122.34a8,8,0,0,0-11.32,11.32l16,16a8,8,0,0,0,11.32,0l32-32a8,8,0,0,0-11.32-11.32Zm0,64L56,196.69,45.66,186.34a8,8,0,0,0-11.32,11.32l16,16a8,8,0,0,0,11.32,0l32-32a8,8,0,0,0-11.32-11.32Z"/>',
 diamond:'<path d="M128,72a8,8,0,0,1,8,8v56a8,8,0,0,1-16,0V80A8,8,0,0,1,128,72ZM116,172a12,12,0,1,0,12-12A12,12,0,0,0,116,172Zm124-44a15.85,15.85,0,0,1-4.67,11.28l-96.05,96.06a16,16,0,0,1-22.56,0h0l-96-96.06a16,16,0,0,1,0-22.56l96.05-96.06a16,16,0,0,1,22.56,0l96.05,96.06A15.85,15.85,0,0,1,240,128Zm-16,0L128,32,32,128,128,224h0Z"/>',
 package:'<path d="M223.68,66.15,135.68,18a15.88,15.88,0,0,0-15.36,0l-88,48.17a16,16,0,0,0-8.32,14v95.64a16,16,0,0,0,8.32,14l88,48.17a15.88,15.88,0,0,0,15.36,0l88-48.17a16,16,0,0,0,8.32-14V80.18A16,16,0,0,0,223.68,66.15ZM128,32l80.34,44-29.77,16.3-80.35-44ZM128,120,47.66,76l33.9-18.56,80.34,44ZM40,90l80,43.78v85.79L40,175.82Zm176,85.78h0l-80,43.79V133.82l32-17.51V152a8,8,0,0,0,16,0V107.55L216,90v85.77Z"/>',
 arrow:'<path d="M221.66,133.66l-72,72a8,8,0,0,1-11.32-11.32L196.69,136H40a8,8,0,0,1,0-16H196.69L138.34,61.66a8,8,0,0,1,11.32-11.32l72,72A8,8,0,0,1,221.66,133.66Z"/>'
};
function icon(name){return `<svg class="ph" viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">${ICONS[name]||''}</svg>`}
function item(title, body, meta='', tags='', extra=''){return `<article class="item">${tags}<h3>${esc(title)}</h3>${meta?`<div class="meta">${meta}</div>`:''}<div class="detail">${esc(body)}</div>${extra}</article>`}
let selectedWorkflowId = null;
let selectedExpectationId = null;
const tabs = [
 ['overview','Overview','house'],
 ['readiness','Readiness','gauge'],
 ['review','Review','clipboard'],
 ['threads','Threads','chats'],
 ['workflows','Top workflows','flow'],
 ['traceability','Traceability','tree'],
 ['source','Source','code'],
 ['records','Records','checks'],
 ['dirty','Dirty/stale','diamond'],
 ['artifact','Artifact','package'],
 ['actions','Next actions','arrow']
];
function section(id,title,html){return `<section class="section" id="section-${id}"><div class="card"><h2>${title}</h2>${html}</div></section>`}
function tokenHtml(line){return line.tokens&&line.tokens.length?line.tokens.map(t=>`<span style="color:${esc(t.color)}">${esc(t.text)}</span>`).join(''):esc(line.text)}
function codePreviewBlock(p){return `<div class="code">${(p.lines||[]).map(line=>`<div class="code-line ${line.number>=p.highlight_start&&line.number<=p.highlight_end?'mark':''}"><span class="ln">${line.number}</span><span class="src">${tokenHtml(line)}</span></div>`).join('')}</div>`}
function codePreview(p){return `<article class="item"><h3>${esc(p.path)}</h3><div class="meta">${esc(p.language)} &middot; lines ${p.start_line}-${p.end_line} &middot; highlight ${p.highlight_start}-${p.highlight_end}</div>${codePreviewBlock(p)}</article>`}
function fileById(id){return (packet.artifact.files||[]).find(f=>f.id===id)}
function symbolById(id){return (packet.artifact.symbols||[]).find(s=>s.id===id)}
function previewForLocation(fileId,location){if(!fileId)return null;const previews=packet.source_previews||[];if(location){const exact=previews.find(p=>p.file_id===fileId&&p.highlight_start===location.start_line&&p.highlight_end===location.end_line);if(exact)return exact;}return previews.find(p=>p.file_id===fileId)||null}
function targetPreview(target,subject){if(!subject)return null;if(target==='workflow'){const w=workflowById(subject);return w?previewForLocation(w.file_id,w.location):null;}if(target==='symbol'){const s=symbolById(subject);return s?previewForLocation(s.file_id,s.location):null;}if(target==='file')return previewForLocation(subject,null);return null}
function sourcePreviewExtra(p){return p?`<div style="margin-top:12px">${codePreviewBlock(p)}</div>`:''}
function sourceMetaForPreview(p){return p?` &middot; source=${esc(p.path)}:${p.highlight_start}`:''}
function workflows(){return packet.artifact.workflows||[]}
function workflowById(id){return workflows().find(w=>w.id===id)}
function workflowSummary(id){return (packet.top_workflows||[]).find(w=>w.id===id)}
function workflowExpectations(id){return (packet.artifact.expectations||[]).filter(e=>e.target==='workflow'&&e.subject===id)}
function workflowVerifications(id){const ids=new Set(workflowExpectations(id).map(e=>e.id));return (packet.artifact.verifications||[]).filter(v=>ids.has(v.expectation_id))}
function workflowDecisions(id){return (packet.artifact.decisions||[]).filter(d=>d.target==='workflow'&&d.subject===id)}
function workflowWork(id){const ids=new Set(workflowExpectations(id).map(e=>e.id));return (packet.artifact.works||[]).filter(w=>(w.target==='workflow'&&w.subject===id)||(w.expectation_id&&ids.has(w.expectation_id)))}
function workflowPreview(id){const w=workflowById(id);return w?previewForLocation(w.file_id,w.location):null}
function workflowCard(w){const summary=workflowSummary(w.id)||{score:0,detail:'Workflow detected from scanner evidence.',expectations:workflowExpectations(w.id).length,verifications:workflowVerifications(w.id).length,work:workflowWork(w.id).length};return `<article class="item clickable ${w.id===selectedWorkflowId?'selected':''}" data-workflow-id="${esc(w.id)}"><div class="workflow-score">${summary.score}</div><h3>${esc(w.trigger)}</h3><div class="meta">${esc(w.id)} &middot; ${esc(w.framework)} &middot; expectations=${summary.expectations} &middot; verifications=${summary.verifications} &middot; work=${summary.work}</div><div class="detail">${esc(summary.detail)}</div></article>`}
function miniList(items,render,empty){return items&&items.length?`<div class="mini">${items.map(render).join('')}</div>`:`<div class="empty">${empty}</div>`}
function verificationItem(v){const e=expectationById(v.expectation_id);const p=e?targetPreview(e.target,e.subject):null;return item(`${v.status} verification`,v.detail,`${esc(v.id)} &middot; method=${esc(v.method)} &middot; evidence=${esc(v.evidence??'-')} &middot; basis=${esc(v.basis??'-')}${sourceMetaForPreview(p)}`,'',sourcePreviewExtra(p))}
function decisionItem(d){const p=targetPreview(d.target,d.subject);return item(d.title,d.detail,`${esc(d.id)} &middot; ${esc(d.status)} &middot; source=${esc(d.source)} &middot; basis=${esc(d.basis??'-')}${sourceMetaForPreview(p)}`,'',sourcePreviewExtra(p))}
function workItem(w){const p=targetPreview(w.target,w.subject);return item(w.title,w.detail,`${esc(w.id)} &middot; ${esc(w.kind)} &middot; ${esc(w.status)} &middot; evidence=${esc(w.evidence??'-')}${sourceMetaForPreview(p)}`,'',sourcePreviewExtra(p))}
function reviewThreadItem(r){const action=r.status==='open'?'This static portal is read-only; use the configured review service to change it.':`Thread is ${r.status}.`;return item(r.title,`${r.detail} ${action}`,`${esc(r.id)} &middot; ${esc(r.kind||'comment')} &middot; ${esc(r.status)} &middot; target=${esc(r.target)}${r.subject?`:${esc(r.subject)}`:''} &middot; anchor=${esc(r.anchor??'-')} &middot; owner=${esc(r.owner??'-')} &middot; parent=${esc(r.parent??'-')} &middot; source=${esc(r.source)}`)}
function reviewThreadRecords(){return packet.artifact.review_threads||[]}
function reviewThreadSearchText(r){return [r.id,r.title,r.detail,r.kind,r.status,r.target,r.subject,r.anchor,r.owner,r.parent,r.source].filter(Boolean).join(' ').toLowerCase()}
function reviewThreadMatches(r){const query=($('threadSearch')?.value||'').trim().toLowerCase();const owner=$('threadOwner')?.value||'';const status=$('threadStatus')?.value||'';return (!query||reviewThreadSearchText(r).includes(query))&&(!owner||(r.owner||'unassigned')===owner)&&(!status||r.status===status)}
function renderReviewThreads(){const threads=reviewThreadRecords();const filtered=threads.filter(reviewThreadMatches);const count=$('threadCount');const results=$('threadResults');if(count)count.textContent=`Showing ${filtered.length} of ${threads.length} review threads`;if(results)results.innerHTML=list(filtered,reviewThreadItem,'No review threads match these filters.')}
function bindReviewThreadFilters(){['threadSearch','threadOwner','threadStatus'].forEach(id=>$(id)?.addEventListener('input',renderReviewThreads));['threadOwner','threadStatus'].forEach(id=>$(id)?.addEventListener('change',renderReviewThreads))}
function reviewThreadsSection(){const threads=reviewThreadRecords();if(!threads.length)return '<div class="empty">No review threads recorded.</div>';const open=threads.filter(r=>r.status==='open');const owners=[...new Set(threads.map(r=>r.owner||'unassigned'))].sort();const workloadOwners=[...new Set(open.map(r=>r.owner||'unassigned'))].sort();const workload=workloadOwners.map(owner=>`<div class="metric"><b>${open.filter(r=>(r.owner||'unassigned')===owner).length}</b><span>${esc(owner)}</span></div>`).join('');const ownerOptions=['',...owners].map(owner=>`<option value="${esc(owner)}">${owner?esc(owner):'All owners'}</option>`).join('');const statusOptions=['','open','resolved','accepted','rejected'].map(status=>`<option value="${status}">${status?esc(status[0].toUpperCase()+status.slice(1)):'All statuses'}</option>`).join('');return `<div class="meta">Discussion is authored review context; it does not by itself verify an expectation or approve a decision.</div><h3>Open review workload</h3><div class="grid">${workload||'<div class="empty">No open review threads.</div>'}</div><h3>Find a discussion</h3><div class="cols"><input class="search" id="threadSearch" aria-label="Search review threads" placeholder="Search threads, owners, targets&hellip;"><select class="search" id="threadOwner" aria-label="Filter review threads by owner">${ownerOptions}</select><select class="search" id="threadStatus" aria-label="Filter review threads by status">${statusOptions}</select></div><div class="meta" id="threadCount"></div><div id="threadResults">${list(threads,reviewThreadItem)}</div>`}
function workflowDetail(id){const w=workflowById(id);if(!w)return `<div class="empty">Select a workflow to inspect its evidence.</div>`;const summary=workflowSummary(id);const preview=workflowPreview(id);return `<div class="item"><h3>${esc(w.trigger)}</h3><div class="meta">${esc(w.id)} &middot; ${esc(w.framework)} &middot; handler=${esc(w.handler??'-')} &middot; confidence=${esc(w.confidence)}</div><div class="detail">${esc(summary?.detail||'Workflow detected from scanner evidence.')}</div></div><h3>Linked expectations</h3>${miniList(workflowExpectations(id),e=>item(e.title,e.detail,`${esc(e.id)} &middot; ${esc(e.status)} &middot; source=${esc(e.source)}`),'No linked expectations.')}<h3>Linked verifications</h3>${miniList(workflowVerifications(id),verificationItem,'No linked verifications.')}<h3>Linked decisions</h3>${miniList(workflowDecisions(id),decisionItem,'No linked decisions.')}<h3>Linked work</h3>${miniList(workflowWork(id),workItem,'No linked work.')}<h3>Source evidence</h3>${preview?codePreview(preview):'<div class="empty">No source preview embedded for this workflow.</div>'}`}
function workflowsSection(){const first=workflows()[0]?.id;selectedWorkflowId=selectedWorkflowId||first;return `<div class="workflow-layout"><div>${list(workflows(),workflowCard,'No workflows detected.')}</div><aside class="detail-pane" id="workflowDetail">${workflowDetail(selectedWorkflowId)}</aside></div>`}
function expectations(){return packet.artifact.expectations||[]}
function expectationById(id){return expectations().find(e=>e.id===id)}
function expectationWorkflow(e){return e&&e.target==='workflow'?workflowById(e.subject):null}
function expectationVerifications(id){return (packet.artifact.verifications||[]).filter(v=>v.expectation_id===id)}
function expectationWork(id){return (packet.artifact.works||[]).filter(w=>w.expectation_id===id)}
function expectationDecisions(e){if(!e)return[];return (packet.artifact.decisions||[]).filter(d=>d.target===e.target&&(d.subject??null)===(e.subject??null))}
function expectationSupport(id){return (packet.expectation_support||[]).find(s=>s.expectation_id===id)}
function supportMeta(s){return s?`${esc(s.support_status)} &middot; posture=${esc(s.evidence_posture||'none')} &middot; target=${s.target_observed?'observed':'missing'} &middot; verifications=${s.verification.passed}/${s.verification.failed}/${s.verification.inconclusive} &middot; work=${s.work} &middot; decisions=${s.decisions}`:'support=unknown'}
function supportReasons(s){return s?miniList(s.reasons||[],r=>item(r,'','support reason'),'No support reasons recorded.'): '<div class="empty">No support summary embedded.</div>'}
function verificationTotal(s){return s?s.verification.passed+s.verification.failed+s.verification.inconclusive:0}
function expectationNextAction(e,s){if(!s)return 'Rebuild the review packet so Susumu can summarize this expectation.';if(s.verification.failed>0)return 'Review the failed verification before relying on this expectation.';if(!s.target_observed)return 'Find or reconnect the target this expectation is about.';if(s.verification.passed>0)return 'Verified: ready for review or business confidence.';if(s.work===0)return `Connect work with susumu git or susumu git link <commit> ${e.id}.`;if(verificationTotal(s)===0)return `Record verification with susumu verify ${e.id} --passed --method "<check>".`;if(s.verification.inconclusive>0&&s.verification.passed===0)return 'Resolve the inconclusive verification evidence.';return 'Review the support evidence and decide whether more verification is needed.'}
function ladderStep(label,value,tone,detail=''){return `<div class="ladder-step ${tone}"><span class="ladder-label">${esc(label)}</span><strong>${esc(value)}</strong>${detail?`<small>${esc(detail)}</small>`:''}</div>`}
function expectationLadder(e,s){if(!s)return '<div class="empty">No evidence ladder embedded for this expectation.</div>';const total=verificationTotal(s);const verificationDetail=`passed=${s.verification.passed}, failed=${s.verification.failed}, inconclusive=${s.verification.inconclusive}, posture=${s.evidence_posture||'none'}`;return `<div class="ladder" data-evidence-ladder="${esc(e.id)}">${ladderStep('Target observation',s.target_observed?'Target observed':'Target missing',s.target_observed?'good':'bad',`${s.target}${s.subject?':'+s.subject:''}`)}${ladderStep('Work support',s.work>0?`${s.work} linked work record(s)`:'No linked work yet',s.work>0?'good':'warn','Work says what changed for this expectation.')}${ladderStep('Verification evidence',total>0?`${total} verification record(s)`:'No verification yet',s.verification.failed>0?'bad':s.verification.passed>0?'good':'warn',verificationDetail)}${ladderStep('Decision context',s.decisions>0?`${s.decisions} decision record(s)`:'No decision context yet',s.decisions>0?'good':'warn','Decisions record judgment, exceptions, and business context.')}${ladderStep('Review status',s.support_status,s.verification.failed>0||!s.target_observed?'bad':s.verification.passed>0?'good':'warn',(s.reasons||[]).join('; '))}${(df=>df.length?ladderStep('Evidence freshness',`${df.length} record(s) recorded against changed evidence`,'warn',df.map(f=>f.detail).join(' | ')):'')(expectationDirtyFindings(e))}</div><article class="item next-action"><h3>Suggested next action</h3><div class="detail">${esc(expectationDirtyFindings(e).length?'Re-verify this expectation or record a decision that accepts the change: '+expectationDirtyFindings(e)[0].detail:expectationNextAction(e,s))}</div></article>`}
function expectationDirtyFindings(e){const ids=new Set([...expectationVerifications(e.id).map(v=>v.id),...expectationDecisions(e).map(d=>d.id)]);return (packet.artifact.findings||[]).filter(f=>dirtyFinding(f)&&ids.has(f.subject))}
function dirtyBadge(e){return expectationDirtyFindings(e).length?`<span class="tag warning" title="A linked verification or decision was recorded against changed evidence">${icon('warning')} evidence changed</span>`:''}
function expectationCard(e){const s=expectationSupport(e.id);return `<article class="item clickable ${e.id===selectedExpectationId?'selected':''}" data-expectation-id="${esc(e.id)}"><h3>${esc(e.title)} ${dirtyBadge(e)}</h3><div class="meta">${esc(e.id)} &middot; ${esc(e.status)} &middot; ${esc(e.target)}${e.subject?`:${esc(e.subject)}`:''} &middot; ${supportMeta(s)}</div><div class="detail">${esc(e.detail)}</div></article>`}
function expectationDetail(id){const e=expectationById(id);if(!e)return `<div class="empty">Select an expectation to inspect its traceability.</div>`;const workflow=expectationWorkflow(e);const preview=workflow?workflowPreview(workflow.id):targetPreview(e.target,e.subject);const s=expectationSupport(id);return `<div class="item"><h3>${esc(e.title)}</h3><div class="meta">${esc(e.id)} &middot; ${esc(e.status)} &middot; source=${esc(e.source)} &middot; target=${esc(e.target)}${e.subject?`:${esc(e.subject)}`:''}</div><div class="detail">${esc(e.detail)}</div></div><h3>Evidence ladder</h3>${expectationLadder(e,s)}<h3>Support summary</h3><div class="item"><h3>${esc(s?.support_status||'unknown')}</h3><div class="meta">${supportMeta(s)}</div></div><h3>Support reasons</h3>${supportReasons(s)}<h3>Workflow context</h3>${workflow?miniList([workflow],w=>item(w.trigger,`${esc(w.framework)} &middot; handler=${esc(w.handler??'-')} &middot; confidence=${esc(w.confidence)}`,w.id),'No workflow context.'): '<div class="empty">This expectation is not attached to a workflow.</div>'}<h3>Verifications</h3>${miniList(expectationVerifications(id),verificationItem,'No verification records.')}<h3>Work records</h3>${miniList(expectationWork(id),workItem,'No work records.')}<h3>Decisions on same target</h3>${miniList(expectationDecisions(e),decisionItem,'No decisions on this target.')}<h3>Source evidence</h3>${preview?codePreview(preview):'<div class="empty">No source preview embedded for this expectation.</div>'}`}
function readinessBucket(s){if(!s)return 'Unknown';if(s.verification.failed>0)return 'Failed verification';if(!s.target_observed)return 'Missing target';if(s.dirty&&s.verification.passed>0)return 'Verified, evidence changed';if(s.verification.passed>0)return 'Verified';if(s.work>0)return 'Has work, needs verification';return 'No linked work yet'}
function readinessTone(bucket){return bucket==='Verified'?'good':bucket==='Failed verification'||bucket==='Missing target'?'bad':'warn'}
function readinessItems(){const stored=packet.expectation_readiness||[];if(stored.length)return stored.map(r=>({id:r.expectation_id,title:r.title,label:r.label,next_action:r.next_action}));return expectations().map(e=>{const s=expectationSupport(e.id);return {id:e.id,title:e.title,label:readinessBucket(s),next_action:expectationNextAction(e,s)}})}
function readinessRow(r){const s=expectationSupport(r.id);return item(r.title,r.next_action,`${esc(r.id)} &middot; ${esc(r.label)} &middot; ${supportMeta(s)}`,`<span class="tag ${readinessTone(r.label)==='good'?'passed':readinessTone(r.label)==='bad'?'critical':'warning'}">${esc(r.label)}</span>`)}
function readinessSection(){const order=['Failed verification','Missing target','Verified, evidence changed','Has work, needs verification','No linked work yet','Verified','Unknown'];const rows=readinessItems();const metrics=order.map(label=>`<div class="metric"><b>${rows.filter(r=>r.label===label).length}</b><span>${esc(label)}</span></div>`).join('');return `<div class="grid">${metrics}</div><div class="list" style="margin-top:16px">${order.map(label=>{const items=rows.filter(r=>r.label===label).map(readinessRow).join('');return items?`<div><h3>${esc(label)}</h3><div class="mini">${items}</div></div>`:''}).join('')||'<div class="empty">No expectations authored yet.</div>'}</div>`}
function traceabilitySection(){const first=expectations()[0]?.id;selectedExpectationId=selectedExpectationId||first;return `<div class="workflow-layout traceability-layout"><div class="traceability-list">${list(expectations(),expectationCard,'No expectations authored yet.')}</div><aside class="detail-pane traceability-detail" id="expectationDetail">${expectationDetail(selectedExpectationId)}</aside></div>`}
function dirtyFinding(f){return ['SUS023','SUS033'].includes(f.rule_id)}
function staleFinding(f){return ['SUS011','SUS021','SUS031','SUS041','SUS043','SUS055'].includes(f.rule_id)}
function findingPreview(f){return previewForLocation(f.file_id,f.location)}
function findingCard(f){const p=findingPreview(f);return item(`${f.rule_id}: ${f.title}`,f.detail,`source=${esc(f.source)} &middot; subject=${esc(f.subject??'-')}${sourceMetaForPreview(p)}`,`<span class="tag ${severity(f.severity)}">${esc(f.severity)}</span>` ,sourcePreviewExtra(p))}
function dirtySection(){const findings=packet.artifact.findings||[];const dirty=findings.filter(dirtyFinding);const stale=findings.filter(staleFinding);return `<div class="cols"><div><h3>Dirty evidence</h3>${list(dirty,findingCard,'No changed verification or decision evidence detected.')}</div><div><h3>Stale or missing record targets</h3>${list(stale,findingCard,'No stale record targets detected.')}</div></div>`}
function systemTheme(){return window.matchMedia&&window.matchMedia('(prefers-color-scheme: light)').matches?'light':'dark';}
function activeTheme(){return document.documentElement.dataset.theme||systemTheme();}
function updateThemeToggle(){const btn=$('themeToggle');if(!btn)return;const next=activeTheme()==='light'?'dark':'light';btn.innerHTML=icon(next==='light'?'sun':'moon')+'<span>'+(next==='light'?'Light':'Dark')+'</span>';btn.title='Switch to '+next+' theme';}
function toggleTheme(){const next=activeTheme()==='light'?'dark':'light';document.documentElement.dataset.theme=next;try{localStorage.setItem('susumu-theme',next);}catch(e){}updateThemeToggle();}
function render(){
 $('projectName').textContent = packet.project.name;
 $('projectSub').textContent = packet.project.root;
 $('result').textContent = packet.result.status;
 $('result').classList.add(packet.result.failed ? 'failed' : 'passed');
 $('resultReason').textContent = packet.result.reason;
 $('critical').textContent = packet.review.critical;
 $('warning').textContent = packet.review.warning;
 $('attention').textContent = packet.review.attention;
 $('workflows').textContent = packet.evidence.workflows;
 $('pills').innerHTML = [
  ['schema',packet.schema_version],['created',packet.created_unix_seconds],['source',packet.source.input],
  ['files',packet.evidence.files],['flows',packet.evidence.flows],['findings',packet.evidence.findings]
 ].map(([k,v])=>`<span class="pill">${esc(k)} <strong>${esc(v)}</strong></span>`).join('');
 $('tabs').innerHTML = tabs.map(([id,label,ic],i)=>`<button class="tab-btn ${i===0?'active':''}" data-tab="${id}">${icon(ic)}<span>${label}</span></button>`).join('');
 $('sections').innerHTML = [
  section('overview','Overview', `<div class="grid">
    <div class="metric"><b>${packet.records.expectations}</b><span>expectations</span></div>
    <div class="metric"><b>${packet.records.verifications}</b><span>verifications</span></div>
    <div class="metric"><b>${packet.records.decisions}</b><span>decisions</span></div>
    <div class="metric"><b>${packet.records.work}</b><span>work records</span></div>
    <div class="metric"><b>${packet.records.review_threads}</b><span>review threads</span></div>
  </div><div class="cols" style="margin-top:16px"><div>${list(packet.caveats,a=>item('Caveat',a))}</div><div>${list(packet.next_actions,a=>item('Suggested action',a))}</div></div>`),
  section('readiness','Expectation readiness board', readinessSection()),
  section('review','Needs review', list(packet.review_items, r => item(r.title, r.detail, `source=${esc(r.source)}`, `<span class="tag ${severity(r.severity)}">${esc(r.severity)}</span>`), 'No review items derived.')),
  section('threads','Review threads', reviewThreadsSection()),
  section('workflows','Workflow evidence', workflowsSection()),
  section('traceability','Expectation traceability', traceabilitySection()),
  section('source','Source previews', list(packet.source_previews, codePreview, 'No source snippets were embedded. Create the review packet from a local project or artifact with readable source files.')),
  section('records','Records requiring follow-up', `<div class="cols"><div><h3>Expectations without verification</h3>${list(packet.expectations_without_verification, r => item(r.title, r.reason, `${esc(r.id)} &middot; ${esc(r.target)} &middot; source=${esc(r.source)}`), 'All expectations have verification records.')}</div><div><h3>Work needing verification</h3>${list(packet.work_needing_verification, r => item(r.title, r.reason, `${esc(r.id)} &middot; ${esc(r.target)} &middot; source=${esc(r.source)}`), 'No work records need verification.')}</div></div>`),
  section('dirty','Dirty and stale evidence', dirtySection()),
  section('artifact','Embedded artifact', `<div class="cols"><div><h3>Files</h3>${list(packet.artifact.files, f => item(f.path, `${f.language} &middot; ${f.lines} lines &middot; ${f.bytes} bytes`, f.id), 'No files.')}</div><div><h3>Workflows</h3>${list(packet.artifact.workflows, w => item(w.trigger, `${w.framework} &middot; handler=${w.handler ?? '-'} &middot; confidence=${w.confidence}`, w.id), 'No workflows.')}</div></div>`),
  section('actions','Next actions', list(packet.next_actions, a=>item('Action',a), 'No next actions.'))
 ].join('');
 document.querySelector('#section-overview').classList.add('active');
 document.querySelectorAll('[data-tab]').forEach(btn=>btn.addEventListener('click',()=>activate(btn.dataset.tab)));
 document.querySelectorAll('[data-workflow-id]').forEach(card=>card.addEventListener('click',()=>selectWorkflow(card.dataset.workflowId)));
 document.querySelectorAll('[data-expectation-id]').forEach(card=>card.addEventListener('click',()=>selectExpectation(card.dataset.expectationId)));
 bindReviewThreadFilters();
 renderReviewThreads();
 $('search').addEventListener('input', filter);
 $('themeToggle').addEventListener('click', toggleTheme);
 updateThemeToggle();
 if(window.matchMedia){window.matchMedia('(prefers-color-scheme: light)').addEventListener('change',updateThemeToggle);}
}
function selectWorkflow(id){selectedWorkflowId=id;document.querySelectorAll('[data-workflow-id]').forEach(card=>card.classList.toggle('selected',card.dataset.workflowId===id));$('workflowDetail').innerHTML=workflowDetail(id);}
function selectExpectation(id){selectedExpectationId=id;document.querySelectorAll('[data-expectation-id]').forEach(card=>card.classList.toggle('selected',card.dataset.expectationId===id));$('expectationDetail').innerHTML=expectationDetail(id);}
function activate(id){document.querySelectorAll('[data-tab]').forEach(b=>b.classList.toggle('active',b.dataset.tab===id));document.querySelectorAll('.section').forEach(s=>s.classList.toggle('active',s.id===`section-${id}`));$('search').value='';filter();}
function filter(){const q=$('search').value.toLowerCase();document.querySelectorAll('.section.active .item').forEach(el=>el.style.display=el.textContent.toLowerCase().includes(q)?'':'none');}
render();
</script>
</body>
</html>"#;

fn review_portal_template() -> &'static str {
    REVIEW_PORTAL_TEMPLATE
}
