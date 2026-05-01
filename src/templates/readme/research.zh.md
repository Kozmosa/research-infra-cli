# {PROJECT_NAME}

> {PROJECT_DESCRIPTION}

## 项目结构

```
{PROJECT_STRUCTURE}
```

## 快速开始

### 环境准备

```{STACK}
{INSTALL_COMMAND}
```

### 运行实验

```bash
arcli exp new --data <DATA_NAME> --cmd "{EXAMPLE_COMMAND}"
arcli exp run <EXP_ID>
```

## 数据资产

使用 `arcli data register` 注册数据集：

```bash
arcli data register ./data/raw --name my-dataset
```

## 实验管理

| 命令 | 说明 |
|------|------|
| `arcli exp new` | 创建实验 |
| `arcli exp run` | 执行实验 |
| `arcli exp status` | 查看状态 |
| `arcli db sync` | 同步实验记录 |

## 许可证

{LICENSE}
