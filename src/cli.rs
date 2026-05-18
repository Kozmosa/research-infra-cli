use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "arcli")]
#[command(about = "Research CLI - 为 AI agent 和人类研究员提供预结构化、可复现的研究工作环境")]
#[command(version = "0.2.0")]
pub struct Cli {
    #[arg(long, env = "RESEARCH_REPO", help = "指定仓库根目录")]
    pub repo: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(subcommand)]
    Project(ProjectCommands),

    #[command(subcommand)]
    Env(EnvCommands),

    #[command(subcommand)]
    Data(DataCommands),

    #[command(subcommand)]
    Exp(ExpCommands),

    #[command(subcommand)]
    Claim(ClaimCommands),

    #[command(subcommand)]
    Db(DbCommands),

    #[command(subcommand)]
    Log(LogCommands),

    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Clone, ValueEnum)]
pub enum ResearchType {
    Ml,
    Data,
    Math,
    Generic,
}

#[derive(Clone, ValueEnum)]
pub enum TechStack {
    Python,
    Rust,
    Julia,
    Go,
}

#[derive(Subcommand)]
pub enum ClaimCommands {
    Add {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "Claim ID (e.g. C1, C2)")]
        id: String,

        #[arg(long, help = "可证伪的论断")]
        statement: String,

        #[arg(long, help = "什么条件会推翻这个 claim")]
        falsification: String,
    },
    List {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,
    },
    Show {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "Claim ID")]
        id: String,
    },
    Verify {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "Claim ID")]
        id: String,

        #[arg(long, help = "绑定的实验 ID")]
        exp: String,
    },
    Unverify {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "Claim ID")]
        id: String,

        #[arg(long, help = "解除绑定的实验 ID")]
        exp: String,
    },
    Update {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "Claim ID")]
        id: String,

        #[arg(long, help = "更新 statement")]
        statement: Option<String>,

        #[arg(long, help = "更新 falsification")]
        falsification: Option<String>,
    },
    Remove {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "Claim ID")]
        id: String,

        #[arg(long, help = "强制删除（即使有实验关联）")]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ProjectCommands {
    Init {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "目标目录，默认当前目录")]
        path: Option<String>,

        #[arg(long, help = "项目名称")]
        name: Option<String>,

        #[arg(long, help = "允许在非空目录下初始化")]
        force: bool,

        #[arg(long, default_value = "experiments", help = "实验目录的名称")]
        exp_dir: String,

        #[arg(long, value_enum, help = "研究类型")]
        research_type: Option<ResearchType>,

        #[arg(long, value_enum, help = "技术栈")]
        stack: Option<TechStack>,

        #[arg(long, help = "生成中文 README")]
        zh: bool,
    },
}

#[derive(Subcommand)]
pub enum EnvCommands {
    Status {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,
    },
    Check {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(long, help = "严格检查工作区是否干净")]
        strict: bool,
    },
}

#[derive(Subcommand)]
pub enum DataCommands {
    Register {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "数据目录的路径（相对于仓库根）")]
        path: String,

        #[arg(long, help = "资产唯一标识符")]
        name: String,

        #[arg(long, help = "人类可读描述")]
        desc: Option<String>,

        #[arg(long, help = "手动提供 SHA256 校验和")]
        checksum: Option<String>,
    },
    List {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,
    },
    Info {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "资产名称")]
        name: String,
    },
    Update {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "资产名称")]
        name: String,

        #[arg(long, help = "新路径")]
        path: Option<String>,

        #[arg(long, help = "重新计算校验和")]
        recompute_checksum: bool,
    },
    Verify {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(long, help = "仅显示变更的资产")]
        changed_only: bool,
    },
}

#[derive(Subcommand)]
pub enum ExpCommands {
    New {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(
            long,
            help = "已注册的数据资产名称",
            required_unless_present = "manual"
        )]
        data: Option<String>,

        #[arg(long, help = "要执行的 shell 命令", required_unless_present = "manual")]
        cmd: Option<String>,

        #[arg(long, help = "手动创建实验记录")]
        manual: bool,

        #[arg(long, help = "可选短标签")]
        label: Option<String>,

        #[arg(long, help = "初始超参 JSON")]
        params: Option<String>,

        #[arg(long, help = "实验目的或备忘")]
        notes: Option<String>,

        #[arg(long, help = "环境名")]
        env: Option<String>,

        #[arg(long, help = "实验模板")]
        template: Option<String>,

        #[arg(long, help = "关联的 claim ID（逗号分隔）")]
        claims: Option<String>,

        #[arg(long, help = "该实验试图测试什么假设")]
        hypothesis: Option<String>,
    },
    Run {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "超时时间（秒），超时后终止并标记 interrupted")]
        timeout: Option<u64>,

        #[arg(trailing_var_arg = true, help = "追加到命令的额外参数")]
        args: Vec<String>,
    },
    Stop {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, default_value = "SIGTERM", help = "信号类型")]
        signal: String,
    },
    Status {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: Option<String>,
    },
    Metric {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "Step 编号")]
        step: i64,

        #[arg(long = "metrics", help = "指标 JSON")]
        metrics_json: Option<String>,

        #[arg(long = "key", help = "指标键", num_args = 0..)]
        keys: Vec<String>,

        #[arg(long = "val", help = "指标值", num_args = 0..)]
        vals: Vec<String>,
    },
    Param {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long = "params", help = "参数 JSON")]
        params_json: String,
    },
    Finish {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "状态")]
        status: String,

        #[arg(long, help = "消息")]
        message: Option<String>,
    },
    Export {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "输出路径")]
        output: Option<String>,
    },
    List {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(long, help = "按状态过滤")]
        status: Option<String>,

        #[arg(long, help = "按日期过滤")]
        since: Option<String>,
    },
    Import {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "要导入的目录路径")]
        path: String,

        #[arg(long, help = "实验标签（必需）")]
        label: String,

        #[arg(long, help = "原始实验命令（必需）")]
        cmd: String,

        #[arg(long, help = "关联的数据资产名称")]
        data: Option<String>,

        #[arg(long, help = "移动目录而非复制")]
        move_dir: bool,

        #[arg(long, help = "跳过确认提示")]
        yes: bool,
    },
    Diff {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID 1")]
        exp_id_1: String,

        #[arg(help = "实验 ID 2")]
        exp_id_2: String,

        #[arg(long, help = "显示完整 diff 内容")]
        full: bool,
    },
    /// 管理实验的 claim 关联
    Claim {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "添加 claim 关联")]
        add: Option<String>,

        #[arg(long, help = "移除 claim 关联")]
        remove: Option<String>,
    },
    /// 设置实验 hypothesis
    Hypothesis {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "hypothesis 文本")]
        set: String,
    },
    /// 记录实验经验教训
    Lesson {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "lesson 文本")]
        set: String,
    },
}

#[derive(Subcommand)]
pub enum DbCommands {
    Sync {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(long, default_value = "auto", help = "同步模式")]
        mode: String,
    },
    ExportAll {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(long, help = "输出目录")]
        out_dir: Option<String>,
    },
    Import {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(long, help = "来源路径")]
        from: String,
    },
    Status {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum LogCommands {
    Show {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "显示最后 N 行")]
        tail: Option<usize>,

        #[arg(long, help = "持续输出新内容")]
        follow: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    Get {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "配置键")]
        key: String,
    },
    Set {
        #[arg(long, global = true, help = "以 JSON 格式输出")]
        json: bool,

        #[arg(help = "配置键")]
        key: String,

        #[arg(help = "配置值")]
        value: String,
    },
}
