use std::fmt;

#[derive(Debug)]
pub enum RcliError {
    WorkspaceNotClean,
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

impl fmt::Display for RcliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RcliError::WorkspaceNotClean => write!(f, "工作区不干净，请提交或暂存更改"),
            RcliError::DataNotFound(name) => write!(f, "数据资产 '{}' 未找到", name),
            RcliError::DataAlreadyExists(name) => write!(f, "数据资产 '{}' 已存在", name),
            RcliError::ExpIdExists(id) => write!(f, "实验 ID '{}' 已存在", id),
            RcliError::MissingRequiredArg(arg) => write!(f, "缺少必需参数: {}", arg),
            RcliError::RepoNotFound => write!(f, "未找到研究仓库，请确认当前目录在仓库内"),
            RcliError::InvalidStatus(s) => write!(f, "无效的状态: {}", s),
            RcliError::ConfigKeyNotFound(k) => write!(f, "配置键 '{}' 未找到", k),
            RcliError::SyncConflict(ids) => write!(f, "同步冲突的实验: {:?}", ids),
            RcliError::Io(e) => write!(f, "IO 错误: {}", e),
            RcliError::Sqlite(e) => write!(f, "数据库错误: {}", e),
            RcliError::Yaml(e) => write!(f, "YAML 错误: {}", e),
            RcliError::Json(e) => write!(f, "JSON 错误: {}", e),
            RcliError::Git(e) => write!(f, "Git 错误: {}", e),
            RcliError::Walkdir(e) => write!(f, "目录遍历错误: {}", e),
            RcliError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for RcliError {}

impl From<std::io::Error> for RcliError {
    fn from(e: std::io::Error) -> Self {
        RcliError::Io(e)
    }
}

impl From<rusqlite::Error> for RcliError {
    fn from(e: rusqlite::Error) -> Self {
        RcliError::Sqlite(e)
    }
}

impl From<serde_yaml::Error> for RcliError {
    fn from(e: serde_yaml::Error) -> Self {
        RcliError::Yaml(e)
    }
}

impl From<serde_json::Error> for RcliError {
    fn from(e: serde_json::Error) -> Self {
        RcliError::Json(e)
    }
}

impl From<git2::Error> for RcliError {
    fn from(e: git2::Error) -> Self {
        RcliError::Git(e)
    }
}

impl From<walkdir::Error> for RcliError {
    fn from(e: walkdir::Error) -> Self {
        RcliError::Walkdir(e)
    }
}

impl RcliError {
    pub fn error_code(&self) -> &'static str {
        match self {
            RcliError::WorkspaceNotClean => "WORKSPACE_NOT_CLEAN",
            RcliError::DataNotFound(_) => "DATA_NOT_FOUND",
            RcliError::DataAlreadyExists(_) => "DATA_ALREADY_EXISTS",
            RcliError::ExpIdExists(_) => "EXP_ID_EXISTS",
            RcliError::MissingRequiredArg(_) => "MISSING_REQUIRED_ARG",
            RcliError::RepoNotFound => "REPO_NOT_FOUND",
            RcliError::InvalidStatus(_) => "INVALID_STATUS",
            RcliError::ConfigKeyNotFound(_) => "CONFIG_KEY_NOT_FOUND",
            RcliError::SyncConflict(_) => "SYNC_CONFLICT",
            RcliError::Io(_) => "IO_ERROR",
            RcliError::Sqlite(_) => "SQLITE_ERROR",
            RcliError::Yaml(_) => "YAML_ERROR",
            RcliError::Json(_) => "JSON_ERROR",
            RcliError::Git(_) => "GIT_ERROR",
            RcliError::Walkdir(_) => "WALKDIR_ERROR",
            RcliError::Other(_) => "UNKNOWN_ERROR",
        }
    }
}

pub type Result<T> = std::result::Result<T, RcliError>;
