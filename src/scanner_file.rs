use std::{collections::HashMap, fs, path::Path};

use crate::{
    language::{ParsedCall, ParsedWorkflow, parse_file},
    model::{
        Dependency, Finding, Language, Location, ProjectAnalysis, Severity, SourceFile, Symbol,
    },
    scanner::{hex_prefix, stable_id},
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct PendingCall {
    pub(crate) file_id: String,
    pub(crate) caller_id: String,
    pub(crate) call: ParsedCall,
}

#[derive(Debug)]
pub(crate) struct PendingWorkflow {
    pub(crate) file_id: String,
    pub(crate) workflow: ParsedWorkflow,
}

pub(crate) fn scan_file(
    root: &Path,
    path: &Path,
    analysis: &mut ProjectAnalysis,
    pending_calls: &mut Vec<PendingCall>,
    pending_workflows: &mut Vec<PendingWorkflow>,
) {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_string = relative.to_string_lossy().replace('\\', "/");
    let file_id = stable_id("f", &[&relative_string]);
    let Some(language) = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(Language::from_extension)
    else {
        return;
    };
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() > MAX_FILE_BYTES {
        add_skipped_file_finding(
            analysis,
            format!("{relative_string} exceeds the 2 MiB scan limit"),
        );
        return;
    }

    let Ok(source) = fs::read_to_string(path) else {
        add_skipped_file_finding(
            analysis,
            format!("{relative_string} is not readable UTF-8 text"),
        );
        return;
    };
    let lines = source.lines().count().max(1);
    analysis.files.push(SourceFile {
        id: file_id.clone(),
        path: relative_string.clone(),
        language,
        lines,
        bytes: metadata.len(),
        content_hash: Some(content_hash(&source)),
    });

    let Some(parsed) = parse_source_file(
        analysis,
        relative,
        &relative_string,
        &file_id,
        language,
        &source,
    ) else {
        return;
    };
    add_incomplete_syntax_finding(analysis, &file_id, lines, parsed.has_parse_errors);

    record_parsed_file(
        analysis,
        &file_id,
        &relative_string,
        pending_calls,
        pending_workflows,
        parsed,
    );
}

fn add_skipped_file_finding(analysis: &mut ProjectAnalysis, detail: String) {
    analysis.findings.push(Finding {
        rule_id: "SUS000".to_owned(),
        source: "susumu:scanner".to_owned(),
        severity: Severity::Info,
        title: "File skipped".to_owned(),
        detail,
        file_id: None,
        subject: None,
        location: None,
    });
}

fn parse_source_file(
    analysis: &mut ProjectAnalysis,
    relative: &Path,
    relative_path: &str,
    file_id: &str,
    language: Language,
    source: &str,
) -> Option<crate::language::ParsedFile> {
    match parse_file(language, source, is_module_entrypoint(relative, language)) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            analysis.findings.push(Finding {
                rule_id: "SUS000".to_owned(),
                source: "susumu:scanner".to_owned(),
                severity: Severity::Warning,
                title: "Parser failed".to_owned(),
                detail: format!("{relative_path}: {error:#}"),
                file_id: Some(file_id.to_owned()),
                subject: None,
                location: None,
            });
            None
        }
    }
}

fn add_incomplete_syntax_finding(
    analysis: &mut ProjectAnalysis,
    file_id: &str,
    lines: usize,
    has_parse_errors: bool,
) {
    if !has_parse_errors {
        return;
    }
    analysis.findings.push(Finding {
        rule_id: "SUS006".to_owned(),
        source: "susumu:scanner".to_owned(),
        severity: Severity::Info,
        title: "Incomplete syntax tree".to_owned(),
        detail: "Tree-sitter recovered from syntax it could not fully parse; evidence from this file may be incomplete.".to_owned(),
        file_id: Some(file_id.to_owned()),
        subject: None,
        location: Some(Location {
            start_line: 1,
            start_column: 1,
            end_line: lines,
            end_column: 1,
        }),
    });
}

fn record_parsed_file(
    analysis: &mut ProjectAnalysis,
    file_id: &str,
    relative_path: &str,
    pending_calls: &mut Vec<PendingCall>,
    pending_workflows: &mut Vec<PendingWorkflow>,
    parsed: crate::language::ParsedFile,
) {
    let symbol_ids = record_symbols(analysis, file_id, relative_path, parsed.symbols);
    for dependency in parsed.dependencies {
        analysis.dependencies.push(Dependency {
            file_id: file_id.to_owned(),
            name: dependency.name,
            location: dependency.location,
        });
    }
    pending_workflows.extend(
        parsed
            .workflows
            .into_iter()
            .map(|workflow| PendingWorkflow {
                file_id: file_id.to_owned(),
                workflow,
            }),
    );
    for call in parsed.calls {
        let Some(caller_id) = symbol_ids.get(call.caller).cloned() else {
            continue;
        };
        pending_calls.push(PendingCall {
            file_id: file_id.to_owned(),
            caller_id,
            call,
        });
    }
}

fn record_symbols(
    analysis: &mut ProjectAnalysis,
    file_id: &str,
    relative_path: &str,
    parsed_symbols: Vec<crate::language::ParsedSymbol>,
) -> Vec<String> {
    let mut occurrences = HashMap::new();
    let mut symbol_ids = Vec::with_capacity(parsed_symbols.len());
    for parsed_symbol in parsed_symbols {
        let occurrence = occurrences
            .entry((parsed_symbol.name.clone(), parsed_symbol.kind))
            .or_insert(0_usize);
        let kind = parsed_symbol.kind.to_string();
        let ordinal = occurrence.to_string();
        let id = stable_id("s", &[relative_path, &kind, &parsed_symbol.name, &ordinal]);
        *occurrence += 1;
        symbol_ids.push(id.clone());
        analysis.symbols.push(Symbol {
            id,
            name: parsed_symbol.name,
            kind: parsed_symbol.kind,
            file_id: file_id.to_owned(),
            location: parsed_symbol.location,
            entrypoint: parsed_symbol.entrypoint,
        });
    }
    symbol_ids
}

fn content_hash(source: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    digest.update(source.as_bytes());
    hex_prefix(&digest.finalize(), 16)
}

fn is_module_entrypoint(relative: &Path, language: Language) -> bool {
    let normalized = relative.to_string_lossy().replace('\\', "/");
    let file_name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    match language {
        Language::Rust => normalized == "src/main.rs" || file_name == "main.rs",
        Language::Php => file_name == "index.php" || normalized.starts_with("routes/"),
        Language::Python => file_name == "__main__.py" || file_name == "main.py",
        Language::JavaScript | Language::TypeScript | Language::Tsx | Language::Vue => matches!(
            file_name,
            "index.js" | "index.ts" | "index.tsx" | "main.js" | "main.ts" | "app.js" | "app.ts"
        ),
    }
}
