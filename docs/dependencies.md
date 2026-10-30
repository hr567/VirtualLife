# 依赖冻结与更新

本次依赖基线冻结于 **2026-10-29**。项目直接依赖采用精确版本，Cargo 的解析结果提交在 `Cargo.lock`，npm 的解析结果提交在 `pnpm-lock.yaml`。开发、CI 与发布均从这些锁文件构建；普通构建不会自动升级依赖。

## 工具链与直接依赖

| 工具 | 固定版本与来源 |
|---|---|
| Rust | `1.99.1`；`rust-toolchain.toml` 同时安装 `wasm32-unknown-unknown` target |
| Node.js | `.node-version` 与 CI 固定 `26.13.0` LTS；`package.json.engines` 要求至少该版本。参见 [Node.js 官方发布记录](https://nodejs.org/en/blog/release/v26.13.0) |
| pnpm | `12.16.1`；由 `packageManager` 与 CI 配置固定 |
| wasm-bindgen-cli | `0.2.129`；必须与 `Cargo.lock` 中的 `wasm-bindgen` crate 完全同版 |

| Rust 直接依赖 | 冻结版本 |
|---|---|
| bevy | 0.20.1 |
| serde | 1.0.229 |
| serde_json | 1.0.154 |
| sha2 | 0.11.1 |
| encoding_rs | 0.8.42 |
| wasm-bindgen | 0.2.129 |
| wasm-bindgen-futures | 0.4.79 |
| serde-wasm-bindgen | 0.6.5 |
| js-sys | 0.3.106 |
| web-sys | 0.3.106 |

上述版本在冻结日与 crates.io 未撤回、非预发布的最新稳定版本核对。除 `encoding_rs` 位于 app crate 外，版本集中在 workspace dependencies；各 crate 通过 `workspace = true` 共享。Bevy 关闭默认 feature 后显式启用网页所需的渲染、窗口、键鼠、触控、picking、音频和 WebGPU 功能。

| npm 直接开发依赖 | 冻结版本 |
|---|---|
| @playwright/test | 1.64.1 |
| @types/node | 26.6.6 |
| typescript | 7.0.2 |
| vite | 8.3.5 |

npm 版本与官方 registry 的 `latest` 标签核对；Node 类型声明包的版本独立于 Node.js 可执行文件的版本。

## 传递依赖与上游约束

“更新直接依赖到最新稳定版”不意味着把每个传递依赖强制替换成最新 major。Cargo 按上游声明的兼容范围解析；例如 `thiserror = "1.0.7"` 接受 1.x，不能替换为 2.x。范围规则见 [Cargo 官方说明](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html)。

冻结日实际检查的重点结果如下；它们来自当前锁文件、下载的上游 manifest 与官方 crates.io 元数据：

| 包 | 当前锁定 | 官方最新稳定版 | 原因 |
|---|---|---|---|
| rand | 0.10.3 | 0.10.3 | 当前只有一个版本；`bevy_math`、`bevy_shape` 等要求 0.10 系列 |
| getrandom | 0.4.3 | 0.4.3 | 当前只有一个版本；`rand`、`uuid` 和平台相关 `jobserver` 路径要求 0.4 系列 |
| thiserror | 1.0.69、2.0.21 | 2.0.21 | Bevy 使用 2.x；`calloop 0.13.0` 要求 1.0.7、`jni 0.21.1` 要求 1.0.20、`ndk 0.9.0` 要求 1.0.23，因而保留 1.x |

其中 `calloop` 来自 `bevy_winit → winit 0.30.13`；旧版 `jni` 来自音频平台层 `cpal 0.17.3`，`ndk` 来自窗口/音频的平台条件依赖。这些是上游维护的条件分支，锁文件也记录未在网页产物中启用的平台路径。实际 WASM 依赖树应使用 `--target wasm32-unknown-unknown` 检查。

不通过 `[patch]`、伪造版本或强制统一 major 绕开这些约束。升级这类分支时先升级其直接上游，再重新验证锁文件与网页构建。

## 核对与复现

依赖查询脚本 `tools/dependency-snapshot.mjs` 从官方 npm registry、crates.io API 与 sparse index 读取当前稳定版本；npm 官方源暂时不可用时会记录实际使用的镜像地址。查询结果写入 `output/dependencies-snapshot.json`，属于本地验证产物。该脚本负责报告，不会修改 manifest 或锁文件。

提交的[冻结快照](dependencies-snapshot.json)合并了脚本实际成功取得的 14 条记录，以及随后单独查询的 `web-sys` 官方响应摘要；每条保留各自实际 `checkedAt`、来源 URL 和 checksum/integrity，另外保留原查询超时记录，不把快照组装时间当成全部依赖的查询时间。

`web-sys` 的 API 与 sparse index 在批量查询中曾超时。2026-10-29T10:32:15.746Z 再次通过[官方 crates.io API](https://crates.io/api/v1/crates/web-sys) 的 `crate.max_stable_version` 核实为 0.3.106；[对应版本 API](https://crates.io/api/v1/crates/web-sys/0.3.106) 确认 `yanked=false`，SHA256 为 `88261b9deccee56594c11a3460c462c41f58d148598fe70ad77070126a68aba4`。本次成功请求采用 `User-Agent: virtual-life-dependency-audit/1.0` 和 20 秒超时，完整实际响应保存在 `output/web-sys-official-latest.json`、`output/web-sys-official-version.json`；提交快照记录从这些响应提取的字段。`rand`、`getrandom`、`thiserror` 的补充查询结果来自 `output/dependencies-transitive-check.json`，成功项也并入冻结快照。这些查询是冻结日证据，后续日期需要重新执行。

```sh
pnpm deps:snapshot
cargo metadata --locked --format-version 1
cargo tree --locked --target all -i thiserror@1.0.69
cargo tree --locked --target all -i rand@0.10.3
cargo tree --locked --target all -i getrandom@0.4.3
cargo tree --locked --target wasm32-unknown-unknown
pnpm install --frozen-lockfile
cargo test --locked --workspace
pnpm build
```

安装绑定生成器时，先由 `node tools/wasm-bindgen-version.mjs` 读取锁定版本，再执行 `cargo install wasm-bindgen-cli --version <输出版本> --locked`。CI 自动使用该输出；`tools/build-wasm.mjs` 会检查 CLI 的实际版本并拒绝不匹配的生成器。升级 `wasm-bindgen` 时同步检查其 JS/WASM 绑定家族和 CLI，参见 [wasm-bindgen CLI 文档](https://rustwasm.github.io/docs/wasm-bindgen/reference/cli.html)。
