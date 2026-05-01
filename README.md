# arcli — Agentic Research Command Line Infrastructure

> 为 AI Agent 与人类研究员打造的**预结构化、可复现、低熵**研究工作环境。

[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![SQLite](https://img.shields.io/badge/SQLite-WAL-blue?logo=sqlite)](https://sqlite.org/)
[![clap](https://img.shields.io/badge/clap-derive%20CLI-green)](https://github.com/clap-rs/clap)
[![JSON](https://img.shields.io/badge/output-JSON%20%7C%20text-lightgrey)](https://www.json.org/)
[![License](https://img.shields.io/badge/license-MIT-purple)](LICENSE)

---

## ✨ What is arcli

`arcli` 是一套用 **Rust** 编写的命令行工具，它定义了一套严格的研究仓库规范，将实验的完整生命周期（创建 → 执行 → 监控 → 归档）纳入**确定性的、可版本控制的文件系统契约**中。

- **预结构化 scaffolding**：`arcli project init` 一键生成标准目录树，Agent 不再即兴创建目录
- **实验即契约**：`arcli exp new` 强制声明数据、命令、环境，生成唯一 ID 并锁定代码版本
- **双轨持久化**：SQLite 作为运行时缓存，JSON 作为版本控制的事实来源，双向自动同步
- **Agent 原生**：所有命令支持 `--json`，输出为机器可解析的确定性 ground truth

---

## 🖥️ Console Preview

```text
$ arcli env status

仓库根目录: /home/user/my-project
当前分支: main
提交哈希: a1b2c3d
工作区干净: true
最后提交时间: 2026-04-28T10:30:00Z

活跃实验:
  run-001-20260428-1030_baseline — running

数据资产:
  imdb-v1
  cifar-10

配置:
  project_name: my-project
  experiments_dir: experiments

$ arcli exp list --status running

ID                          Status   Command
run-001-20260428-1030_baseline  running  python train.py --epochs 10
```

---

## 🚀 Why arcli

| 没有 arcli | 使用 arcli |
|---|---|
| Agent 每次运行创建不同的目录结构，人类无法理解 | `project init` 生成**标准化目录树**，所有实验按统一规范存放 |
| 大量 token 浪费在环境探测和 shell 命令推断上 | `env status` **一次性注入** git 状态、数据资产、活跃实验等 ground truth |
| 实验记录散落在各处，无法追溯代码版本和数据关联 | `exp new` **强制契约**：数据资产、命令、commit hash 全部落库 |
| 手动管理实验状态，容易遗漏或冲突 | SQLite WAL + 原子 `UPDATE ... RETURNING` 保证**并发无冲突** |
| 人类与 Agent 各自维护一份实验记录，信息不同步 | `db sync --mode auto` 实现 **SQLite ↔ JSON 双向同步**，最近写入获胜 |

---

## 🧩 Core Capabilities

### 📁 `arcli project` — 项目脚手架
- `project init [PATH]` — 生成标准研究仓库目录树（`data/`, `experiments/`, `src/`, `.research/` 等）
- 自动初始化 git 仓库、生成 `.gitignore`、创建 SQLite 数据库 schema
- 支持 `--exp-dir` 自定义实验目录名，`--force` 覆盖非空目录

### 🔍 `arcli env` — 工作区感知
- `env status` — 输出仓库根路径、git 分支/提交/清洁度、活跃实验列表、数据资产列表、配置快照
- `env check` — 基本就绪性检查
- `env check --strict` — 严格模式：验证 git workspace clean **且** `.research/hooks/` 就绪

### 🗃️ `arcli data` — 数据资产管理
- `data register PATH --name <NAME>` — 注册数据资产，递归计算 SHA256 校验和
- 以 **YAML 索引文件**（`.research/data_index.yaml`）为事实来源，SQLite 为缓存
- `data list`, `data info <NAME>`, `data update <NAME> --recompute-checksum`

### 🧪 `arcli exp` — 实验生命周期
- `exp new --data <ASSET> --cmd "python train.py"` — 创建实验，生成唯一 ID `run-<short_id>-<YYYYMMDD-HHmm>`
- `exp run <EXP_ID>` — 执行命令，捕获 stdout/stderr 到 `logs/run.log`，自动状态转换
- `exp stop <EXP_ID> --signal SIGTERM` — 发送信号终止实验，状态收敛至 `interrupted`
- `exp metric <EXP_ID> --step 1 --json '{"loss":0.5}'` — 记录指标，支持覆盖更新
- `exp param <EXP_ID> --params '{"lr":0.001}'` — 合并更新超参数
- `exp finish <EXP_ID> --status finished --message "OOM"` — 手动标记完成/失败
- `exp status [EXP_ID]`, `exp list [--status <S>] [--since <DATE>]` — 查询实验状态

### 🔄 `arcli db` — 双轨同步
- `db sync --mode export` — SQLite → JSON，覆盖所有 `experiment.json`
- `db sync --mode import` — JSON → SQLite，扫描实验目录导入
- `db sync --mode auto` — 按 `updated_at` 时间戳逐实验比较，**最近写入获胜**
- `db status` — 显示需导出 / 需导入 / 已同步 / 冲突 / JSON-only 实验摘要
- `db export-all`, `db import --from <PATH>` — 批量导出/外部导入

### 📜 `arcli log` — 日志流式访问
- `log show <EXP_ID>` — 显示完整日志
- `log show <EXP_ID> --tail 20` — 最后 N 行
- `log show <EXP_ID> --follow` — 持续输出新写入内容（类似 `tail -f`）

### ⚙️ `arcli config` — 配置管理
- `config get <KEY>` — 读取配置值（支持点号表示法，如 `experiments.dir`）
- `config set <KEY> <VALUE>` — 更新并持久化到 `.research/config.yaml`

---

## 🏗️ Architecture

### 仓库目录结构

```text
<repo_root>/
├── data/
│   ├── raw/                 # 原始数据，Agent 只读
│   └── processed/           # 处理后数据
├── experiments/             # 实验容器（目录名可配置）
│   └── run-001-20260428-1030_baseline/
│       ├── experiment.json  # 完整实验记录（纳入版本管理）
│       ├── artifacts/       # 模型、图表等产出
│       └── logs/
│           └── run.log      # stdout/stderr 捕获
├── src/                     # 源代码
├── tests/                   # 测试代码
├── docs/                    # 文档
└── .research/               # arcli 内部数据
    ├── config.yaml          # 项目配置
    ├── research.db          # SQLite 运行时缓存（.gitignore 忽略）
    ├── data_index.yaml      # 数据资产事实来源
    └── hooks/               # 可选 hooks / readiness 检查
```

### SQLite ↔ JSON 同步流程

```text
┌─────────────┐     export      ┌─────────────────┐
│  SQLite     │ ───────────────→│ experiment.json │
│ research.db │                 │ (版本控制事实源) │
│  (运行时缓存) │ ←───────────────│                 │
└─────────────┘     import      └─────────────────┘
        ↑                              ↑
        │    db sync --mode auto       │
        │    按 updated_at 比较合并     │
        └──────────────────────────────┘
```

### 实验状态机

```text
┌─────────┐    run     ┌─────────┐   exit(0)   ┌──────────┐
│ created │ ─────────→ │ running │ ──────────→ │ finished │
└────┬────┘            └────┬────┘   exit(≠0)  └──────────┘
     │    manual/resume     │    ──────────→   ┌────────┐
     └──────────────────────┘                  │ failed │
                      │  SIGTERM/SIGINT         └────────┘
                      └─────────────────────→  ┌─────────────┐
                                                 │ interrupted │
                                                 └──────┬──────┘
                                                        │ resume
                                                        └──────→ running
```

---

## ⚡ Quick Start

```bash
# 1. 安装
$ cargo install --path .

# 2. 初始化研究仓库
$ arcli project init ./my-project --name "My Project"
$ cd my-project

# 3. 注册数据资产
$ arcli data register ./data/raw --name imdb-v1 --desc "IMDB dataset"

# 4. 创建实验
$ arcli exp new --data imdb-v1 --cmd "python train.py --epochs 10"
# → 输出: 实验 ID: run-001-20260428-1030

# 5. 运行实验
$ arcli exp run run-001-20260428-1030

# 6. 查看状态（JSON 模式，适合 Agent 消费）
$ arcli exp status run-001-20260428-1030 --json

# 7. 记录指标
$ arcli exp metric run-001-20260428-1030 --step 1 --json '{"loss":0.5,"acc":0.92}'

# 8. 同步到 JSON（纳入版本控制）
$ arcli db sync --mode export
$ git add experiments/*/experiment.json
```

---

## 📁 Project Layout

```text
src/
├── main.rs           # CLI 入口，命令分发
├── cli.rs            # clap 派生宏定义（所有子命令参数）
├── lib.rs            # 库入口
├── repo.rs           # 仓库发现、路径解析
├── config.rs         # 配置读写（点号键支持）
├── db.rs             # SQLite 封装（schema、CRUD、WAL 模式）
├── error.rs          # 统一错误枚举 + error_code
├── output.rs         # JSON / 文本输出格式化
└── commands/
    ├── mod.rs
    ├── project.rs    # project init
    ├── env.rs        # env status / check
    ├── data.rs       # data register / list / info / update
    ├── exp.rs        # exp new / run / stop / status / metric / param / finish / export
    ├── db.rs         # db sync / export-all / import / status
    ├── log.rs        # log show（tail / follow）
    └── config.rs     # config get / set
```

---

## 🛠️ Tech Stack

| Layer | Stack |
|---|---|
| Language | Rust 2024 Edition |
| CLI Parser | clap (derive macro) |
| Database | SQLite via rusqlite (bundled, WAL mode) |
| Serialization | serde + serde_json + serde_yaml |
| Git Integration | git2 |
| Checksum | sha2 + walkdir (recursive dir hash) |
| Signal Handling | ctrlc + libc (SIGTERM forwarding) |
| Testing | tempfile + libtest (62 tests) |

---

## ✅ Development

```bash
# 运行全部单元测试
$ cargo test --lib

# 构建 release 二进制
$ cargo build --release

# 运行 clippy
$ cargo clippy
```

---

## 📄 License

MIT License — 详见 [LICENSE](LICENSE)。
