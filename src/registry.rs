use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;

pub fn load_managed_sessions() -> anyhow::Result<HashSet<String>> {
    load_managed_sessions_from_path(&default_managed_sessions_path()?)
}

pub fn load_managed_sessions_from_path(path: &Path) -> anyhow::Result<HashSet<String>> {
    if !path.exists() {
        return Ok(HashSet::new());
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let values: Vec<String> = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(values.into_iter().collect())
}

pub fn save_managed_sessions<I, S>(sessions: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    save_managed_sessions_to_path(&default_managed_sessions_path()?, sessions)
}

pub fn save_managed_sessions_to_path<I, S>(path: &Path, sessions: I) -> anyhow::Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut values = sessions
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    fs::write(path, serde_json::to_string_pretty(&values)?)?;
    Ok(())
}

fn default_managed_sessions_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/ocmux/managed-sessions.json"))
}
