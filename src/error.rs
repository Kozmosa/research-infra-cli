use std::fmt;

#[derive(Debug)]
pub enum ArcliError {
    WorkspaceNotClean,
    ReadinessCheckFailed(String),
    DataNotFound(String),
    DataAlreadyExists(String),
    ExpIdExists(String),
    MissingRequiredArg(String),
    RepoNotFound,
    InvalidStatus(String),
    ConfigKeyNotFound(String),
    SyncConflict(Vec<String>),
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Yaml(serde_yaml::Error),
    Json(serde_json::Error),
    Git(git2::Error),
    Walkdir(walkdir::Error),
    Other(String),
}

impl fmt::Display for ArcliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArcliError::WorkspaceNotClean => write!(f, "工作区不干净，请提交或暂存更改"),
            ArcliError::ReadinessCheckFailed(msg) => write!(f, "环境就绪检查失败: {}", msg),
            ArcliError::DataNotFound(name) => write!(f, "数据资产 '{}' 未找到", name),
            ArcliError::DataAlreadyExists(name) => write!(f, "数据资产 '{}' 已存在", name),
            ArcliError::ExpIdExists(id) => write!(f, "实验 ID '{}' 已存在", id),
            ArcliError::MissingRequiredArg(arg) => write!(f, "缺少必需参数: {}", arg),
            ArcliError::RepoNotFound => write!(f, "未找到研究仓库，请确认当前目录在仓库内"),
            ArcliError::InvalidStatus(s) => write!(f, "无效的状态: {}", s),
            ArcliError::ConfigKeyNotFound(k) => write!(f, "配置键 '{}' 未找到", k),
            ArcliError::SyncConflict(ids) => write!(f, "同步冲突的实验: {:?}", ids),
            ArcliError::Io(e) => write!(f, "IO 错误: {}", e),
            ArcliError::Sqlite(e) => write!(f, "数据库错误: {}", e),
            ArcliError::Yaml(e) => write!(f, "YAML 错误: {}", e),
            ArcliError::Json(e) => write!(f, "JSON 错误: {}", e),
            ArcliError::Git(e) => write!(f, "Git 错误: {}", e),
            ArcliError::Walkdir(e) => write!(f, "目录遍历错误: {}", e),
            ArcliError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for ArcliError {}

impl From<std::io::Error> for ArcliError {
    fn from(e: std::io::Error) -> Self {
        ArcliError::Io(e)
    }
}

impl From<rusqlite::Error> for ArcliError {
    fn from(e: rusqlite::Error) -> Self {
        ArcliError::Sqlite(e)
    }
}

impl From<serde_yaml::Error> for ArcliError {
    fn from(e: serde_yaml::Error) -> Self {
        ArcliError::Yaml(e)
    }
}

impl From<serde_json::Error> for ArcliError {
    fn from(e: serde_json::Error) -> Self {
        ArcliError::Json(e)
    }
}

impl From<git2::Error> for ArcliError {
    fn from(e: git2::Error) -> Self {
        ArcliError::Git(e)
    }
}

impl From<walkdir::Error> for ArcliError {
    fn from(e: walkdir::Error) -> Self {
        ArcliError::Walkdir(e)
    }
}

impl ArcliError {
    pub fn error_code(&self) -> &'static str {
        match self {
            ArcliError::WorkspaceNotClean => "WORKSPACE_NOT_CLEAN",
            ArcliError::ReadinessCheckFailed(_) => "READINESS_CHECK_FAILED",
            ArcliError::DataNotFound(_) => "DATA_NOT_FOUND",
            ArcliError::DataAlreadyExists(_) => "DATA_ALREADY_EXISTS",
            ArcliError::ExpIdExists(_) => "EXP_ID_EXISTS",
            ArcliError::MissingRequiredArg(_) => "MISSING_REQUIRED_ARG",
            ArcliError::RepoNotFound => "REPO_NOT_FOUND",
            ArcliError::InvalidStatus(_) => "INVALID_STATUS",
            ArcliError::ConfigKeyNotFound(_) => "CONFIG_KEY_NOT_FOUND",
            ArcliError::SyncConflict(_) => "SYNC_CONFLICT",
            ArcliError::Io(_) => "IO_ERROR",
            ArcliError::Sqlite(_) => "SQLITE_ERROR",
            ArcliError::Yaml(_) => "YAML_ERROR",
            ArcliError::Json(_) => "JSON_ERROR",
            ArcliError::Git(_) => "GIT_ERROR",
            ArcliError::Walkdir(_) => "WALKDIR_ERROR",
            ArcliError::Other(_) => "UNKNOWN_ERROR",
        }
    }
}

pub type Result<T> = std::result::Result<T, ArcliError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes_are_consistent() {
        let errors = vec![
            (ArcliError::WorkspaceNotClean, "WORKSPACE_NOT_CLEAN"),
            (
                ArcliError::ReadinessCheckFailed("hooks missing".to_string()),
                "READINESS_CHECK_FAILED",
            ),
            (ArcliError::DataNotFound("x".to_string()), "DATA_NOT_FOUND"),
            (
                ArcliError::DataAlreadyExists("x".to_string()),
                "DATA_ALREADY_EXISTS",
            ),
            (ArcliError::ExpIdExists("x".to_string()), "EXP_ID_EXISTS"),
            (
                ArcliError::MissingRequiredArg("x".to_string()),
                "MISSING_REQUIRED_ARG",
            ),
            (ArcliError::RepoNotFound, "REPO_NOT_FOUND"),
            (ArcliError::InvalidStatus("x".to_string()), "INVALID_STATUS"),
            (
                ArcliError::ConfigKeyNotFound("x".to_string()),
                "CONFIG_KEY_NOT_FOUND",
            ),
            (
                ArcliError::SyncConflict(vec!["x".to_string()]),
                "SYNC_CONFLICT",
            ),
            (ArcliError::Io(std::io::Error::other("x")), "IO_ERROR"),
            (
                ArcliError::Sqlite(rusqlite::Error::InvalidQuery),
                "SQLITE_ERROR",
            ),
            (ArcliError::Other("x".to_string()), "UNKNOWN_ERROR"),
        ];

        for (err, expected_code) in errors {
            assert_eq!(
                err.error_code(),
                expected_code,
                "错误 {:?} 的 error_code 应为 {}",
                err,
                expected_code
            );
            let msg = format!("{}", err);
            assert!(!msg.is_empty(), "错误 {:?} 的消息不应为空", err);
        }
    }

    #[test]
    fn test_error_display_includes_context() {
        let err = ArcliError::DataNotFound("imdb-v1".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("imdb-v1"), "错误消息应包含上下文信息");

        let err = ArcliError::MissingRequiredArg("--data".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("--data"), "错误消息应包含参数名");
    }
}
