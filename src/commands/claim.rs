use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::error::{ArcliError, Result};
use crate::repo::Repository;

/// Root structure for .research/claims.yaml
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ClaimsFile {
    pub claims_format: String,
    pub claims: std::collections::BTreeMap<String, Claim>,
}

/// A single project-level claim
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub statement: String,
    pub falsification: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified_by: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Summary of a claim for list output
#[derive(Debug, Serialize)]
pub struct ClaimSummary {
    pub id: String,
    pub statement: String,
    pub verified_by_count: usize,
}

/// Detailed claim output for `claim show`
#[derive(Debug, Serialize)]
pub struct ClaimDetail {
    pub id: String,
    pub statement: String,
    pub falsification: String,
    pub verified_by: Vec<VerifiedExp>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct VerifiedExp {
    pub exp_id: String,
    pub status: String,
    pub commit_hash: Option<String>,
    pub created_at: String,
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

pub fn load_claims(path: &Path) -> Result<ClaimsFile> {
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let cf: ClaimsFile = serde_yaml::from_str(&content)?;
        Ok(cf)
    } else {
        Ok(ClaimsFile {
            claims_format: "1.0".to_string(),
            claims: std::collections::BTreeMap::new(),
        })
    }
}

fn save_claims(path: &Path, cf: &ClaimsFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_yaml::to_string(cf).map_err(ArcliError::Yaml)?;
    fs::write(path, content)?;
    Ok(())
}

pub fn add(repo: &Repository, id: &str, statement: &str, falsification: &str) -> Result<()> {
    let path = repo.claims_path();
    let mut cf = load_claims(&path)?;

    let id_upper = id.to_uppercase();
    if cf.claims.contains_key(&id_upper) {
        return Err(ArcliError::ClaimExists(id_upper));
    }

    let ts = now();
    cf.claims.insert(
        id_upper.clone(),
        Claim {
            statement: statement.to_string(),
            falsification: falsification.to_string(),
            verified_by: Vec::new(),
            created_at: ts.clone(),
            updated_at: ts,
        },
    );

    save_claims(&path, &cf)?;

    let db = Database::open(&repo.db_path())?;
    db.ensure_claim(&id_upper)?;

    Ok(())
}

pub fn list(repo: &Repository) -> Result<Vec<ClaimSummary>> {
    let path = repo.claims_path();
    let cf = load_claims(&path)?;
    let summaries: Vec<ClaimSummary> = cf
        .claims
        .iter()
        .map(|(id, c)| ClaimSummary {
            id: id.clone(),
            statement: c.statement.chars().take(80).collect(),
            verified_by_count: c.verified_by.len(),
        })
        .collect();
    Ok(summaries)
}

pub fn show(repo: &Repository, id: &str) -> Result<ClaimDetail> {
    let path = repo.claims_path();
    let cf = load_claims(&path)?;
    let id_upper = id.to_uppercase();

    let claim = cf
        .claims
        .get(&id_upper)
        .ok_or_else(|| ArcliError::ClaimNotFound(id_upper.clone()))?;

    let db = Database::open(&repo.db_path())?;
    let mut verified_exps = Vec::new();
    for exp_id in &claim.verified_by {
        if let Some(exp) = db.get_experiment(exp_id)? {
            verified_exps.push(VerifiedExp {
                exp_id: exp.id,
                status: exp.status,
                commit_hash: exp.commit_hash,
                created_at: exp.created_at,
            });
        }
    }

    Ok(ClaimDetail {
        id: id_upper,
        statement: claim.statement.clone(),
        falsification: claim.falsification.clone(),
        verified_by: verified_exps,
        created_at: claim.created_at.clone(),
        updated_at: claim.updated_at.clone(),
    })
}

pub fn verify(repo: &Repository, claim_id: &str, exp_id: &str) -> Result<()> {
    let path = repo.claims_path();
    let mut cf = load_claims(&path)?;
    let id_upper = claim_id.to_uppercase();

    let claim = cf
        .claims
        .get_mut(&id_upper)
        .ok_or_else(|| ArcliError::ClaimNotFound(id_upper.clone()))?;

    if claim.verified_by.contains(&exp_id.to_string()) {
        return Err(ArcliError::ClaimAlreadyVerified(id_upper));
    }

    // Write DB first so experiment-side link is created before claims.yaml is updated
    let db = Database::open(&repo.db_path())?;
    db.add_claim_to_experiment(exp_id, &id_upper)?;

    claim.verified_by.push(exp_id.to_string());
    claim.updated_at = now();
    save_claims(&path, &cf)?;

    Ok(())
}

pub fn unverify(repo: &Repository, claim_id: &str, exp_id: &str) -> Result<()> {
    let path = repo.claims_path();
    let mut cf = load_claims(&path)?;
    let id_upper = claim_id.to_uppercase();

    let claim = cf
        .claims
        .get_mut(&id_upper)
        .ok_or_else(|| ArcliError::ClaimNotFound(id_upper.clone()))?;

    let old_len = claim.verified_by.len();
    claim.verified_by.retain(|e| e != exp_id);
    if claim.verified_by.len() == old_len {
        return Err(ArcliError::ClaimNotFound(format!("实验 {} 的绑定", exp_id)));
    }
    claim.updated_at = now();

    // Write DB first so experiment-side link is removed before claims.yaml
    let db = Database::open(&repo.db_path())?;
    db.remove_claim_from_experiment(exp_id, &id_upper)?;

    save_claims(&path, &cf)?;

    Ok(())
}

pub fn update(
    repo: &Repository,
    id: &str,
    statement: Option<&str>,
    falsification: Option<&str>,
) -> Result<()> {
    let path = repo.claims_path();
    let mut cf = load_claims(&path)?;
    let id_upper = id.to_uppercase();

    let claim = cf
        .claims
        .get_mut(&id_upper)
        .ok_or_else(|| ArcliError::ClaimNotFound(id_upper.clone()))?;

    if let Some(s) = statement {
        claim.statement = s.to_string();
    }
    if let Some(f) = falsification {
        claim.falsification = f.to_string();
    }
    claim.updated_at = now();
    save_claims(&path, &cf)?;
    Ok(())
}

pub fn remove(repo: &Repository, id: &str, force: bool) -> Result<()> {
    let path = repo.claims_path();
    let mut cf = load_claims(&path)?;
    let id_upper = id.to_uppercase();

    let claim = cf
        .claims
        .get(&id_upper)
        .ok_or_else(|| ArcliError::ClaimNotFound(id_upper.clone()))?;

    if !force {
        if !claim.verified_by.is_empty() {
            return Err(ArcliError::ClaimHasExperiments(id_upper));
        }
        let db = Database::open(&repo.db_path())?;
        let refs = db.get_experiments_referencing_claim(&id_upper)?;
        if !refs.is_empty() {
            return Err(ArcliError::ClaimHasExperiments(id_upper));
        }
    }

    cf.claims.remove(&id_upper);
    save_claims(&path, &cf)?;
    Ok(())
}
