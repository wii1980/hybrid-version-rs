# hybrid-version

[English](README.md) | **简体中文**

> 融合 Cargo.toml + Git 的编译期版本生成库，专为 Rust `build.rs` 设计。

在编译时合并 **Cargo.toml** 版本元数据与 **Git 仓库** 状态，生成全面的版本常量和构建指纹。

## 功能特性

- **混合版本源** — 从 `Cargo.toml` 读取 `major.minor.patch`，结合 git 分支、提交哈希和时间戳
- **自动 patch 计数** — 当 `patch = 0` 时，自动统计自版本行最后一次变更以来的提交数（基于 git blame）
- **修改行检测** — 追踪已暂存和未暂存的变更，附加 `-D`、`-M{N}` 后缀
- **发布安全** — `modified_cannot_build_release()` 在 release 模式下若有未提交变更则直接 panic
- **丰富指纹输出** — 生成 `SOURCES_FINGERPRINT`（版本+分支+提交+提交时间）和 `BUILD_FINGERPRINT`（构建时间+工具链）
- **构建日志** — 写入带时间戳的构建日志，配合 `cargo:rerun-if-changed` 避免不必要的重编译
- **环境变量导出** — 通过 `.env` 文件或 `setx`（Windows）导出版本字符串到 shell
- **Fluent API** — 方法链式调用：`Version::new(path)?.write_version(out)?.set_output_env("VAR")?.write_buildlog(log)?`
- **Git 子模块支持** — 正确处理嵌套 git 仓库
- **无 `unsafe` 代码** — `#![forbid(unsafe_code)]`

## 快速开始

### 1. 添加构建依赖

```toml
[build-dependencies]
hybrid-version = "0.1.0"
```

### 2. 创建 `build.rs`

```rust
use hybrid_version::error::VResult;
use hybrid_version::version::Version;
use std::path::PathBuf;

fn main() -> VResult<()> {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Buildlog.txt");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();

    Version::new(&manifest_dir)?
        .modified_cannot_build_release()
        .write_version(PathBuf::from(&out_dir).join("version.rs"))?;
    Ok(())
}
```

### 3. 在代码中使用

```rust
include!(concat!(env!("OUT_DIR"), "/version.rs"));

fn main() {
    println!("Version: {}", VERSION);
    println!("Source:  {}", SOURCES_FINGERPRINT);
    println!("Build:   {}", BUILD_FINGERPRINT);
}
```

## 生成的常量

生成的 `version.rs` 提供以下编译时常量：

| 常量 | 示例 | 说明 |
|---|---|---|
| `VERSION` | `"0.5.3.beta1-D/M12"` | 完整版本字符串，含构建元数据 |
| `VERSION_MAJOR` | `0` | 主版本号（来自 Cargo.toml） |
| `VERSION_MINOR` | `5` | 次版本号（来自 Cargo.toml） |
| `VERSION_PATCH` | `3` | 修订号（若 Cargo.toml 中为 0 则自动计算） |
| `BUILD_ID` | `"beta1"` | 可选构建标识（来自 `BUILD_ID` 环境变量） |
| `SOURCES_FINGERPRINT` | `"v0.5.3-D/M dev-57181d0 2023-08-02T14:05:08+08:00"` | 源码级指纹 |
| `BUILD_FINGERPRINT` | `"2023-08-02T14:10:09+08:00 debug \[stable-x86_64-unknown-linux-gnu, rustc 1.71.0, cargo 1.71.0\]"` | 构建环境指纹 |

`VERSION` 字符串后缀含义：

- `-D` — Debug 构建
- `-M12` — Release 构建，有 12 行修改
- `-D/M12` — Debug 构建，有 12 行修改

## Version 公开 API

`Version` 结构体是 hybrid-version 的核心，其公开字段和方法构成了完整的用户接口。

### 构造函数

| 方法 | 说明 |
|---|---|
| `Version::new(path)` | 从项目目录创建 Version 实例。读取 `Cargo.toml` 获取版本号，以及 git 状态（分支、提交哈希、时间戳、修改行数）。 |
| `Version::new_for(path, build_id)` | 与 `new` 相同，但接受一个显式的构建标识符（如 `"beta1"`、`"rc.2"`），会追加到版本字符串中。 |

### 链式方法

所有方法消费并返回 `Self`，支持流畅的链式调用：

| 方法 | 说明 |
|---|---|
| `.modified_cannot_build_release()` | 发布安全门。在 release（`--release`）构建中，若工作区存在未提交的修改则 **panic**。Debug 构建下为无操作。在 `write_version` 前调用，防止意外从脏源码发布构建。 |
| `.write_version(path)` | 在指定路径生成 `version.rs` 文件，包含所有版本常量（见[生成的常量](#生成的常量)）。这是主要的输出方法。 |
| `.set_output_env(var)` | 导出版本字符串到环境变量。Unix/macOS 上写入 `.{VAR}.env` 文件供 `source` 使用；Windows 上使用 `setx`。 |
| `.write_buildlog(path)` | 将单行构建记录追加到日志文件（见[构建日志](#构建日志)）。常配合 `cargo:rerun-if-changed=Buildlog.txt` 使用，避免不必要的重编译。 |

### Getter 方法

| 方法 | 返回类型 | 说明 |
|---|---|---|
| `major()` | `u32` | 主版本号，直接来自 `Cargo.toml` |
| `minor()` | `u32` | 次版本号，直接来自 `Cargo.toml` |
| `patch()` | `u32` | 修订号。若 `Cargo.toml` 中为 `0`，则根据自版本行最后变更以来的提交数自动计算 |
| `build_id()` | `Option<&str>` | 可选的构建标识符，如 `Some("beta1")` |
| `branch()` | `&str` | 当前 git 分支名（detached HEAD 时为空字符串） |
| `commit()` | `&str` | 短 git 提交哈希（如 `57181d0`） |
| `commit_ts()` | `&DateTime` | 提交的作者时间戳（已转换为本地时区） |
| `modified()` | `usize` | 已暂存 + 未暂存的修改行总数（用于计算 `-D`/`-M{N}` 后缀） |
| `build_ts()` | `&DateTime` | 调用 `Version::new()`/`new_for()` 时的时间戳 |

### 使用示例

```rust
use hybrid_version::version::Version;

let ver = Version::new("/path/to/project")?;

println!("{}.{}.{}", ver.major(), ver.minor(), ver.patch()); // 0.5.3
println!("branch: {}, commit: {}", ver.branch(), ver.commit()); // main, 57181d0
println!("modified lines: {}", ver.modified()); // 例如 12
```

### 构建日志

使用 `write_buildlog` 配合 `cargo:rerun-if-changed` 避免不必要的重编译：

```rust
// build.rs
println!("cargo:rerun-if-changed=Buildlog.txt");
Version::new(&manifest_dir)?
    .write_buildlog("Buildlog.txt")?;
```

构建日志示例：

| 构建时间 | 类型 | 分支 | 提交 | 提交时间 | 目标平台 | 编译器 | 环境 |
|---|---|---|---|---|---|---|---|
| 2023-08-02T14:38:52+08:00 | Debug | v0.1.17-D/M | dev-f2098e8 | 2023-08-02T14:26:28+08:00 | stable-x86_64-unknown-linux-gnu | rustc 1.71.0 | cargo 1.71.0 |
| 2023-08-02T14:40:56+08:00 | release | v0.1.18 | dev-848120e | 2023-08-02T14:40:45+08:00 | stable-x86_64-unknown-linux-gnu | rustc 1.71.0 | cargo 1.71.0 |

## 为什么选择 hybrid-version？

与其他仅从 git 标签或 `git describe` 推导版本的 crate 不同，**hybrid-version** 将权威版本号保留在 `Cargo.toml` 中，并用 git 元数据丰富它。这种方式的优势：

- 保持 `Cargo.toml` 作为语义化版本号的唯一权威来源
- `patch = 0` 时，自动从提交计数派生产出版本
- 生成丰富的指纹信息，便于调试和追溯
- 防止意外发布带有未提交变更的构建

## 同类对比

| 特性 | hybrid-version | git-version | git2version | vergen |
|---|---|---|---|---|
| 版本来源 | Cargo.toml + Git | Git describe | Git tags | Git + 其他 |
| 自动 patch（提交计数） | ✅ | ❌ | ❌ | ❌ |
| 修改行计数 | ✅ | 仅 dirty 标记 | ✅ | ❌ |
| SOURCES_FINGERPRINT | ✅ | ❌ | ❌ | ❌ |
| BUILD_FINGERPRINT | ✅ | ❌ | ❌ | ✅ |
| 构建日志 | ✅ | ❌ | ❌ | ❌ |
| 发布安全检查 | ✅ | ❌ | ❌ | ❌ |
| 环境变量导出 | ✅ | ❌ | ❌ | ❌ |
| Fluent API | ✅ | ❌ | ❌ | Builder |

## 许可协议

MIT
