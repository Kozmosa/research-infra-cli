# rcli 实现计划

## 目标描述

实现 `rcli`，一个用 Rust 编写的命令行工具，为 AI agent 和人类研究员提供一个预结构化、可复现、低熵的研究工作环境。它管理项目脚手架、数据资产注册、实验生命周期（创建、执行、监控、完成）、SQLite/JSON 同步、环境状态报告和配置管理 —— 所有功能均通过确定性的 CLI 命令实现，支持人类可读的文本输出和 JSON 机器可解析输出。

## 验收标准

遵循 TDD 哲学，每个标准都包含正向测试和负向测试以实现确定性验证。

- AC-1: 项目初始化创建完整的研究仓库脚手架
  - 正向测试（预期通过）：
    - `rcli project init ./test-project` 创建完整目录树（`data/raw`、`data/processed`、`experiments`、`src`、`tests`、`artifacts`、`docs`、`.research`），初始化 git 仓库，生成 `.gitignore` 和 `.research/config.yaml`，并创建包含所需表的 SQLite 数据库
    - `rcli project init ./existing --force` 在非空目录中成功执行，且不覆盖已有文件
    - `rcli project init ./project --exp-dir exps` 创建 `exps/` 作为实验目录，而非默认的 `experiments/`
    - `rcli project init ./project --name "My Project"` 将项目名称写入 `README.md` 和 `config.yaml`
  - 负向测试（预期失败）：
    - `rcli project init ./nonempty` 不带 `--force` 时以非零状态退出，并返回适当的错误消息
    - `rcli project init ./project` 在已包含 `.research/config.yaml` 的目录中执行时，以错误退出，指示该目录已是研究项目

- AC-2: 环境状态与检查命令提供准确的工作区信息
  - 正向测试（预期通过）：
    - `rcli env status` 输出仓库根路径、当前 git 分支、提交哈希、工作区清洁度、最后提交时间、活跃实验列表、已注册数据资产列表和当前配置
    - `rcli env status --json` 输出包含所有字段的有效 JSON 对象，与文档化 schema 匹配
    - `rcli env check` 在工作区处于基本就绪状态时返回退出码 0
    - `rcli env check --strict` 在 `git status --porcelain` 为空且必要钩子已就位时返回退出码 0
  - 负向测试（预期失败）：
    - `rcli env check --strict` 在存在未提交更改的工作区中返回非零退出码，错误码为 `WORKSPACE_NOT_CLEAN`，并附带描述性消息
    - `rcli env status` 在研究仓库外部执行时返回非零退出码，指示未找到仓库

- AC-3: 数据资产管理支持注册、列出、查看和更新，以 YAML/JSON 文件作为事实来源
  - 正向测试（预期通过）：
    - `rcli data register ./data/raw --name imdb-v1` 注册数据资产，递归计算目录下所有文件的 SHA256 校验和，并更新 `.research/data_index.yaml`（或等效 JSON 文件），记录名称、路径、校验和、描述和注册日期
    - `rcli data list` 以人类可读格式显示所有已注册数据资产名称
    - `rcli data list --json` 输出包含所有已注册资产的有效 JSON 数组
    - `rcli data info imdb-v1` 显示详细信息，包括名称、相对路径、校验和、描述和注册日期
    - `rcli data info imdb-v1 --json` 输出包含资产详情有效 JSON 对象
    - `rcli data update imdb-v1 --recompute-checksum` 重新计算并更新数据索引文件中的校验和
    - `rcli data update imdb-v1 --path ./data/new-location` 更新资产的存储路径
  - 负向测试（预期失败）：
    - `rcli data register ./missing-dir --name x` 因路径不存在而失败，错误码为 `DATA_NOT_FOUND`
    - `rcli data info nonexistent` 因资产不存在而失败，错误码为 `DATA_NOT_FOUND`
    - `rcli data register ./data/raw --name imdb-v1` 在已存在同名资产时失败，返回适当的错误码

- AC-4: 实验创建强制执行实验契约，并生成唯一、确定性的记录
  - 正向测试（预期通过）：
    - `rcli exp new --data imdb-v1 --cmd "python train.py"` 在 `experiments/` 下创建实验目录，生成格式为 `run-<short_id>-<YYYY-MM-DD-HHmm>` 的唯一实验 ID，写入状态为 `created` 的 `experiment.json` 文件，并记录提供的数据资产、命令、当前提交哈希和创建时间戳
    - `rcli exp new --data imdb-v1 --cmd "python train.py" --label test` 生成带 `_test` 后缀的 ID
    - `rcli exp new --data imdb-v1 --cmd "python train.py" --params '{"lr":0.01}'` 在实验记录中存储初始参数
    - `rcli exp new --data imdb-v1 --cmd "python train.py" --notes "Baseline run"` 存储备注
    - `rcli exp new --data imdb-v1 --cmd "python train.py" --env conda-ml` 存储环境名称
    - `rcli exp new --manual --cmd "python train.py"`（或等效的手动创建机制）为手动执行的运行创建实验记录，无需 `--data`
    - 多个 `rcli exp new` 并发调用从独立进程生成唯一的 `short_id` 值，无重复
  - 负向测试（预期失败）：
    - `rcli exp new --cmd "python train.py"` 不带 `--data` 时失败，错误码为 `MISSING_REQUIRED_ARG`
    - `rcli exp new --data missing-asset --cmd "x"` 失败，错误码为 `DATA_NOT_FOUND`
    - `rcli exp new --data imdb-v1 --cmd "x"` 在脏工作区中（启用严格检查时）失败，错误码为 `WORKSPACE_NOT_CLEAN`
    - `rcli exp new --data imdb-v1 --cmd "x"` 在已存在相同 ID 的实验时失败，返回适当的错误码

- AC-5: 实验执行正确管理子进程生命周期和状态转换
  - 正向测试（预期通过）：
    - `rcli exp run <EXP_ID>` 对状态为 `created` 或 `interrupted` 的实验，将其状态转换为 `running`，执行存储的命令，捕获 stdout 和 stderr 到 `logs/run.log`，在退出码为 0 时将状态转换为 `finished` 并记录结束时间
    - `rcli exp run <EXP_ID>` 对以非零代码退出的命令，将状态转换为 `failed` 并记录结束时间
    - `rcli exp run <EXP_ID> -- --extra-arg` 在执行前将额外参数追加到存储的命令
    - `rcli exp run <EXP_ID>` 将 SIGINT/SIGTERM 转发给子进程，并在 CLI 自身收到这些信号时将实验状态设置为 `interrupted`
  - 负向测试（预期失败）：
    - `rcli exp run <EXP_ID>` 对已处于 `running` 状态的实验失败，返回适当的错误
    - `rcli exp run <EXP_ID>` 对已处于 `finished` 状态的实验失败，返回适当的错误
    - `rcli exp run <NONEXISTENT_ID>` 失败，返回适当的错误码

- AC-6: 实验监控与控制命令管理运行中的实验
  - 正向测试（预期通过）：
    - `rcli exp stop <EXP_ID>` 向运行中实验的进程发送 SIGTERM，并将状态转换为 `interrupted`
    - `rcli exp stop <EXP_ID> --signal SIGKILL` 发送 SIGKILL 而非 SIGTERM
    - `rcli exp status <EXP_ID>` 显示特定实验的当前状态、开始时间、结束时间、退出码和其他元数据
    - `rcli exp status` 不带 ID 时显示所有实验的状态
    - `rcli exp status --json` 输出有效的 JSON
    - `rcli exp list` 显示所有实验的摘要
    - `rcli exp list --status running` 过滤并仅显示状态为 `running` 的实验
    - `rcli exp list --since 2026-04-01` 过滤并仅显示在指定日期或之后创建的实验
  - 负向测试（预期失败）：
    - `rcli exp stop <EXP_ID>` 对不处于 `running` 状态的实验失败，返回适当的错误
    - `rcli exp status <NONEXISTENT_ID>` 失败，返回适当的错误码

- AC-7: 实验数据管理支持指标记录、参数更新和手动完成
  - 正向测试（预期通过）：
    - `rcli exp metric <EXP_ID> --step 1 --json '{"loss":0.5}'` 将 step 1 的指标记录到 SQLite 数据库
    - `rcli exp metric <EXP_ID> --step 1 --key loss --val 0.5 --key acc --val 0.9` 通过键值对记录多个指标
    - 以相同的实验、step 和指标键调用 `rcli exp metric` 时，覆盖之前的值
    - `rcli exp param <EXP_ID> --json '{"lr":0.001}'` 在实验记录中添加或更新参数，与现有参数合并，键冲突时覆盖
    - `rcli exp finish <EXP_ID> --status finished` 手动将实验状态设置为 `finished` 并记录结束时间
    - `rcli exp finish <EXP_ID> --status failed --message "Out of memory"` 将状态设置为 `failed` 并记录提供的消息
  - 负向测试（预期失败）：
    - `rcli exp metric <NONEXISTENT_ID> --step 1 --json '{"loss":0.5}'` 失败，返回适当的错误
    - `rcli exp finish <EXP_ID> --status invalid` 失败，因为状态必须是状态机中的有效值之一

- AC-8: 数据库同步维护 SQLite 缓存与 JSON 文件之间的一致性
  - 正向测试（预期通过）：
    - `rcli db sync --mode export` 将所有实验记录从 SQLite 导出到对应的 `experiment.json` 文件，覆盖现有 JSON 文件
    - `rcli db sync --mode import` 扫描实验目录中的所有 `experiment.json` 文件，将其内容导入 SQLite
    - `rcli db sync --mode auto` 比较 SQLite 和 JSON 文件中每个实验的修改时间戳，按实验逐个导出或导入，使最近修改的版本获胜
    - `rcli db sync --mode auto` 在每个实验仅在一侧被修改时无错误成功执行
    - `rcli db export-all` 将所有实验从 SQLite 导出到 JSON 文件
    - `rcli db import --from /external/path` 从外部目录导入实验 JSON 文件到 SQLite
    - `rcli db status` 以人类可读格式显示 SQLite 与 JSON 文件之间的差异摘要
    - `rcli db status --json` 输出有效的 JSON，描述哪些实验需要导出、哪些需要导入、哪些存在冲突
  - 负向测试（预期失败）：
    - `rcli db sync --mode auto` 在实验同时在 SQLite 和其 JSON 文件中被修改且内容不一致时（通过内容哈希不匹配检测），以非零退出码失败并列出冲突的实验 ID

- AC-9: 日志和配置命令提供对实验日志和项目设置的读取访问
  - 正向测试（预期通过）：
    - `rcli log show <EXP_ID>` 显示实验 `logs/run.log` 文件的完整内容
    - `rcli log show <EXP_ID> --tail 10` 仅显示日志的最后 10 行
    - `rcli log show <EXP_ID> --follow` 持续输出新写入的日志内容（用于流式传输）
    - `rcli config get experiments.dir` 返回配置键的当前值
    - `rcli config get templates.dir` 支持使用点号表示法的嵌套键
    - `rcli config set experiments.dir exps` 更新配置文件并持久化更改
  - 负向测试（预期失败）：
    - `rcli log show <EXP_ID>` 在日志文件不存在时以信息性消息优雅失败
    - `rcli config get nonexistent.key` 在键未找到时失败，返回适当的错误

- AC-10: 所有命令的错误处理和输出格式保持一致
  - 正向测试（预期通过）：
    - 每个命令在成功执行时返回退出码 0
    - 每个命令支持 `--json` 标志，在成功时生成有效的、可解析的 JSON 输出
    - 发生错误且指定 `--json` 时，输出为包含 `error_code` 字符串字段和 `message` 字符串字段的 JSON 对象
    - 常见错误码（`WORKSPACE_NOT_CLEAN`、`DATA_NOT_FOUND`、`EXP_ID_EXISTS`、`MISSING_REQUIRED_ARG`）在相关命令中一致使用
  - 负向测试（预期失败）：
    - 任何遇到错误的命令返回非零退出码
    - 无效命令行参数触发非零退出码并附带描述性错误

- AC-11: 在并行访问下保持并发性和数据完整性保证
  - 正向测试（预期通过）：
    - SQLite 数据库以 WAL（预写日志）模式打开，以支持写入期间的并发读取
    - SQLite 中的 `short_id` 序列使用 `UPDATE ... RETURNING` 或等效的原子操作原子递增，确保 100 个并发的 `rcli exp new` 调用产生 100 个唯一的 short ID，无冲突
    - 实验 ID 结合了唯一的 short ID 和时间戳，保证全局唯一性
  - 负向测试（预期失败）：
    - 本标准无直接负向测试；违反将表现为 AC-4 并发测试中的失败

## 路径边界

路径边界定义了可接受实现质量的范围和选择。

### 上界（最大可接受范围）

实现包含所有命令组（`project`、`env`、`data`、`exp`、`db`、`log`、`config`），具备全面的错误处理、每个命令的 JSON 输出模式、SQLite WAL 模式并发访问、通过数据库序列的原子 ID 生成、优雅实验中断的信号处理、日志 tail 和 follow 支持、数据资产的递归 SHA256 校验和计算、配置的嵌套键支持、手动实验创建能力，以及带冲突检测的双向 SQLite/JSON 同步。

### 下界（最小可接受范围）

实现包含核心 `project init` 脚手架、基本的 `env status` 报告、数据资产注册和列出、实验创建（`exp new`）及原子 ID 生成、实验执行（`exp run`）及子进程管理和状态转换、包含 experiments 和 metrics 表的 SQLite schema，以及实验记录的 JSON 导出。

### 允许的选择

- 可以使用：Rust 标准库、`clap`（CLI 参数解析）、`rusqlite`（SQLite 访问）、`serde`（序列化/反序列化）、`tokio` 或 `std::process`（子进程管理）、`chrono`（时间戳处理）、`sha2`（校验和计算）、`rayon`（并行校验和计算）、`anyhow` 或 `thiserror`（错误处理）
- 不可使用：外部网络服务、云 API 或任何需要网络访问的操作；GUI 框架；嵌入式脚本语言
- 固定选择：Rust 是 PRD 规范要求的实现语言
- 数据资产事实来源：版本控制的 YAML 或 JSON 文件（例如 `.research/data_index.yaml`）是数据资产的事实来源，SQLite 作为运行时缓存
- 手动实验创建：实现必须支持为手动执行的运行创建实验记录（例如通过 `exp new --manual` 或 `exp finish --create`）

## 可行性提示与建议

> **注意**：本节仅供参考和理解。这些是概念性建议，非规定性要求。

### 概念性实现路径

围绕分层架构构建 `rcli`：

1. **CLI 层**（`clap`）：定义所有命令结构、参数解析和帮助文本。每个子命令映射到一个处理函数。
2. **命令处理层**：接收解析后的参数和一个 `Repository` 上下文对象。验证输入，调用适当的领域操作，并格式化输出（人类可读的表格/列表或 JSON）。
3. **仓库层**：封装所有文件系统和 git 操作。通过向上遍历目录树查找 `.research/config.yaml` 来发现仓库根目录。提供读取/写入 `experiment.json`、`config.yaml` 和数据索引文件的方法。
4. **持久化层**：管理 SQLite 数据库（`rusqlite`）。提供 ID 生成、实验 CRUD、指标插入和批量导出/导入的原子操作。使用 WAL 模式。

对于 `exp run` 中的子进程管理，使用带有管道 stdout/stderr 的 `std::process::Command`。生成一个线程将输出同时流式传输到终端和日志文件。安装信号处理器（通过 `ctrlc` crate 或 `signal-hook`）以将 SIGINT/SIGTERM 转发给子进程，并将实验状态更新为 `interrupted`。

对于同步，在 SQLite 和 JSON 中维护 `modified_at` 时间戳。在 `auto` 模式下，按实验比较时间戳；如果只有一侧较新，则按该方向复制。如果两侧都较新，比较内容哈希 —— 如果不同，报告冲突。

### 相关参考

这是一个绿地项目，没有现有代码库。所有组件都将全新编写。

## 依赖关系与顺序

### 里程碑

1. **里程碑 1: 项目基础**：建立 CLI 框架、仓库发现机制、配置管理和项目初始化
   - 阶段 A: 搭建 Rust 项目结构、使用 `clap` 进行 CLI 解析、仓库根目录发现、配置文件（`config.yaml`）的读取/写入
   - 阶段 B: 实现 `project init` 命令，包括目录树创建、git 初始化、`.gitignore` 生成，以及 SQLite 数据库初始化（创建 `seq`、`experiments` 和 `metrics_history` 表）

2. **里程碑 2: 环境与数据管理**：实现查询工作区状态和管理数据资产的命令
   - 阶段 A: 实现 `env status`（git 状态、活跃实验、数据资产、配置）和 `env check`（工作区就绪性验证）
   - 阶段 B: 实现 `data register`、`data list`、`data info` 和 `data update` 命令，以 YAML/JSON 文件作为数据索引的事实来源

3. **里程碑 3: 实验核心**：实现实验创建和执行
   - 阶段 A: 实现 `exp new` 命令，包括通过 SQLite 的原子 short ID 生成、工作区验证、数据资产验证、实验目录创建和 `experiment.json` 生成；包含手动实验创建支持
   - 阶段 B: 实现 `exp run` 命令，包括状态转换验证、子进程生成、stdout/stderr 捕获到 `logs/run.log`、退出码处理和信号转发

4. **里程碑 4: 实验生命周期与可观测性**：实现监控、数据记录和日志访问命令
   - 阶段 A: 实现 `exp stop`、`exp status`、`exp list`、`exp metric`、`exp param` 和 `exp finish` 命令
   - 阶段 B: 实现 `log show` 的 `--tail` 和 `--follow` 支持

5. **里程碑 5: 持久化与同步**：实现数据库同步、批量操作和配置命令
   - 阶段 A: 实现 `db sync` 的 `export`、`import` 和 `auto` 模式（包括冲突检测），以及 `db status`
   - 阶段 B: 实现 `db export-all`、`db import --from` 和 `config get/set` 命令

## 实现注意事项

### 代码风格要求
- 实现代码和注释中不得包含计划特定的术语，如 "AC-"、"Milestone"、"Phase"、"Step" 或类似的工作流标记
- 这些术语仅用于计划文档，不用于最终代码库
- 在代码中使用描述性的、与领域相关的命名

--- Original Design Draft Start ---

# rcli 产品需求文档 (PRD)

**版本**: 0.1  
**状态**: 草案  
**目标用户**: AI Agent（主）、人类研究员（辅）  
**垂直领域**: 机器学习/深度学习实验验证、数据分析数学建模

---

## 1. 概述

`rcli` 是一套用 Rust 编写的命令行工具，旨在为 AI agent 提供一个**预结构化、可复现、低熵的研究工作环境**。它定义了科学的仓库目录规范，管理实验的完整生命周期，并将所有关键环境信息（代码状态、数据资产、实验记录）转化为确定的、可查询的 ground truth，从而消除 agent 的“即兴探索”行为，减少 token 消耗与幻觉，并保证人类与 agent 的无缝协作。

`rcli` 是整个研究基础设施（`research-infra`）的核心组件，它与一个固定的项目结构、一个本地 SQLite 缓存数据库、以及导出为 JSON 的实验记录文件协同工作。上游的 MCP server 与 Skill 层将其封装为对 agent 友好的工具。

---

## 2. 问题陈述

当前的 LLM agent 在仅有目标任务描述的情况下，会自行创建目录结构、推断文件位置、执行 shell 命令来探测环境。这带来以下严重问题：

- **不可复现**: 每次产生的目录结构都不同，人类无法理解，实验难以重现。
- **高摩擦**: 大量 token 浪费在环境探索上，且 agent 可能感知错误信息或产生幻觉。
- **协作困难**: 人类无法可靠地跟踪 agent 的行为，难以干预或审计。
- **缺乏 grounding**: agent 不知道项目已经有什么数据、哪些实验已完成、当前代码处于哪个版本。

`rcli` 通过将环境信息“注入” agent 的初始上下文，并为所有敏感操作提供严格的 CLI 界面，直接回应以上问题。

---

## 3. 解决方案原则

1. **一切皆为文件**: SQLite 数据库作为运行时缓存，但其内容通过 `experiment.json` 物化到文件系统，纳入版本控制。人类和 agent 最终只需信任 JSON 文件。
2. **确定性接口**: 所有 ground truth 的获取（git 状态、数据列表、实验列表）均通过不可变的 CLI 命令实现，消除 agent 自行执行易出错的 shell 命令的需求。
3. **严格的实验契约**: 采用“实验申请表”模式，agent 必须显式提供实验所需数据、命令、环境等参数，CLI 负责验证工作区净空、生成唯一 ID、锁定代码版本。
4. **无状态 CLI**: `rcli` 本身不维护常驻进程，每次调用即结束。所有状态存储在仓库内的文件（SQLite、JSON、YAML）中。
5. **分层解耦**: `rcli` 是底层原子操作集；MCP server / Skill 是上层封装，负责上下文注入、权限校验与流程编排。CLI 与 MCP 为独立的二进制文件。

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
└── .research/               # rcli 内部数据（部分纳入版本管理）
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

## 5. CLI 详细规约 (rcli)

### 5.1 全局约定

- **仓库发现**: 默认从当前目录向上查找 `.research` 目录作为仓库根。可通过 `--repo <PATH>` 或环境变量 `RESEARCH_REPO` 显式指定。
- **输出格式**: 所有命令默认输出人类可读文本（表格、列表）。加上 `--json` 标志则输出稳定的 JSON 到 stdout，便于 agent 解析。
- **错误处理**: 任何错误都会设置非零退出码。在 `--json` 模式下，输出包含 `error_code` (string) 和 `message` (string) 的 JSON 对象。常见错误码如 `WORKSPACE_NOT_CLEAN`、`DATA_NOT_FOUND`、`EXP_ID_EXISTS` 等。
- **幂等性**: 创建类命令（如 `exp new`）若检测到冲突（重复 ID）则报错；修改类命令（如 `exp metric`）对同 step 同指标默认允许覆盖（更新）。

### 5.2 命令总览

```
rcli project init [PATH] [options]         # 创建项目脚手架
rcli env status [options]                  # 获取环境快照
rcli env check [options]                   # 检查工作区是否就绪
rcli data register <PATH> [options]        # 注册数据资产
rcli data list [options]                   # 列出数据资产
rcli data info <NAME> [options]            # 查看数据详情
rcli data update <NAME> [options]          # 更新数据信息
rcli exp new [options]                     # 提交实验申请
rcli exp run <EXP_ID> [--] [ARGS...]       # 包装执行实验
rcli exp stop <EXP_ID>                     # 终止实验
rcli exp status [EXP_ID] [options]         # 查看实验状态
rcli exp metric <EXP_ID> [options]         # 记录指标
rcli exp param <EXP_ID> [options]          # 记录/更新参数
rcli exp finish <EXP_ID> [options]         # 手动标记实验结束
rcli exp export <EXP_ID> [options]         # 导出实验 JSON
rcli exp list [options]                    # 列出实验摘要
rcli db sync [options]                     # SQLite 与 JSON 同步
rcli db export-all [options]              # 全量导出 JSON
rcli db import [options]                   # 从 JSON 导入
rcli db status [options]                   # 显示数据库与 JSON 差异
rcli log show <EXP_ID> [options]           # 查看实验日志
rcli config get <KEY>                      # 读取配置项
rcli config set <KEY> <VALUE>             # 设置配置项
```

### 5.3 `project` 命令组

#### `project init`

初始化一个新的研究仓库，创建完整目录结构和 Git 仓库。

**用法**: `rcli project init [PATH] [--name <NAME>] [--force] [--exp-dir <DIRNAME>]`

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
rcli project init ./my-ml-project --name "Image Classification" --exp-dir exps
```

---

### 5.4 `env` 命令组

#### `env status`

获取当前仓库的完整环境快照，作为 agent 的初始上下文。

**用法**: `rcli env status [--json]`

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

**用法**: `rcli env check [--strict]`

- `--strict`: 检查工作区是否干净（`git status --porcelain` 为空）、必要的钩子是否就绪等。
- 若检查不通过，退出码非零，并返回具体原因。

---

### 5.5 `data` 命令组

#### `data register`

将目录注册为正式的数据资产，并记录校验和与添加日期。

**用法**: `rcli data register <PATH> --name <NAME> [--desc <TEXT>] [--checksum <SHA256>]`

- `PATH`: 数据目录的路径（相对于仓库根）。
- `--name`: 该资产的唯一标识符（如 `imdb-v1`）。
- `--desc`: 人类可读描述。
- `--checksum`: 若未提供，CLI 自动对目录下所有文件递归计算 SHA256（或可配置算法）。
- 注册信息存入 `.research/data_index.yaml` 或 SQLite `datasets` 表。

#### `data list`

列出所有已注册数据资产。

**用法**: `rcli data list [--json]`

输出名称列表，agent 可直接拿去填写 `--data` 参数。

#### `data info`

查看数据资产的详细信息。

**用法**: `rcli data info <NAME> [--json]`

返回 JSON 包含名称、路径、添加日期、校验和、描述等。

#### `data update`

更新数据的位置或手动触发重新计算校验和。

**用法**: `rcli data update <NAME> [--path <NEW_PATH>] [--recompute-checksum]`

---

### 5.6 `exp` 命令组（实验生命周期）

#### `exp new`

提交一份“实验申请表”，创建实验记录和目录，但**不立即执行**。

**用法**: 
```
rcli exp new --data <DATA_NAME> --cmd <COMMAND> 
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

**用法**: `rcli exp run <EXP_ID> [--] [EXTRA_ARGS...]`

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
7. 如果 rcli 自身被信号中断（SIGINT/SIGTERM），将信号转发给子进程，并将状态置为 `interrupted`。子进程异常终止也会导致 `interrupted`。
8. 更新数据库和 JSON，输出最后几行日志和退出码。

**注意**: `exp run` 会阻塞直到进程结束，MCP 层可将其作为长时间工具调用处理。

#### `exp stop`

终止运行中的实验。

**用法**: `rcli exp stop <EXP_ID> [--signal <SIGNAL>]`

- 默认发送 SIGTERM，可指定 SIGKILL。要求状态为 `running`。

#### `exp status`

查看单个或所有实验的状态。

**用法**: `rcli exp status [EXP_ID] [--json]`

若省略 `EXP_ID`，返回所有实验的摘要（与 `exp list` 类似但包含更详细的当前状态）。

#### `exp metric`

记录结构化指标（如 loss、accuracy）。

**用法**:
- `rcli exp metric <EXP_ID> --step <N> --json '{"loss": 0.5}'`
- `rcli exp metric <EXP_ID> --step <N> --key loss --val 0.5 --key acc --val 0.9`

指标追加到 SQLite `metrics_history` 表，并异步影响下一次 JSON 导出。允许对同 step 同指标覆盖（更新）。

#### `exp param`

在实验运行中或结束后补录参数。与 `exp new` 时的 `--params` 合并，有同名键则覆盖。

**用法**: `rcli exp param <EXP_ID> --json '{"lr": 0.001}'`

#### `exp finish`

用于未通过 `exp run` 管理的手动实验（人类直接运行），或对 `interrupted` 实验主动标记为失败。

**用法**: `rcli exp finish <EXP_ID> --status <finished|failed> [--message <TEXT>]`

#### `exp export`

强制将 SQLite 中的实验数据导出到 `<exp-dir>/<ID>/experiment.json`。通常由同步钩子调用。

**用法**: `rcli exp export <EXP_ID> [--output <PATH>]`

#### `exp list`

列出所有实验的摘要信息，支持过滤。

**用法**: `rcli exp list [--status <STATUS>] [--since <DATE>] [--json]`

---

### 5.7 `db` 命令组

#### `db sync`

执行 SQLite 与 JSON 的双向同步。

**用法**: `rcli db sync [--mode export|import|auto]`

- `export`: 将所有 SQLite 记录导出并覆盖对应实验的 JSON 文件。
- `import`: 扫描所有 `experiment.json`，将更新的内容写回 SQLite。
- `auto`: 比较每个实验的修改时间，最后写入者获胜。若同一实验两者均被修改且内容实际冲突（SHA 不一致），命令报错并列出冲突实验。

`auto` 是默认模式，通常用于 pre-commit 钩子 (`db sync --mode auto`) 和 post-merge 钩子 (`db sync --mode import`)。

#### `db export-all`

将所有实验从 SQLite 导出到一系列 JSON 文件，用于团队共享或备份。

**用法**: `rcli db export-all [--out-dir <DIR>]`

#### `db import`

从指定的 `experiment.json` 或一个包含多个 JSON 的目录导入到 SQLite。仅对不存在或更新的实验执行插入/更新。

**用法**: `rcli db import --from <PATH>`

#### `db status`

显示数据库与实验目录 JSON 之间的同步状态（哪些实验需要更新，哪个方向）。

**用法**: `rcli db status [--json]`

---

### 5.8 `log` 命令组

#### `log show`

显示实验的运行日志。

**用法**: `rcli log show <EXP_ID> [--tail N] [--follow]`

- `--tail N`: 显示最后 N 行。
- `--follow`: 持续输出新内容（类似 `tail -f`），用于 streaming。

---

### 5.9 `config` 命令组

#### `config get`

读取 `.research/config.yaml` 中的某个值。

**用法**: `rcli config get <KEY>`

#### `config set`

设置 `.research/config.yaml` 中的某个值。

**用法**: `rcli config set <KEY> <VALUE>`

支持嵌套键，如 `templates.dir`。

---

## 6. 并发与数据完整性

- **short_id 自增序列**: 在 SQLite 中维护 `seq` 表。分配 ID 时，使用原子操作：`UPDATE seq SET id = id + 1 WHERE name = 'experiment' RETURNING id`。若无行则插入。这保证了即使多个进程同时创建实验，也绝不会产生相同 `short_id`。
- **SQLite 并发**: 数据库以 WAL 模式打开，允许多读并发。写操作（插入指标、更新状态）串行化，但对于典型的实验日志写入频率（秒级）完全足够。若未来有高频写入需求，可通过内存缓冲批量写入。
- **实验 ID 唯一性**: 由于 `short_id` 全局唯一且时间戳以分钟为单位，完全避免了 ID 冲突。CLI 绝不信任 agent 传递的任何 ID 前缀，始终由系统生成。
- **工作区净空保证**: `exp new` 强制依赖 `env check --strict`，防止在脏工作区上创建实验。这不需要文件锁，因为 git status 本身是即时快照。若极端并发下两个进程同时检查并通过，然后一个进程 commit 后另一个进程才创建实验，则后者的 commit hash 将是最新的。这是可接受的：实验记录将绑定它被创建时的 commit，不会是脏的。
- **进程信号与状态**: `exp run` 维持对子进程的引用，确保在 rcli 被终止时能更新实验状态，避免僵尸实验。

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

**文档维护**: 本 PRD 将随 `rcli` 的实现和反馈持续迭代。下一步：根据此 PRD 编写 Rust 代码结构设计，并定义 MCP 工具映射 schema。

--- Original Design Draft End ---
