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
