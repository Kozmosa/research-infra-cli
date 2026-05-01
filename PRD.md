# arcli — Agentic Research Command Line Infrastructure

**产品需求文档 (PRD)**

**版本**: 0.1  
**状态**: 草案  
**目标用户**: AI Agent（主）、人类研究员（辅）  
**垂直领域**: 机器学习/深度学习实验验证、数据分析数学建模

---

## 1. 概述

`arcli` 是一套用 Rust 编写的命令行工具，旨在为 AI agent 提供一个**预结构化、可复现、低熵的研究工作环境**。它定义了科学的仓库目录规范，管理实验的完整生命周期，并将所有关键环境信息（代码状态、数据资产、实验记录）转化为确定的、可查询的 ground truth，从而消除 agent 的“即兴探索”行为，减少 token 消耗与幻觉，并保证人类与 agent 的无缝协作。

`arcli` 是整个研究基础设施（`research-infra`）的核心组件，它与一个固定的项目结构、一个本地 SQLite 缓存数据库、以及导出为 JSON 的实验记录文件协同工作。上游的 MCP server 与 Skill 层将其封装为对 agent 友好的工具。

---

## 2. 问题陈述

当前的 LLM agent 在仅有目标任务描述的情况下，会自行创建目录结构、推断文件位置、执行 shell 命令来探测环境。这带来以下严重问题：

- **不可复现**: 每次产生的目录结构都不同，人类无法理解，实验难以重现。
- **高摩擦**: 大量 token 浪费在环境探索上，且 agent 可能感知错误信息或产生幻觉。
- **协作困难**: 人类无法可靠地跟踪 agent 的行为，难以干预或审计。
- **缺乏 grounding**: agent 不知道项目已经有什么数据、哪些实验已完成、当前代码处于哪个版本。

`arcli` 通过将环境信息“注入” agent 的初始上下文，并为所有敏感操作提供严格的 CLI 界面，直接回应以上问题。

---

## 3. 解决方案原则

1. **一切皆为文件**: SQLite 数据库作为运行时缓存，但其内容通过 `experiment.json` 物化到文件系统，纳入版本控制。人类和 agent 最终只需信任 JSON 文件。
2. **确定性接口**: 所有 ground truth 的获取（git 状态、数据列表、实验列表）均通过不可变的 CLI 命令实现，消除 agent 自行执行易出错的 shell 命令的需求。
3. **严格的实验契约**: 采用“实验申请表”模式，agent 必须显式提供实验所需数据、命令、环境等参数，CLI 负责验证工作区净空、生成唯一 ID、锁定代码版本。
4. **无状态 CLI**: `arcli` 本身不维护常驻进程，每次调用即结束。所有状态存储在仓库内的文件（SQLite、JSON、YAML）中。
5. **分层解耦**: `arcli` 是底层原子操作集；MCP server / Skill 是上层封装，负责上下文注入、权限校验与流程编排。CLI 与 MCP 为独立的二进制文件。

---

## 4. 系统架构

### 4.1 仓库目录结构（由 `project init` 生成）

```
<repo_root>/
├── README.md
├── .gitignore
├── data/
│   ├── raw/                 # 原始数据，对 agent 只读
│   └── processed/           # 处理后数据
├── experiments/             # 所有实验的容器（目录名可在配置文件定制）
│   └── <exp_id>/            # 单个实验根目录
│       ├── experiment.json  # 完整实验记录（纳入版本管理）
│       ├── config.yaml      # 该实验的运行时配置（超参等）
│       ├── notes.md         # agent 或人类撰写的笔记
│       ├── artifacts/       # 模型、图表等产出
│       └── logs/            # 标准输出/错误日志
├── src/                     # 源代码
├── tests/                   # 测试代码
├── artifacts/               # 项目级共享制品
├── docs/                    # 文档
└── .research/               # arcli 内部数据（部分纳入版本管理）
    ├── config.yaml          # 项目配置
    ├── research.db          # SQLite 运行时缓存（.gitignore 忽略）
    ├── templates/           # 实验模板
    └── hooks/               # 可选 Git hooks
```

### 4.2 数据流：SQLite ↔ JSON 同步

- **SQLite (`research.db`)**: 仅作为执行态缓存，存储实验元数据、指标序列等，支持高效查询。该文件不纳入版本管理。
- **JSON (`experiment.json`)**: 每个实验目录下均有一份，是版本管理中的事实来源。它由 SQLite 在关键节点（实验创建、结束、手动导出）自动导出。
- **同步时机**:
  - **Pre-commit**: 将所有更新的实验从 SQLite 导出覆盖 JSON（`db sync --mode export` 或自动）。
  - **Post-merge/checkout**: 将从上游拉取的较新 JSON 导入 SQLite（`db sync --mode import`）。
  - **双向策略**: 以“最后写入时间”为准实现合并。若同一实验在 JSON 和 SQLite 中均有更新且冲突（理论上不应发生），命令报错并提示手动干预。

### 4.3 实验状态机

```
created ──→ running ──→ finished
            │   │
            │   └───→ failed
            │
            └───────→ interrupted
                        │
                        └──→ running (恢复运行)
```

- `created`: 实验元数据已生成，但尚未执行。
- `running`: 包装进程正在执行。
- `finished`: 进程退出码为 0，由 CLI 自动设置。
- `failed`: 进程退出码非 0，自动设置。
- `interrupted`: 进程收到终止信号，或异常断开。
- 只有 `created` 和 `interrupted` 可转换为 `running`。

---

## 5. CLI 详细规约 (arcli)

### 5.1 全局约定

- **仓库发现**: 默认从当前目录向上查找 `.research` 目录作为仓库根。可通过 `--repo <PATH>` 或环境变量 `RESEARCH_REPO` 显式指定。
- **输出格式**: 所有命令默认输出人类可读文本（表格、列表）。加上 `--json` 标志则输出稳定的 JSON 到 stdout，便于 agent 解析。
- **错误处理**: 任何错误都会设置非零退出码。在 `--json` 模式下，输出包含 `error_code` (string) 和 `message` (string) 的 JSON 对象。常见错误码如 `WORKSPACE_NOT_CLEAN`、`DATA_NOT_FOUND`、`EXP_ID_EXISTS` 等。
- **幂等性**: 创建类命令（如 `exp new`）若检测到冲突（重复 ID）则报错；修改类命令（如 `exp metric`）对同 step 同指标默认允许覆盖（更新）。

### 5.2 命令总览

```
arcli project init [PATH] [options]         # 创建项目脚手架
arcli env status [options]                  # 获取环境快照
arcli env check [options]                   # 检查工作区是否就绪
arcli data register <PATH> [options]        # 注册数据资产
arcli data list [options]                   # 列出数据资产
arcli data info <NAME> [options]            # 查看数据详情
arcli data update <NAME> [options]          # 更新数据信息
arcli exp new [options]                     # 提交实验申请
arcli exp run <EXP_ID> [--] [ARGS...]       # 包装执行实验
arcli exp stop <EXP_ID>                     # 终止实验
arcli exp status [EXP_ID] [options]         # 查看实验状态
arcli exp metric <EXP_ID> [options]         # 记录指标
arcli exp param <EXP_ID> [options]          # 记录/更新参数
arcli exp finish <EXP_ID> [options]         # 手动标记实验结束
arcli exp export <EXP_ID> [options]         # 导出实验 JSON
arcli exp list [options]                    # 列出实验摘要
arcli db sync [options]                     # SQLite 与 JSON 同步
arcli db export-all [options]              # 全量导出 JSON
arcli db import [options]                   # 从 JSON 导入
arcli db status [options]                   # 显示数据库与 JSON 差异
arcli log show <EXP_ID> [options]           # 查看实验日志
arcli config get <KEY>                      # 读取配置项
arcli config set <KEY> <VALUE>             # 设置配置项
```

### 5.3 `project` 命令组

#### `project init`

初始化一个新的研究仓库，创建完整目录结构和 Git 仓库。

**用法**: `arcli project init [PATH] [--name <NAME>] [--force] [--exp-dir <DIRNAME>]`

- `PATH`: 目标目录，默认当前目录。
- `--name`: 项目名称，写入 `README.md` 和配置。
- `--force`: 允许在非空目录下初始化（不会覆盖已有文件）。
- `--exp-dir`: 实验目录的名称，默认 `experiments`。

**行为**:
1. 检查目标目录是否为空，若非空且无 `--force` 则报错退出。
2. 创建目录树：`data/raw`, `data/processed`, `<exp-dir>`, `src`, `tests`, `artifacts`, `docs`, `.research`。
3. 在目标目录执行 `git init`。
4. 生成 `.gitignore`，默认忽略 `.research/*.db*`、`__pycache__` 等，并注释建议使用 DVC 管理大数据。
5. 在 `.research/` 下创建 `config.yaml`，含默认配置。
6. 执行 SQLite 数据库初始化，创建 `seq`、`experiments`、`metrics_history` 等表，文件写入 `.research/research.db`。
7. 输出创建的文件列表（`--json` 时返回路径数组）。

**示例**:
```bash
arcli project init ./my-ml-project --name "Image Classification" --exp-dir exps
```

---

### 5.4 `env` 命令组

#### `env status`

获取当前仓库的完整环境快照，作为 agent 的初始上下文。

**用法**: `arcli env status [--json]`

**JSON 输出结构**:
```json
{
  "repo_root": "/abs/path",
  "git": {
    "branch": "main",
    "commit_hash": "abc123...",
    "is_clean": true,
    "last_commit_time": "2026-04-28T12:00:00Z"
  },
  "active_experiments": [
    {"id": "run-002-2026-04-28-1432", "status": "running", "start_time": "..."}
  ],
  "data_assets": ["imdb-2026-04-28", "cleaned-v2"],
  "config": { ... }
}
```

#### `env check`

用于执行策略前的检查，如“工作区必须净空”。

**用法**: `arcli env check [--strict]`

- `--strict`: 检查工作区是否干净（`git status --porcelain` 为空）、必要的钩子是否就绪等。
- 若检查不通过，退出码非零，并返回具体原因。

---

### 5.5 `data` 命令组

#### `data register`

将目录注册为正式的数据资产，并记录校验和与添加日期。

**用法**: `arcli data register <PATH> --name <NAME> [--desc <TEXT>] [--checksum <SHA256>]`

- `PATH`: 数据目录的路径（相对于仓库根）。
- `--name`: 该资产的唯一标识符（如 `imdb-v1`）。
- `--desc`: 人类可读描述。
- `--checksum`: 若未提供，CLI 自动对目录下所有文件递归计算 SHA256（或可配置算法）。
- 注册信息存入 `.research/data_index.yaml` 或 SQLite `datasets` 表。

#### `data list`

列出所有已注册数据资产。

**用法**: `arcli data list [--json]`

输出名称列表，agent 可直接拿去填写 `--data` 参数。

#### `data info`

查看数据资产的详细信息。

**用法**: `arcli data info <NAME> [--json]`

返回 JSON 包含名称、路径、添加日期、校验和、描述等。

#### `data update`

更新数据的位置或手动触发重新计算校验和。

**用法**: `arcli data update <NAME> [--path <NEW_PATH>] [--recompute-checksum]`

---

### 5.6 `exp` 命令组（实验生命周期）

#### `exp new`

提交一份“实验申请表”，创建实验记录和目录，但**不立即执行**。

**用法**: 
```
arcli exp new --data <DATA_NAME> --cmd <COMMAND> 
             [--label <SHORT_LABEL>] [--params <JSON>] 
             [--notes <TEXT>] [--env <ENV_NAME>] [--template <TEMPLATE>]
             [--json]
```

- `--data`: **必填**，已注册的数据资产名称。
- `--cmd`: **必填**，要执行的 shell 命令（如 `python train.py --lr 0.01`）。
- `--label`: 可选短标签，将追加在实验 ID 尾部，用下划线连接。
- `--params`: 初始超参 JSON，如 `'{"batch_size": 32}'`。
- `--notes`: 实验目的或备忘。
- `--env`: 环境名（conda 环境或 Docker 镜像），`exp run` 时将激活。
- `--template`: 从 `.research/templates/<template>` 复制文件到实验目录。

**行为**:
1. 运行 `env check --strict`（若检查失败则拒绝创建）。
2. 验证 `--data` 在已注册列表中，否则报错。
3. 在 SQLite 的原子序列中获取下一个 `short_id`（格式：`RUN-<自增数字>`）。
4. 生成实验 ID：`run-<short_id>-<YYYY-MM-DD-HHmm>`，若提供 `--label` 则追加 `_<label>`。
5. 在 `<exp-dir>/<ID>/` 下创建目录，并生成初始 `experiment.json`：
   - id, created_at, commit_hash, data_used, command, params, notes, env, status="created"
   - 开始/结束时间为 null
6. 将记录插入 SQLite，并同步导出到 `experiment.json`。
7. 输出新实验的 ID 和目录路径。

**错误码示例**:
- `WORKSPACE_NOT_CLEAN`
- `DATA_NOT_FOUND`
- `MISSING_REQUIRED_ARG`

#### `exp run`

使用包装器模式执行实验。

**用法**: `arcli exp run <EXP_ID> [--] [EXTRA_ARGS...]`

- `EXTRA_ARGS`: 将追加到 `exp new` 中保存的命令后面。

**行为**:
1. 加载实验记录，验证状态必须为 `created` 或 `interrupted`。
2. 若指定了 `--env`，则激活相应环境（例如 `conda activate <env>` 或 `docker run`）。
3. 记录实际的开始时间，设置状态为 `running`，更新数据库和 JSON。
4. Fork/exec 命令，同时捕获 stdout 和 stderr，写入 `logs/run.log`。
5. 等待进程结束，获取退出码。
6. 记录结束时间：
   - 退出码 0 → 状态 `finished`
   - 退出码 非0 → 状态 `failed`
7. 如果 arcli 自身被信号中断（SIGINT/SIGTERM），将信号转发给子进程，并将状态置为 `interrupted`。子进程异常终止也会导致 `interrupted`。
8. 更新数据库和 JSON，输出最后几行日志和退出码。

**注意**: `exp run` 会阻塞直到进程结束，MCP 层可将其作为长时间工具调用处理。

#### `exp stop`

终止运行中的实验。

**用法**: `arcli exp stop <EXP_ID> [--signal <SIGNAL>]`

- 默认发送 SIGTERM，可指定 SIGKILL。要求状态为 `running`。

#### `exp status`

查看单个或所有实验的状态。

**用法**: `arcli exp status [EXP_ID] [--json]`

若省略 `EXP_ID`，返回所有实验的摘要（与 `exp list` 类似但包含更详细的当前状态）。

#### `exp metric`

记录结构化指标（如 loss、accuracy）。

**用法**:
- `arcli exp metric <EXP_ID> --step <N> --json '{"loss": 0.5}'`
- `arcli exp metric <EXP_ID> --step <N> --key loss --val 0.5 --key acc --val 0.9`

指标追加到 SQLite `metrics_history` 表，并异步影响下一次 JSON 导出。允许对同 step 同指标覆盖（更新）。

#### `exp param`

在实验运行中或结束后补录参数。与 `exp new` 时的 `--params` 合并，有同名键则覆盖。

**用法**: `arcli exp param <EXP_ID> --json '{"lr": 0.001}'`

#### `exp finish`

用于未通过 `exp run` 管理的手动实验（人类直接运行），或对 `interrupted` 实验主动标记为失败。

**用法**: `arcli exp finish <EXP_ID> --status <finished|failed> [--message <TEXT>]`

#### `exp export`

强制将 SQLite 中的实验数据导出到 `<exp-dir>/<ID>/experiment.json`。通常由同步钩子调用。

**用法**: `arcli exp export <EXP_ID> [--output <PATH>]`

#### `exp list`

列出所有实验的摘要信息，支持过滤。

**用法**: `arcli exp list [--status <STATUS>] [--since <DATE>] [--json]`

---

### 5.7 `db` 命令组

#### `db sync`

执行 SQLite 与 JSON 的双向同步。

**用法**: `arcli db sync [--mode export|import|auto]`

- `export`: 将所有 SQLite 记录导出并覆盖对应实验的 JSON 文件。
- `import`: 扫描所有 `experiment.json`，将更新的内容写回 SQLite。
- `auto`: 比较每个实验的修改时间，最后写入者获胜。若同一实验两者均被修改且内容实际冲突（SHA 不一致），命令报错并列出冲突实验。

`auto` 是默认模式，通常用于 pre-commit 钩子 (`db sync --mode auto`) 和 post-merge 钩子 (`db sync --mode import`)。

#### `db export-all`

将所有实验从 SQLite 导出到一系列 JSON 文件，用于团队共享或备份。

**用法**: `arcli db export-all [--out-dir <DIR>]`

#### `db import`

从指定的 `experiment.json` 或一个包含多个 JSON 的目录导入到 SQLite。仅对不存在或更新的实验执行插入/更新。

**用法**: `arcli db import --from <PATH>`

#### `db status`

显示数据库与实验目录 JSON 之间的同步状态（哪些实验需要更新，哪个方向）。

**用法**: `arcli db status [--json]`

---

### 5.8 `log` 命令组

#### `log show`

显示实验的运行日志。

**用法**: `arcli log show <EXP_ID> [--tail N] [--follow]`

- `--tail N`: 显示最后 N 行。
- `--follow`: 持续输出新内容（类似 `tail -f`），用于 streaming。

---

### 5.9 `config` 命令组

#### `config get`

读取 `.research/config.yaml` 中的某个值。

**用法**: `arcli config get <KEY>`

#### `config set`

设置 `.research/config.yaml` 中的某个值。

**用法**: `arcli config set <KEY> <VALUE>`

支持嵌套键，如 `templates.dir`。

---

## 6. 并发与数据完整性

- **short_id 自增序列**: 在 SQLite 中维护 `seq` 表。分配 ID 时，使用原子操作：`UPDATE seq SET id = id + 1 WHERE name = 'experiment' RETURNING id`。若无行则插入。这保证了即使多个进程同时创建实验，也绝不会产生相同 `short_id`。
- **SQLite 并发**: 数据库以 WAL 模式打开，允许多读并发。写操作（插入指标、更新状态）串行化，但对于典型的实验日志写入频率（秒级）完全足够。若未来有高频写入需求，可通过内存缓冲批量写入。
- **实验 ID 唯一性**: 由于 `short_id` 全局唯一且时间戳以分钟为单位，完全避免了 ID 冲突。CLI 绝不信任 agent 传递的任何 ID 前缀，始终由系统生成。
- **工作区净空保证**: `exp new` 强制依赖 `env check --strict`，防止在脏工作区上创建实验。这不需要文件锁，因为 git status 本身是即时快照。若极端并发下两个进程同时检查并通过，然后一个进程 commit 后另一个进程才创建实验，则后者的 commit hash 将是最新的。这是可接受的：实验记录将绑定它被创建时的 commit，不会是脏的。
- **进程信号与状态**: `exp run` 维持对子进程的引用，确保在 arcli 被终止时能更新实验状态，避免僵尸实验。

---

## 7. 扩展性：超参调优实验 (Study)

作为第一版后扩展，实验可增加 `type` 字段。`study` 类型的实验允许内嵌多个 trial。其目录结构可设计为：

```
experiments/run-010-..._hparam-search/
├── experiment.json        # 记录搜索空间、算法、最优结果
├── trials/
│   ├── trial-001/
│   │   └── experiment.json
│   └── trial-002/
...
```

`exp new` 将增加 `--type study --study-config <json>`，`exp run` 可批量执行或由外部调优框架循环调用 `exp new` + `exp run`（将每个 trial 作为子实验）。当前 PRD 不展开此部分。

---

## 8. 未来路线图

- **模板系统**: 允许用户定义实验目录骨架，复制标准脚本和配置文件。
- **远程同步**: `remote` 命令组，与中央实验数据库或 DVC 远程存储交互。
- **查询 API**: 提供内置的指标查询语言（如 SQL 查询），支持跨实验对比。
- **插件系统**: 允许用户扩展自定义指标记录器或环境激活器。

---

## 9. 成功标准

- Agent 进入一个空仓库后，通过 `project init` 一键获得完整环境，无需探索。
- 所有实验均可通过 `experiment.json` 完全复现，人类只需检出对应 commit 并读取该文件。
- Agent 的 token 消耗中，环境探测部分降为零；所有上下文通过初次 `env status` 注入。
- 实验记录与代码版本严格绑定，绝不会出现“忘记 commit 或环境”的错误。

---

**文档维护**: 本 PRD 将随 `arcli` 的实现和反馈持续迭代。下一步：根据此 PRD 编写 Rust 代码结构设计，并定义 MCP 工具映射 schema。
