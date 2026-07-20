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

#[allow(clippy::too_many_lines)]
fn review_portal_template() -> &'static str {
    r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__SUSUMU_PORTAL_TITLE__ &middot; __SUSUMU_REVIEW_TITLE__</title>
<style>
:root{color-scheme:dark;--bg:#11131a;--panel:#1a1f2b;--panel2:#202638;--text:#e8e2d7;--muted:#aaa292;--line:#363b49;--accent:#9eb7a0;--accent2:#aaa2bf;--bad:#cc8e8a;--warn:#c8aa72;--ok:#91ad86}
__SUSUMU_PORTAL_THEME__
*{box-sizing:border-box}body{margin:0;font-family:Inter,ui-sans-serif,system-ui,-apple-system,Segoe UI,sans-serif;background:radial-gradient(circle at 20% -10%,#282f3f 0,#11131a 38%),var(--bg);color:var(--text)}
.shell{max-width:1220px;margin:0 auto;padding:40px 22px 70px}.hero{display:grid;grid-template-columns:1.4fr .8fr;gap:20px;align-items:stretch}.card{min-width:0;max-width:100%;overflow:hidden;background:linear-gradient(180deg,rgba(255,255,255,.045),rgba(255,255,255,.02));border:1px solid var(--line);border-radius:24px;box-shadow:0 24px 70px rgba(0,0,0,.22);padding:24px;backdrop-filter:blur(12px)}
.eyebrow{color:var(--accent);font-size:12px;font-weight:800;letter-spacing:.16em;text-transform:uppercase}h1{font-size:clamp(34px,6vw,68px);line-height:.94;margin:12px 0}.sub{color:var(--muted);font-size:16px;line-height:1.6}.pill{display:inline-flex;gap:8px;align-items:center;border:1px solid var(--line);border-radius:999px;padding:7px 11px;color:var(--muted);font-size:13px;margin:4px 4px 0 0}.pill strong{color:var(--text)}
.result{font-size:28px;font-weight:850}.failed{color:var(--bad)}.passed{color:var(--ok)}.grid{display:grid;grid-template-columns:repeat(4,1fr);gap:14px;margin-top:18px}.metric{background:rgba(255,255,255,.045);border:1px solid var(--line);border-radius:18px;padding:16px}.metric b{display:block;font-size:28px}.metric span{color:var(--muted);font-size:13px}
.toolbar{position:sticky;top:0;z-index:3;margin:26px -8px 20px;padding:10px 8px;background:linear-gradient(180deg,rgba(17,19,26,.98),rgba(17,19,26,.78));backdrop-filter:blur(12px)}button{appearance:none;border:1px solid var(--line);border-radius:999px;background:#1b2130;color:var(--text);padding:10px 14px;margin:4px;cursor:pointer;transition:.18s ease}button:hover,button.active{border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.12);transform:translateY(-1px)}
.section{display:none;animation:rise .28s ease}.section.active{display:block}@keyframes rise{from{opacity:0;transform:translateY(8px)}to{opacity:1;transform:none}}h2{font-size:28px;margin:0 0 14px}.list{display:grid;gap:12px;min-width:0}.item{min-width:0;overflow-wrap:anywhere;border:1px solid var(--line);border-radius:18px;background:rgba(255,255,255,.03);padding:16px}.item.clickable{cursor:pointer;transition:.18s ease}.item.clickable:hover,.item.selected{border-color:var(--accent);box-shadow:0 0 0 3px rgba(158,183,160,.11);transform:translateY(-1px)}.item h3{margin:0 0 8px;font-size:17px}.meta{color:var(--muted);font-size:13px;line-height:1.5}.detail{color:#d5ccbd;line-height:1.55}.tag{display:inline-block;border-radius:999px;padding:4px 9px;margin-right:6px;font-size:12px;background:#252b3b;color:#ddd6ca}.critical{background:rgba(204,142,138,.15);color:#ead0cc}.warning{background:rgba(200,170,114,.14);color:#eadab8}.attention{background:rgba(170,162,191,.15);color:#ded8e8}.workflow-score{font-size:24px;color:var(--accent);font-weight:850}.cols{display:grid;grid-template-columns:1fr 1fr;gap:16px}.workflow-layout{display:grid;grid-template-columns:minmax(280px,.8fr) minmax(0,1.2fr);gap:16px;align-items:start;min-width:0;max-width:100%}.workflow-layout>*{min-width:0}.detail-pane{position:sticky;top:98px;align-self:start;min-width:0;max-width:100%;overflow:hidden}.traceability-layout{height:calc(100vh - 180px);min-height:540px;align-items:stretch}.traceability-list,.traceability-detail{min-width:0;max-width:100%;min-height:0;overflow:auto;overscroll-behavior:contain;padding:8px 6px 0 0}.traceability-detail{position:static;align-self:stretch}.mini{display:grid;gap:8px;min-width:0}.mini .item{padding:12px}.ladder{display:grid;gap:10px;margin:10px 0 16px}.ladder-step{position:relative;border:1px solid var(--line);border-radius:16px;background:rgba(255,255,255,.03);padding:13px 14px 13px 46px}.ladder-step:before{content:'';position:absolute;left:17px;top:18px;width:12px;height:12px;border-radius:999px;background:var(--muted);box-shadow:0 0 0 5px rgba(170,162,146,.09)}.ladder-step:after{content:'';position:absolute;left:22px;top:36px;bottom:-18px;width:2px;background:var(--line)}.ladder-step:last-child:after{display:none}.ladder-step.good{border-color:rgba(145,173,134,.45)}.ladder-step.good:before{background:var(--ok);box-shadow:0 0 0 5px rgba(145,173,134,.12)}.ladder-step.warn{border-color:rgba(200,170,114,.48)}.ladder-step.warn:before{background:var(--warn);box-shadow:0 0 0 5px rgba(200,170,114,.12)}.ladder-step.bad{border-color:rgba(204,142,138,.5)}.ladder-step.bad:before{background:var(--bad);box-shadow:0 0 0 5px rgba(204,142,138,.12)}.ladder-label{display:block;color:var(--muted);font-size:12px;font-weight:800;letter-spacing:.08em;text-transform:uppercase}.ladder-step strong{display:block;margin-top:3px}.ladder-step small{display:block;color:#d5ccbd;line-height:1.45;margin-top:4px}.next-action{border-color:rgba(158,183,160,.45);background:linear-gradient(135deg,rgba(158,183,160,.12),rgba(170,162,191,.08))}.search{width:100%;border:1px solid var(--line);border-radius:16px;background:#171b26;color:var(--text);padding:13px 15px;margin:0 0 14px}.empty{color:var(--muted);border:1px dashed var(--line);border-radius:18px;padding:22px;text-align:center}.code{max-width:100%;overflow:auto;background:#131821;border:1px solid #32394a;border-radius:16px;padding:12px;font:13px/1.55 ui-monospace,SFMono-Regular,Consolas,Menlo,monospace}.code-line{display:grid;grid-template-columns:64px minmax(0,1fr);min-width:max-content}.code-line.mark{background:rgba(158,183,160,.09);border-left:3px solid var(--accent)}.ln{color:#777f8f;text-align:right;padding-right:14px;user-select:none}.src{white-space:pre}
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
  <nav class="toolbar" id="tabs"></nav>
  <input class="search" id="search" placeholder="Filter visible section&hellip;">
  <main id="sections"></main>
</div>
<script>
const packet = __SUSUMU_REVIEW_DATA__;
const $ = (id) => document.getElementById(id);
const esc = (v) => String(v ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const list = (items, render, empty='Nothing here yet.') => items && items.length ? `<div class="list">${items.map(render).join('')}</div>` : `<div class="empty">${empty}</div>`;
const severity = (s) => s === 'critical' ? 'critical' : s === 'warning' ? 'warning' : 'attention';
function item(title, body, meta='', tags='', extra=''){return `<article class="item">${tags}<h3>${esc(title)}</h3>${meta?`<div class="meta">${meta}</div>`:''}<div class="detail">${esc(body)}</div>${extra}</article>`}
let selectedWorkflowId = null;
let selectedExpectationId = null;
const tabs = [
 ['overview','Overview'],
 ['readiness','Readiness'],
 ['review','Review'],
 ['workflows','Top workflows'],
 ['traceability','Traceability'],
 ['source','Source'],
 ['records','Records'],
 ['dirty','Dirty/stale'],
 ['artifact','Artifact'],
 ['actions','Next actions']
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
function expectationLadder(e,s){if(!s)return '<div class="empty">No evidence ladder embedded for this expectation.</div>';const total=verificationTotal(s);const verificationDetail=`passed=${s.verification.passed}, failed=${s.verification.failed}, inconclusive=${s.verification.inconclusive}, posture=${s.evidence_posture||'none'}`;return `<div class="ladder" data-evidence-ladder="${esc(e.id)}">${ladderStep('Target observation',s.target_observed?'Target observed':'Target missing',s.target_observed?'good':'bad',`${s.target}${s.subject?':'+s.subject:''}`)}${ladderStep('Work support',s.work>0?`${s.work} linked work record(s)`:'No linked work yet',s.work>0?'good':'warn','Work says what changed for this expectation.')}${ladderStep('Verification evidence',total>0?`${total} verification record(s)`:'No verification yet',s.verification.failed>0?'bad':s.verification.passed>0?'good':'warn',verificationDetail)}${ladderStep('Decision context',s.decisions>0?`${s.decisions} decision record(s)`:'No decision context yet',s.decisions>0?'good':'warn','Decisions record judgment, exceptions, and business context.')}${ladderStep('Review status',s.support_status,s.verification.failed>0||!s.target_observed?'bad':s.verification.passed>0?'good':'warn',(s.reasons||[]).join('; '))}</div><article class="item next-action"><h3>Suggested next action</h3><div class="detail">${esc(expectationNextAction(e,s))}</div></article>`}
function expectationCard(e){const s=expectationSupport(e.id);return `<article class="item clickable ${e.id===selectedExpectationId?'selected':''}" data-expectation-id="${esc(e.id)}"><h3>${esc(e.title)}</h3><div class="meta">${esc(e.id)} &middot; ${esc(e.status)} &middot; ${esc(e.target)}${e.subject?`:${esc(e.subject)}`:''} &middot; ${supportMeta(s)}</div><div class="detail">${esc(e.detail)}</div></article>`}
function expectationDetail(id){const e=expectationById(id);if(!e)return `<div class="empty">Select an expectation to inspect its traceability.</div>`;const workflow=expectationWorkflow(e);const preview=workflow?workflowPreview(workflow.id):targetPreview(e.target,e.subject);const s=expectationSupport(id);return `<div class="item"><h3>${esc(e.title)}</h3><div class="meta">${esc(e.id)} &middot; ${esc(e.status)} &middot; source=${esc(e.source)} &middot; target=${esc(e.target)}${e.subject?`:${esc(e.subject)}`:''}</div><div class="detail">${esc(e.detail)}</div></div><h3>Evidence ladder</h3>${expectationLadder(e,s)}<h3>Support summary</h3><div class="item"><h3>${esc(s?.support_status||'unknown')}</h3><div class="meta">${supportMeta(s)}</div></div><h3>Support reasons</h3>${supportReasons(s)}<h3>Workflow context</h3>${workflow?miniList([workflow],w=>item(w.trigger,`${esc(w.framework)} &middot; handler=${esc(w.handler??'-')} &middot; confidence=${esc(w.confidence)}`,w.id),'No workflow context.'): '<div class="empty">This expectation is not attached to a workflow.</div>'}<h3>Verifications</h3>${miniList(expectationVerifications(id),verificationItem,'No verification records.')}<h3>Work records</h3>${miniList(expectationWork(id),workItem,'No work records.')}<h3>Decisions on same target</h3>${miniList(expectationDecisions(e),decisionItem,'No decisions on this target.')}<h3>Source evidence</h3>${preview?codePreview(preview):'<div class="empty">No source preview embedded for this expectation.</div>'}`}
function readinessBucket(s){if(!s)return 'Unknown';if(s.verification.failed>0)return 'Failed verification';if(!s.target_observed)return 'Missing target';if(s.verification.passed>0)return 'Verified';if(s.work>0)return 'Has work, needs verification';return 'No linked work yet'}
function readinessTone(bucket){return bucket==='Verified'?'good':bucket==='Failed verification'||bucket==='Missing target'?'bad':'warn'}
function readinessItems(){const stored=packet.expectation_readiness||[];if(stored.length)return stored.map(r=>({id:r.expectation_id,title:r.title,label:r.label,next_action:r.next_action}));return expectations().map(e=>{const s=expectationSupport(e.id);return {id:e.id,title:e.title,label:readinessBucket(s),next_action:expectationNextAction(e,s)}})}
function readinessRow(r){const s=expectationSupport(r.id);return item(r.title,r.next_action,`${esc(r.id)} &middot; ${esc(r.label)} &middot; ${supportMeta(s)}`,`<span class="tag ${readinessTone(r.label)==='good'?'passed':readinessTone(r.label)==='bad'?'critical':'warning'}">${esc(r.label)}</span>`)}
function readinessSection(){const order=['Failed verification','Missing target','Has work, needs verification','No linked work yet','Verified','Unknown'];const rows=readinessItems();const metrics=order.map(label=>`<div class="metric"><b>${rows.filter(r=>r.label===label).length}</b><span>${esc(label)}</span></div>`).join('');return `<div class="grid">${metrics}</div><div class="list" style="margin-top:16px">${order.map(label=>{const items=rows.filter(r=>r.label===label).map(readinessRow).join('');return items?`<div><h3>${esc(label)}</h3><div class="mini">${items}</div></div>`:''}).join('')||'<div class="empty">No expectations authored yet.</div>'}</div>`}
function traceabilitySection(){const first=expectations()[0]?.id;selectedExpectationId=selectedExpectationId||first;return `<div class="workflow-layout traceability-layout"><div class="traceability-list">${list(expectations(),expectationCard,'No expectations authored yet.')}</div><aside class="detail-pane traceability-detail" id="expectationDetail">${expectationDetail(selectedExpectationId)}</aside></div>`}
function dirtyFinding(f){return ['SUS023','SUS033'].includes(f.rule_id)}
function staleFinding(f){return ['SUS011','SUS021','SUS031','SUS041','SUS043'].includes(f.rule_id)}
function findingPreview(f){return previewForLocation(f.file_id,f.location)}
function findingCard(f){const p=findingPreview(f);return item(`${f.rule_id}: ${f.title}`,f.detail,`source=${esc(f.source)} &middot; subject=${esc(f.subject??'-')}${sourceMetaForPreview(p)}`,`<span class="tag ${severity(f.severity)}">${esc(f.severity)}</span>` ,sourcePreviewExtra(p))}
function dirtySection(){const findings=packet.artifact.findings||[];const dirty=findings.filter(dirtyFinding);const stale=findings.filter(staleFinding);return `<div class="cols"><div><h3>Dirty evidence</h3>${list(dirty,findingCard,'No changed verification or decision evidence detected.')}</div><div><h3>Stale or missing record targets</h3>${list(stale,findingCard,'No stale record targets detected.')}</div></div>`}
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
 $('tabs').innerHTML = tabs.map(([id,label],i)=>`<button class="${i===0?'active':''}" data-tab="${id}">${label}</button>`).join('');
 $('sections').innerHTML = [
  section('overview','Overview', `<div class="grid">
    <div class="metric"><b>${packet.records.expectations}</b><span>expectations</span></div>
    <div class="metric"><b>${packet.records.verifications}</b><span>verifications</span></div>
    <div class="metric"><b>${packet.records.decisions}</b><span>decisions</span></div>
    <div class="metric"><b>${packet.records.work}</b><span>work records</span></div>
  </div><div class="cols" style="margin-top:16px"><div>${list(packet.caveats,a=>item('Caveat',a))}</div><div>${list(packet.next_actions,a=>item('Suggested action',a))}</div></div>`),
  section('readiness','Expectation readiness board', readinessSection()),
  section('review','Needs review', list(packet.review_items, r => item(r.title, r.detail, `source=${esc(r.source)}`, `<span class="tag ${severity(r.severity)}">${esc(r.severity)}</span>`), 'No review items derived.')),
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
 $('search').addEventListener('input', filter);
}
function selectWorkflow(id){selectedWorkflowId=id;document.querySelectorAll('[data-workflow-id]').forEach(card=>card.classList.toggle('selected',card.dataset.workflowId===id));$('workflowDetail').innerHTML=workflowDetail(id);}
function selectExpectation(id){selectedExpectationId=id;document.querySelectorAll('[data-expectation-id]').forEach(card=>card.classList.toggle('selected',card.dataset.expectationId===id));$('expectationDetail').innerHTML=expectationDetail(id);}
function activate(id){document.querySelectorAll('[data-tab]').forEach(b=>b.classList.toggle('active',b.dataset.tab===id));document.querySelectorAll('.section').forEach(s=>s.classList.toggle('active',s.id===`section-${id}`));$('search').value='';filter();}
function filter(){const q=$('search').value.toLowerCase();document.querySelectorAll('.section.active .item').forEach(el=>el.style.display=el.textContent.toLowerCase().includes(q)?'':'none');}
render();
</script>
</body>
</html>"#
}
