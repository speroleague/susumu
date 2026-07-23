use serde::Serialize;
use susumu::model::{ExpectationTarget, ProjectAnalysis, Work};

use crate::cli::values::GitTargetDepth;

use super::connect::GitConnection;

#[derive(Debug)]
pub(crate) struct GitImportContext<'a> {
    pub(crate) artifact: Option<&'a ProjectAnalysis>,
    pub(crate) target_depth: GitTargetDepth,
}

#[derive(Debug, Clone)]
pub(crate) struct GitCommit {
    pub(crate) hash: String,
    pub(crate) author_name: String,
    pub(crate) author_email: String,
    pub(crate) author_date: String,
    pub(crate) subject: String,
    pub(crate) body: String,
    pub(crate) changed_files: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct ImportedGitWork {
    pub(crate) work: Work,
    pub(crate) commit_hash: String,
    pub(crate) targeting: String,
    pub(crate) changed_files: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitImportJson<'a> {
    pub(crate) output: String,
    pub(crate) imported: usize,
    pub(crate) records: Vec<GitImportRecordJson<'a>>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitImportRecordJson<'a> {
    pub(crate) id: &'a str,
    pub(crate) commit: &'a str,
    pub(crate) target: String,
    pub(crate) subject: Option<&'a str>,
    pub(crate) expectation: Option<&'a str>,
    pub(crate) title: &'a str,
    pub(crate) targeting: &'a str,
    pub(crate) changed_files: &'a [String],
}

#[derive(Debug, Serialize)]
pub(crate) struct GitConnectExport {
    pub(crate) path: String,
    pub(crate) written: usize,
    pub(crate) source: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitConnectJson<'a> {
    pub(crate) repo: String,
    pub(crate) artifact: String,
    pub(crate) since: Option<&'a str>,
    pub(crate) until: Option<&'a str>,
    pub(crate) commits: usize,
    pub(crate) connected: usize,
    pub(crate) needs_record: usize,
    pub(crate) unconnected: usize,
    pub(crate) export: Option<&'a GitConnectExport>,
    pub(crate) records: &'a [GitConnection],
}

#[derive(Debug)]
pub(crate) struct GitWorkTarget {
    pub(crate) target: ExpectationTarget,
    pub(crate) subject: Option<String>,
    pub(crate) note: String,
}

#[derive(Debug)]
pub(crate) struct GitExpectationLink {
    pub(crate) id: String,
    pub(crate) target: ExpectationTarget,
    pub(crate) subject: Option<String>,
}
