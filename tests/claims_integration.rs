use std::fs;

use arcli::commands::claim;
use arcli::commands::exp;
use arcli::db::Database;
use arcli::repo::Repository;

fn setup() -> (Repository, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let _ = git2::Repository::init(root);
    fs::create_dir_all(root.join(".research")).unwrap();
    fs::create_dir_all(root.join(".research/hooks")).unwrap();
    fs::write(root.join(".research/hooks/pre-experiment"), "#!/bin/sh\n").unwrap();
    fs::create_dir_all(root.join("experiments")).unwrap();

    let config = arcli::config::Config::default();
    config.save(&root.join(".research/config.yaml")).unwrap();

    let db = Database::open(&root.join(".research/research.db")).unwrap();
    db.init_schema().unwrap();

    // Create .gitignore to ignore SQLite temp files
    fs::write(
        root.join(".gitignore"),
        ".research/*.db*\n.research/*.db-wal\n.research/*.db-shm\n",
    )
    .unwrap();

    // commit for clean workspace
    let git_repo = git2::Repository::open(root).unwrap();
    let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
    let mut index = git_repo.index().unwrap();
    index
        .add_all(["."], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = git_repo.find_tree(tree_id).unwrap();
    git_repo
        .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();

    (Repository { root: root.to_path_buf() }, dir)
}

fn commit_all(repo: &Repository) {
    let git_repo = git2::Repository::open(&repo.root).unwrap();
    let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
    let mut index = git_repo.index().unwrap();
    index
        .add_all(["."], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = git_repo.find_tree(tree_id).unwrap();
    let parent = git_repo.head().unwrap().peel_to_commit().unwrap();
    git_repo
        .commit(Some("HEAD"), &sig, &sig, "test commit", &tree, &[&parent])
        .unwrap();
}

#[test]
fn test_add_and_list_claims() {
    let (repo, _dir) = setup();

    claim::add(
        &repo,
        "C1",
        "Attention gate improves F1 by 5pp",
        "F1 improvement < 2pp",
    )
    .unwrap();

    let list = claim::list(&repo).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "C1");

    // duplicate fails
    let err = claim::add(
        &repo,
        "C1",
        "Another statement",
        "Another falsification",
    )
    .unwrap_err();
    assert!(err.to_string().contains("已存在"));
}

#[test]
fn test_show_claim() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C1", "Claim statement", "Falsification").unwrap();

    let detail = claim::show(&repo, "C1").unwrap();
    assert_eq!(detail.id, "C1");
    assert_eq!(detail.statement, "Claim statement");
    assert_eq!(detail.falsification, "Falsification");
    assert_eq!(detail.verified_by.len(), 0);
    assert_eq!(detail.created_at, detail.updated_at);

    // non-existent
    let err = claim::show(&repo, "C_nope").unwrap_err();
    assert!(err.to_string().contains("未找到"));
}

#[test]
fn test_update_claim() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C2", "Old statement", "Old falsification").unwrap();
    claim::update(&repo, "C2", Some("New statement"), Some("New falsification")).unwrap();

    let detail = claim::show(&repo, "C2").unwrap();
    assert_eq!(detail.statement, "New statement");
    assert_eq!(detail.falsification, "New falsification");

    // partial update: only statement
    claim::update(&repo, "C2", Some("Only statement"), None).unwrap();
    let detail = claim::show(&repo, "C2").unwrap();
    assert_eq!(detail.statement, "Only statement");
    assert_eq!(detail.falsification, "New falsification");
}

#[test]
fn test_verify_and_unverify_claim() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C3", "Testable claim", "When it fails").unwrap();
    commit_all(&repo);

    // Create an experiment to bind
    let (exp_id, _) = exp::new(
        &repo,
        None,
        Some("echo test".to_string()),
        true,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    commit_all(&repo);

    // Verify (bind experiment to claim)
    claim::verify(&repo, "C3", &exp_id).unwrap();

    let detail = claim::show(&repo, "C3").unwrap();
    assert_eq!(detail.verified_by.len(), 1);
    assert_eq!(detail.verified_by[0].exp_id, exp_id);

    // Duplicate verify should fail
    let err = claim::verify(&repo, "C3", &exp_id).unwrap_err();
    assert!(err.to_string().contains("已被该实验验证"));

    // Unverify
    claim::unverify(&repo, "C3", &exp_id).unwrap();
    let detail = claim::show(&repo, "C3").unwrap();
    assert_eq!(detail.verified_by.len(), 0);

    // Unverify non-existent binding should fail
    let err = claim::unverify(&repo, "C3", &exp_id).unwrap_err();
    assert!(err.to_string().contains("未找到"));
}

#[test]
fn test_remove_claim() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C4", "Removable claim", "Anything").unwrap();
    commit_all(&repo);

    // Create an experiment bound to this claim
    exp::new(
        &repo,
        None,
        Some("echo test".to_string()),
        true,
        None,
        None,
        None,
        None,
        None,
        Some("C4".to_string()),
        None,
    )
    .unwrap();
    commit_all(&repo);

    // Remove without --force should fail because experiment is linked
    let err = claim::remove(&repo, "C4", false).unwrap_err();
    assert!(err.to_string().contains("仍有实验引用"));

    // Remove with --force should succeed
    claim::remove(&repo, "C4", true).unwrap();

    // Claim should be gone
    let err = claim::show(&repo, "C4").unwrap_err();
    assert!(err.to_string().contains("未找到"));
}

#[test]
fn test_remove_nonexistent_claim_fails() {
    let (repo, _dir) = setup();
    let err = claim::remove(&repo, "no_such_claim", true).unwrap_err();
    assert!(err.to_string().contains("未找到"));
}

#[test]
fn test_exp_new_with_claims_and_hypothesis() {
    let (repo, _dir) = setup();

    // Create claims first
    claim::add(&repo, "C5", "Claim 5", "Falsify 5").unwrap();
    claim::add(&repo, "C6", "Claim 6", "Falsify 6").unwrap();
    commit_all(&repo);

    let (exp_id, _) = exp::new(
        &repo,
        None,
        Some("echo hello".to_string()),
        true,
        None,
        None,
        None,
        None,
        None,
        Some("C5,C6".to_string()),
        Some("Testing if C5 and C6 hold".to_string()),
    )
    .unwrap();

    // Check experiment.json
    let exp_json_path = repo.exp_json_path(&exp_id);
    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&exp_json_path).unwrap()).unwrap();
    let claims: Vec<String> = content["relates_to_claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(claims.contains(&"C5".to_string()));
    assert!(claims.contains(&"C6".to_string()));
    assert_eq!(
        content["hypothesis"].as_str().unwrap(),
        "Testing if C5 and C6 hold"
    );
}

#[test]
fn test_exp_claim_add_and_remove() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C7", "Claim 7", "Falsify 7").unwrap();
    claim::add(&repo, "C8", "Claim 8", "Falsify 8").unwrap();
    commit_all(&repo);

    let (exp_id, _) = exp::new(
        &repo,
        None,
        Some("echo test".to_string()),
        true,
        None,
        None,
        None,
        None,
        None,
        Some("C7".to_string()),
        None,
    )
    .unwrap();
    commit_all(&repo);

    // Add another claim
    exp::add_claim(&repo, &exp_id, "C8").unwrap();

    let exp_json_path = repo.exp_json_path(&exp_id);
    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&exp_json_path).unwrap()).unwrap();
    let claims: Vec<String> = content["relates_to_claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(claims.contains(&"C7".to_string()));
    assert!(claims.contains(&"C8".to_string()));

    // Remove claim C7
    exp::remove_claim(&repo, &exp_id, "C7").unwrap();

    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&exp_json_path).unwrap()).unwrap();
    let claims: Vec<String> = content["relates_to_claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(!claims.contains(&"C7".to_string()));
    assert!(claims.contains(&"C8".to_string()));
}

#[test]
fn test_exp_hypothesis_and_lesson() {
    let (repo, _dir) = setup();

    let (exp_id, _) = exp::new(
        &repo,
        None,
        Some("echo test".to_string()),
        true,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    commit_all(&repo);

    // Set hypothesis
    exp::set_hypothesis(&repo, &exp_id, "The model will converge in 100 steps").unwrap();

    let exp_json_path = repo.exp_json_path(&exp_id);
    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&exp_json_path).unwrap()).unwrap();
    assert_eq!(
        content["hypothesis"].as_str().unwrap(),
        "The model will converge in 100 steps"
    );

    // Set lesson
    exp::set_lesson(
        &repo,
        &exp_id,
        "Convergence required 200 steps due to high LR",
    )
    .unwrap();

    let content: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&exp_json_path).unwrap()).unwrap();
    assert_eq!(
        content["lesson"].as_str().unwrap(),
        "Convergence required 200 steps due to high LR"
    );
}

#[test]
fn test_verify_claim_updates_claims_yaml() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C9", "Persistent claim", "Falsification condition").unwrap();
    commit_all(&repo);

    let (exp_id, _) = exp::new(
        &repo,
        None,
        Some("echo test".to_string()),
        true,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    commit_all(&repo);

    claim::verify(&repo, "C9", &exp_id).unwrap();

    // Reload claims from disk and check persistence
    let cf = claim::load_claims(&repo.claims_path()).unwrap();
    let c = cf.claims.get("C9").unwrap();
    assert!(c.verified_by.contains(&exp_id));
}

#[test]
fn test_claim_verify_nonexistent_experiment_fails() {
    let (repo, _dir) = setup();

    claim::add(&repo, "C10", "Claim 10", "Falsify 10").unwrap();

    let err = claim::verify(&repo, "C10", "no_such_exp").unwrap_err();
    assert!(err.to_string().contains("未找到"));
}
