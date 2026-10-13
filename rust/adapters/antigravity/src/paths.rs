use std::{collections::HashSet, env, fs, path::PathBuf};

use ccusage_adapter_common::collect_files_with_extension;

use crate::Result;

pub(super) const ANTIGRAVITY_DATA_DIR_ENV: &str = "ANTIGRAVITY_DATA_DIR";

const DEFAULT_ANTIGRAVITY_ROOTS: [&str; 6] = [
    ".gemini/antigravity",
    ".gemini/antigravity-cli",
    ".gemini/antigravity-ide",
    ".gemini/antigravity-backup",
    ".gemini/antigravity-acp",
    ".config/antigravity",
];

fn roots() -> Vec<PathBuf> {
    if let Ok(value) = env::var(ANTIGRAVITY_DATA_DIR_ENV) {
        return value
            .split(',')
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .collect();
    }

    crate::home::home_dir()
        .into_iter()
        .flat_map(|home| {
            DEFAULT_ANTIGRAVITY_ROOTS
                .into_iter()
                .map(move |root| home.join(root))
        })
        .collect()
}

fn conversation_dir(root: PathBuf) -> PathBuf {
    let nested = root.join("conversations");
    if nested.is_dir() { nested } else { root }
}

/// Returns each `.db` path and whether it must be a conversation DB. Only a fallback
/// scan of a default root may skip other SQLite files (e.g. `conversation_summaries.db`).
pub(super) fn conversation_db_paths() -> Result<Vec<(PathBuf, bool)>> {
    let explicit = env::var(ANTIGRAVITY_DATA_DIR_ENV).is_ok();
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for root in roots().into_iter().map(conversation_dir) {
        let strict = explicit || root.file_name().is_some_and(|name| name == "conversations");
        let mut files = Vec::new();
        collect_files_with_extension(&root, "db", &mut files);
        for path in files {
            let canonical_path = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(canonical_path) {
                paths.push((path, strict));
            }
        }
    }
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use ccusage_test_support::{EnvVarsGuard, fs_fixture};

    use super::*;

    #[test]
    fn discovers_databases_from_independent_roots() {
        let fixture = fs_fixture!({
            ".gemini/antigravity/conversations/ide.db": "",
            ".gemini/antigravity-cli/conversations/cli.db": "",
            ".gemini/antigravity-ide/conversations/ide-alt.db": "",
            ".gemini/antigravity-backup/conversations/backup.db": "",
            ".gemini/antigravity-acp/conversations/acp.db": "",
            ".config/antigravity/conversations/config.db": "",
        });
        let _guard = EnvVarsGuard::set_many([
            (ANTIGRAVITY_DATA_DIR_ENV, None),
            ("HOME", Some(OsString::from(fixture.root()))),
            ("USERPROFILE", Some(OsString::from(fixture.root()))),
        ]);

        let paths = conversation_db_paths().unwrap();

        assert_eq!(paths.len(), 6);
        assert!(
            paths
                .iter()
                .all(|(path, strict)| *strict && path.extension().unwrap() == "db")
        );
    }

    #[test]
    fn marks_default_root_fallback_scans_as_non_strict() {
        let fixture = fs_fixture!({
            ".gemini/antigravity-cli/conversation_summaries.db": "",
            ".gemini/antigravity/conversations/session.db": "",
        });
        let _guard = EnvVarsGuard::set_many([
            (ANTIGRAVITY_DATA_DIR_ENV, None),
            ("HOME", Some(OsString::from(fixture.root()))),
            ("USERPROFILE", Some(OsString::from(fixture.root()))),
        ]);

        assert_eq!(
            conversation_db_paths().unwrap(),
            vec![
                (
                    fixture.path(".gemini/antigravity/conversations/session.db"),
                    true
                ),
                (
                    fixture.path(".gemini/antigravity-cli/conversation_summaries.db"),
                    false
                ),
            ]
        );
    }

    #[test]
    fn accepts_conversation_directory_and_parent_directory_overrides() {
        let fixture = fs_fixture!({
            "parent/conversations/parent.db": "",
            "direct/conversation.db": "",
        });
        let override_value = format!(
            "{},{}",
            fixture.path("parent").display(),
            fixture.path("direct").display()
        );
        let _guard = EnvVarsGuard::set_many([(
            ANTIGRAVITY_DATA_DIR_ENV,
            Some(OsString::from(override_value)),
        )]);

        let paths = conversation_db_paths().unwrap();

        assert_eq!(
            paths,
            vec![
                (fixture.path("direct/conversation.db"), true),
                (fixture.path("parent/conversations/parent.db"), true),
            ]
        );
    }

    #[test]
    fn deduplicates_canonicalized_override_aliases() {
        let fixture = fs_fixture!({
            "conversations/session.db": "",
        });
        let override_value = format!(
            "{},{}",
            fixture.root().display(),
            fixture.path("conversations/..").display()
        );
        let _guard = EnvVarsGuard::set_many([(
            ANTIGRAVITY_DATA_DIR_ENV,
            Some(OsString::from(override_value)),
        )]);

        assert_eq!(
            conversation_db_paths().unwrap(),
            vec![(fixture.path("conversations/session.db"), true)]
        );
    }
}
