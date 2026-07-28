use std::{collections::BTreeSet, fs, path::Path, sync::OnceLock};

use susumu::model::{Language, Location, ProjectAnalysis};
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
};

use super::{
    checks::check_item_jsons,
    expectation_readiness::{expectation_readiness, expectation_support},
    types::{
        CheckEvidenceJson, CheckProjectJson, CheckRecordsJson, CheckReport, CheckResultJson,
        CheckReviewJson, HandoffReport, ReviewPacketJson, ReviewSourceJson, ReviewSourceLine,
        ReviewSourcePreview, ReviewSourceToken, check_result_reason,
    },
};

pub(crate) fn review_packet<'a>(
    input: String,
    created_unix_seconds: u64,
    analysis: &'a ProjectAnalysis,
    check: &'a CheckReport,
    handoff: &'a HandoffReport,
) -> ReviewPacketJson<'a> {
    let expectation_support = expectation_support(analysis);
    let expectation_readiness = expectation_readiness(analysis, &expectation_support);
    ReviewPacketJson {
        schema_version: "susumu.review.v1",
        created_unix_seconds,
        source: ReviewSourceJson { input },
        project: CheckProjectJson {
            name: &analysis.project_name,
            root: &analysis.root,
            generated_unix_seconds: analysis.generated_unix_seconds,
        },
        evidence: CheckEvidenceJson {
            files: analysis.files.len(),
            workflows: analysis.workflows.len(),
            flows: analysis.flows.len(),
            findings: analysis.findings.len(),
        },
        records: CheckRecordsJson {
            expectations: analysis.expectations.len(),
            verifications: analysis.verifications.len(),
            decisions: analysis.decisions.len(),
            work: analysis.works.len(),
            review_threads: analysis.review_threads.len(),
        },
        review: CheckReviewJson {
            critical: check.critical,
            warning: check.warning,
            attention: check.attention,
        },
        result: CheckResultJson {
            status: if check.failed { "failed" } else { "passed" },
            failed: check.failed,
            strict: check.strict,
            reason: check_result_reason(check),
        },
        top_workflows: &handoff.top_workflows,
        review_items: check_item_jsons(&check.items),
        source_previews: review_source_previews(analysis),
        expectation_support,
        expectation_readiness,
        expectations_without_verification: &handoff.expectations_without_verification,
        work_needing_verification: &handoff.work_needing_verification,
        caveats: &handoff.caveats,
        next_actions: &handoff.next_actions,
        artifact: analysis,
    }
}

pub(crate) fn review_source_previews(analysis: &ProjectAnalysis) -> Vec<ReviewSourcePreview> {
    let mut previews = Vec::new();
    let mut seen = BTreeSet::new();
    for workflow in &analysis.workflows {
        push_review_source_preview(
            analysis,
            &mut previews,
            &mut seen,
            &workflow.file_id,
            &workflow.location,
        );
    }
    for finding in &analysis.findings {
        let (Some(file_id), Some(location)) =
            (finding.file_id.as_deref(), finding.location.as_ref())
        else {
            continue;
        };
        push_review_source_preview(analysis, &mut previews, &mut seen, file_id, location);
    }
    previews.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.highlight_start.cmp(&right.highlight_start))
            .then_with(|| left.highlight_end.cmp(&right.highlight_end))
    });
    previews
}

fn push_review_source_preview(
    analysis: &ProjectAnalysis,
    previews: &mut Vec<ReviewSourcePreview>,
    seen: &mut BTreeSet<String>,
    file_id: &str,
    location: &Location,
) {
    let key = format!("{}:{}:{}", file_id, location.start_line, location.end_line);
    if !seen.insert(key) {
        return;
    }
    let Some(file) = analysis.files.iter().find(|file| file.id == file_id) else {
        return;
    };
    let path = Path::new(&analysis.root).join(&file.path);
    let Ok(source) = fs::read_to_string(&path) else {
        return;
    };
    let source_lines = source.lines().collect::<Vec<_>>();
    let line_count = source_lines.len().max(1);
    let start = location.start_line.saturating_sub(6).max(1);
    let end = (location.end_line + 10).min(line_count);
    let mut highlighter = HighlightLines::new(
        syntax_for_review_language(review_syntax_set(), file.language),
        review_syntax_theme(),
    );
    let lines = (start..=end)
        .map(|number| {
            let text = source_lines.get(number - 1).copied().unwrap_or_default();
            let tokens = highlighted_review_line_tokens(&mut highlighter, text);
            ReviewSourceLine {
                number,
                text: text.to_owned(),
                html: review_tokens_to_html(&tokens),
                tokens,
            }
        })
        .collect::<Vec<_>>();
    previews.push(ReviewSourcePreview {
        file_id: file.id.clone(),
        path: file.path.clone(),
        language: file.language.to_string(),
        start_line: start,
        end_line: end,
        highlight_start: location.start_line,
        highlight_end: location.end_line,
        lines,
    });
}

fn highlighted_review_line_tokens(
    highlighter: &mut HighlightLines<'_>,
    text: &str,
) -> Vec<ReviewSourceToken> {
    let syntax_set = review_syntax_set();
    let Ok(ranges) = highlighter.highlight_line(text, syntax_set) else {
        return vec![ReviewSourceToken {
            text: text.to_owned(),
            color: "#d8dee9".to_owned(),
        }];
    };
    ranges
        .into_iter()
        .map(|(style, segment)| {
            let foreground = style.foreground;
            ReviewSourceToken {
                text: segment.to_owned(),
                color: format!(
                    "#{:02x}{:02x}{:02x}",
                    foreground.r, foreground.g, foreground.b
                ),
            }
        })
        .collect()
}

fn review_tokens_to_html(tokens: &[ReviewSourceToken]) -> String {
    let mut output = String::new();
    for token in tokens {
        output.push_str("<span style=\"color:");
        output.push_str(&source_preview_html_escape(&token.color));
        output.push_str("\">");
        output.push_str(&source_preview_html_escape(&token.text));
        output.push_str("</span>");
    }
    output
}

fn review_syntax_set() -> &'static SyntaxSet {
    static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn review_syntax_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.values().next())
            .cloned()
            .unwrap_or_default()
    })
}

fn syntax_for_review_language(syntax_set: &SyntaxSet, language: Language) -> &SyntaxReference {
    let extension = match language {
        Language::Rust => "rs",
        Language::Php => "php",
        Language::Python => "py",
        Language::JavaScript => "js",
        Language::TypeScript => "ts",
        Language::Tsx => "tsx",
        Language::Vue => "vue",
    };
    syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

fn source_preview_html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use susumu::model::{Confidence, Location, SCHEMA_VERSION, SourceFile, Workflow, WorkflowKind};

    #[test]
    fn review_source_previews_embed_highlighted_tokens() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(temp.path().join("src")).expect("create source dir");
        fs::write(
            temp.path().join("src").join("api.ts"),
            "export function checkout() { return 'ok'; }\n",
        )
        .expect("write source");
        let artifact = ProjectAnalysis {
            schema_version: SCHEMA_VERSION,
            project_name: "fixture".to_owned(),
            root: temp.path().display().to_string(),
            generated_unix_seconds: 0,
            files: vec![SourceFile {
                id: "f_api".to_owned(),
                path: "src/api.ts".to_owned(),
                language: Language::TypeScript,
                lines: 1,
                bytes: 43,
                content_hash: None,
            }],
            symbols: Vec::new(),
            dependencies: Vec::new(),
            workflows: vec![Workflow {
                id: "w_checkout".to_owned(),
                kind: WorkflowKind::Http,
                framework: "express".to_owned(),
                trigger: "POST /checkout".to_owned(),
                handler: Some("checkout".to_owned()),
                entry_symbol: None,
                file_id: "f_api".to_owned(),
                confidence: Confidence::Exact,
                location: Location {
                    start_line: 1,
                    start_column: 1,
                    end_line: 1,
                    end_column: 1,
                },
            }],
            workflow_priorities: Vec::new(),
            flows: Vec::new(),
            expectations: Vec::new(),
            verifications: Vec::new(),
            decisions: Vec::new(),
            works: Vec::new(),
            review_threads: Vec::new(),
            findings: Vec::new(),
        };

        let previews = review_source_previews(&artifact);

        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].path, "src/api.ts");
        assert!(previews[0].lines[0].html.contains("<span"));
        assert!(!previews[0].lines[0].tokens.is_empty());
    }
}
