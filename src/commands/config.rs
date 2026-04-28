use crate::config::Config;
use crate::error::Result;
use crate::repo::Repository;

pub fn get(repo: &Repository, key: &str) -> Result<String> {
    let config = Config::load(&repo.config_path())?;
    config.get(key)
}

pub fn set(repo: &Repository, key: &str, value: &str) -> Result<()> {
    let mut config = Config::load(&repo.config_path())?;
    config.set(key, value)?;
    config.save(&repo.config_path())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::Repository;
    use std::fs;

    fn create_test_repo() -> (Repository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join(".research")).unwrap();

        let config = crate::config::Config::default();
        config.save(&root.join(".research/config.yaml")).unwrap();

        (Repository { root: root.to_path_buf() }, dir)
    }

    #[test]
    fn test_get_existing_key() {
        let (repo, _dir) = create_test_repo();

        let value = get(&repo, "project_name").unwrap();
        assert_eq!(value, "research-project");
    }

    #[test]
    fn test_set_and_get() {
        let (repo, _dir) = create_test_repo();

        set(&repo, "project_name", "new-name").unwrap();

        let value = get(&repo, "project_name").unwrap();
        assert_eq!(value, "new-name");
    }

    #[test]
    fn test_get_missing_key_fails() {
        let (repo, _dir) = create_test_repo();

        let result = get(&repo, "nonexistent");
        assert!(result.is_err());
    }
}
