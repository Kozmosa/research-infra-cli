use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "arcli")]
#[command(about = "Research CLI - 为 AI agent 和人类研究员提供预结构化、可复现的研究工作环境")]
#[command(version = "0.1.0")]
pub struct Cli {
    #[arg(long, env = "RESEARCH_REPO", help = "指定仓库根目录")]
    pub repo: Option<String>,

    #[arg(long, help = "以 JSON 格式输出")]
    pub json: bool,

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
pub enum ProjectCommands {
    Init {
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
    Status,
    Check {
        #[arg(long, help = "严格检查工作区是否干净")]
        strict: bool,
    },
}

#[derive(Subcommand)]
pub enum DataCommands {
    Register {
        #[arg(help = "数据目录的路径（相对于仓库根）")]
        path: String,

        #[arg(long, help = "资产唯一标识符")]
        name: String,

        #[arg(long, help = "人类可读描述")]
        desc: Option<String>,

        #[arg(long, help = "手动提供 SHA256 校验和")]
        checksum: Option<String>,
    },
    List,
    Info {
        #[arg(help = "资产名称")]
        name: String,
    },
    Update {
        #[arg(help = "资产名称")]
        name: String,

        #[arg(long, help = "新路径")]
        path: Option<String>,

        #[arg(long, help = "重新计算校验和")]
        recompute_checksum: bool,
    },
}

#[derive(Subcommand)]
pub enum ExpCommands {
    New {
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
    },
    Run {
        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(trailing_var_arg = true, help = "追加到命令的额外参数")]
        args: Vec<String>,
    },
    Stop {
        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, default_value = "SIGTERM", help = "信号类型")]
        signal: String,
    },
    Status {
        #[arg(help = "实验 ID")]
        exp_id: Option<String>,
    },
    Metric {
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
        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long = "params", help = "参数 JSON")]
        params_json: String,
    },
    Finish {
        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "状态")]
        status: String,

        #[arg(long, help = "消息")]
        message: Option<String>,
    },
    Export {
        #[arg(help = "实验 ID")]
        exp_id: String,

        #[arg(long, help = "输出路径")]
        output: Option<String>,
    },
    List {
        #[arg(long, help = "按状态过滤")]
        status: Option<String>,

        #[arg(long, help = "按日期过滤")]
        since: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DbCommands {
    Sync {
        #[arg(long, default_value = "auto", help = "同步模式")]
        mode: String,
    },
    ExportAll {
        #[arg(long, help = "输出目录")]
        out_dir: Option<String>,
    },
    Import {
        #[arg(long, help = "来源路径")]
        from: String,
    },
    Status,
}

#[derive(Subcommand)]
pub enum LogCommands {
    Show {
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
        #[arg(help = "配置键")]
        key: String,
    },
    Set {
        #[arg(help = "配置键")]
        key: String,

        #[arg(help = "配置值")]
        value: String,
    },
}
