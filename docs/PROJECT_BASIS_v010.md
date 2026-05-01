# arcli — Agentic Research Command Line Infrastructure

> 本文档描述 arcli v0.1.0 的项目定位、技术栈、仓库结构与核心工作流。

---

## 1. 项目定位

**arcli**（Agentic Research Command Line Infrastructure）是一套用 Rust 编写的命令行工具，为 AI Agent 和人类研究员提供预结构化、可复现、低熵的研究工作环境。

- **技术栈**: Rust 2024 Edition + clap + SQLite + git2
- **目标用户**: AI Agent（主）、人类研究员（辅）
- **核心问题**: 消除 Agent 的"即兴探索"行为，将实验完整生命周期纳入确定性的文件系统契约

---

## 2. 仓库结构

```
research-infra-cli/
├── src/
│   ├── main.rs              # CLI 入口，命令分发
│   ├── cli.rs               # clap 派生宏（所有子命令参数）
│   ├── lib.rs               # 库入口
│   ├── repo.rs              # 仓库发现、路径解析
│   ├── config.rs            # 配置读写（点号键支持）
│   ├── db.rs                # SQLite 封装（schema、CRUD、WAL 模式）
│   ├── error.rs             # 统一错误枚举 + error_code
│   ├── output.rs            # JSON / 文本输出格式化
│   ├── templates/           # 模板文件（编译时内嵌）
│   │   ├── gitignore/
│   │   ├── readme/
│   │   ├── config/
│   │   ├── experiment/
│   │   └── docs/
│   └── commands/
│       ├── mod.rs
│       ├── project.rs       # project init（模板脚手架）
│       ├── env.rs           # env status / check
│       ├── data.rs          # data register / list / info / update
│       ├── exp.rs           # exp new / run / stop / status / metric / param / finish
│       ├── db.rs            # db sync / export-all / import / status
│       ├── log.rs           # log show（tail / follow）
│       └── config.rs        # config get / set
├── docs/                    # 项目文档
│   ├── PROJECT_BASIS_v010.md
│   └── PROJECT_BASIS_v010_VIS.html
├── Cargo.toml
├── Cargo.lock
├── build.rs                 # Windows 下链接 advapi32
├── README.md                # 项目总览
├── PRD.md                   # 产品需求文档
├── LICENSE
└── .github/workflows/       # CI/CD
    ├── ci.yml
    └── release.yml
```

---

## 3. 核心设计

### 3.1 模板脚手架（`project init`）

`project init` 生成标准研究仓库：

| 参数 | 说明 |
|------|------|
| `--name` | 项目名称 |
| `--force` | 允许非空目录初始化 |
| `--exp-dir` | 实验目录名（默认 `experiments`） |
| `--research-type` | `ml` / `data` / `math` / `generic` |
| `--stack` | `python` / `rust` / `julia` / `go`（默认 Python） |
| `--zh` | 生成中文 README |

生成的目录结构：

```
<repo_root>/
├── data/
│   ├── raw/                 # 原始数据，Agent 只读
│   └── processed/           # 处理后数据
├── experiments/             # 实验容器
│   └── run-001-YYYYMMDD-HHMM_label/
│       ├── experiment.json  # 完整实验记录（版本控制）
│       ├── artifacts/       # 模型、图表等产出
│       └── logs/
│           └── run.log      # stdout/stderr 捕获
├── src/                     # 源代码
├── tests/                   # 测试代码
├── artifacts/               # 项目级共享制品
├── docs/                    # 文档
└── .research/               # arcli 内部数据
    ├── config.yaml          # 项目配置
    ├── research.db          # SQLite 运行时缓存（gitignore）
    ├── templates/           # 实验模板
    └── hooks/               # 可选 hooks
```

研究类型额外目录：
- `ml` → `models/` + `checkpoints/`
- `data` → `notebooks/` + `reports/`
- `math` → `proofs/` + `formulations/`

### 3.2 实验生命周期

```
created --run--> running --exit(0)--> finished
             |          |
             |          |--exit(!0)--> failed
             |          |
             |          |--SIGTERM--> interrupted --resume--> running
             |----------|
           (manual/resume)
```

### 3.3 双轨持久化

| 存储 | 格式 | 用途 | 版本控制 |
|------|------|------|---------|
| SQLite | `.research/research.db` | 运行时缓存、高效查询 | **忽略** |
| JSON | `experiments/*/experiment.json` | 事实来源、可复现 | **纳入** |

同步命令：

```bash
# SQLite -> JSON（实验后执行）
arcli db sync --mode export

# JSON -> SQLite（拉取新实验后执行）
arcli db sync --mode import

# 自动比较时间戳合并（pre-commit 钩子推荐）
arcli db sync --mode auto
```

---

## 4. 命令总览

| 命令组 | 命令 | 说明 |
|--------|------|------|
| `project` | `init [PATH]` | 初始化研究仓库 |
| `env` | `status` | 环境快照 |
| `env` | `check --strict` | 严格检查工作区 |
| `data` | `register PATH --name NAME` | 注册数据资产 |
| `data` | `list` | 列出数据资产 |
| `data` | `info NAME` | 查看数据详情 |
| `data` | `update NAME` | 更新数据信息 |
| `exp` | `new --data NAME --cmd CMD` | 创建实验 |
| `exp` | `run EXP_ID` | 执行实验 |
| `exp` | `stop EXP_ID` | 终止实验 |
| `exp` | `status [EXP_ID]` | 查看状态 |
| `exp` | `metric EXP_ID --step N --json JSON` | 记录指标 |
| `exp` | `param EXP_ID --params JSON` | 更新参数 |
| `exp` | `finish EXP_ID --status STATUS` | 标记完成 |
| `exp` | `list` | 列出实验 |
| `db` | `sync --mode auto|export|import` | 双向同步 |
| `db` | `status` | 查看同步状态 |
| `log` | `show EXP_ID --tail N --follow` | 查看日志 |
| `config` | `get KEY` | 读取配置 |
| `config` | `set KEY VALUE` | 设置配置 |

---

## 5. 技术实现要点

- **并发安全**: SQLite WAL 模式 + 原子 `UPDATE ... RETURNING` 分配 short_id
- **无状态**: arcli 不维护常驻进程，所有状态存储在仓库文件中
- **Agent 原生**: 所有命令支持 `--json`，输出机器可解析
- **TDD**: 62 个单元测试覆盖全部核心功能
- **CI/CD**: GitHub Actions，自动 fmt/check/clippy/test/build

---

*Generated for arcli v0.1.0*
