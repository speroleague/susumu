use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Component, Path, PathBuf},
    process,
    process::Command as ProcessCommand,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

pub(crate) fn git_snapshot_dir(repo: &Path, revision: &str) -> Result<PathBuf> {
    let snapshot_dir =
        env::temp_dir().join(format!("susumu-rewind-{}", git_snapshot_id(repo, revision)));
    fs::create_dir_all(&snapshot_dir)
        .with_context(|| format!("could not create {}", snapshot_dir.display()))?;

    let result = populate_git_snapshot(repo, revision, &snapshot_dir);
    if result.is_err() {
        let _ = fs::remove_dir_all(&snapshot_dir);
    }
    result?;
    Ok(snapshot_dir)
}

pub(crate) fn populate_git_snapshot(
    repo: &Path,
    revision: &str,
    snapshot_dir: &Path,
) -> Result<()> {
    for git_path in git_tree_paths(repo, revision)? {
        let output_path = safe_snapshot_path(snapshot_dir, &git_path)?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let bytes = git_file_bytes(repo, revision, &git_path)?;
        fs::write(&output_path, bytes)
            .with_context(|| format!("could not write {}", output_path.display()))?;
    }
    Ok(())
}

pub(crate) fn git_tree_paths(repo: &Path, revision: &str) -> Result<Vec<String>> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("ls-tree")
        .arg("-r")
        .arg("-z")
        .arg("--name-only")
        .arg(revision)
        .output()
        .with_context(|| format!("could not list files at Git ref {revision}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git ls-tree failed for {revision}: {}", stderr.trim());
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).into_owned())
        .collect())
}

pub(crate) fn git_file_bytes(repo: &Path, revision: &str, git_path: &str) -> Result<Vec<u8>> {
    let spec = format!("{revision}:{git_path}");
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo)
        .arg("show")
        .arg(spec)
        .output()
        .with_context(|| format!("could not read {git_path} at Git ref {revision}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git show failed for {git_path} at {revision}: {}",
            stderr.trim()
        );
    }
    Ok(output.stdout)
}

pub(crate) fn safe_snapshot_path(root: &Path, git_path: &str) -> Result<PathBuf> {
    let normalized = normalize_path(git_path);
    if normalized.is_empty() {
        bail!("Git snapshot path cannot be empty");
    }
    if looks_like_windows_absolute_path(&normalized) {
        bail!("refusing unsafe Git snapshot path: {git_path}");
    }

    let mut output = root.to_path_buf();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                bail!("refusing unsafe Git snapshot path: {git_path}")
            }
        }
    }
    Ok(output)
}

pub(crate) fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

pub(crate) fn git_snapshot_id(repo: &Path, revision: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let mut hash = Sha256::new();
    hash.update(repo.display().to_string().as_bytes());
    hash.update([0]);
    hash.update(revision.as_bytes());
    hash.update([0]);
    hash.update(process::id().to_string().as_bytes());
    hash.update([0]);
    hash.update(timestamp.to_string().as_bytes());
    hex_prefix(&hash.finalize(), 8)
}

pub(crate) fn git_repo_label(repo: &Path) -> String {
    repo.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or_else(|| repo.display().to_string(), str::to_owned)
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_owned()
}

fn hex_prefix(bytes: &[u8], count: usize) -> String {
    let mut output = String::new();
    for byte in bytes.iter().take(count) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
