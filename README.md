# 虚拟人生 · Bevy 网页重制

从 1999 年《虚拟人生》原光盘恢复的浏览器版本。完整 Bevy 应用编译为 WebAssembly，统一管理游戏流程、原版界面、输入、画面、音频和资源生命周期，通过 WebGPU 呈现原版 640×480 画面。游戏状态、随机数和存档在设备本地运行，网站只分发静态文件。

[在线试玩](https://hr567.github.io/VirtualLife/) · [项目仓库](https://github.com/hr567/VirtualLife)。GitHub Pages 发布路径为 `/VirtualLife/`；仓库内实现和线上已部署版本可能不同。

已实现建角、人生阶段、事件与职业、六种小游戏、学生大赛、普通与最终战斗、物品装备、房产关系、参数页、结局和轮回。还原依据及原程序未定义行为见 [保真边界](docs/fidelity.md)。机器码样本、原素材像素检查和浏览器流程测试各自覆盖不同范围，不代表已完成原 Windows 游戏全部 GUI、所有分支和设备的对照验收。

## 本地构建

需要 Git LFS、Node.js、pnpm、Rust 和与 `Cargo.lock` 一致的 `wasm-bindgen-cli`。Rust 与 WASM target 由 `rust-toolchain.toml` 固定，Node.js/pnpm 要求由 `package.json` 固定，Rust 和 JavaScript 依赖分别由两份 lockfile 固定。版本记录见 [依赖基线](docs/dependencies.md)。

```sh
git lfs install
git clone https://github.com/hr567/VirtualLife.git
cd VirtualLife
git lfs pull
git lfs fsck
npm install --global pnpm@12.16.1
pnpm install --frozen-lockfile
cargo install wasm-bindgen-cli --version "$(node tools/wasm-bindgen-version.mjs)" --locked
pnpm verify:assets
pnpm build
pnpm verify:dist
pnpm preview
```

打开 `http://127.0.0.1:4173/VirtualLife/`。开发模式使用 `pnpm dev`；修改 Rust 后需要重新生成 WASM。构建与调试步骤见 [开发说明](docs/development.md)。

目标客户端是网页。浏览器必须在 HTTPS 或 localhost 环境中成功取得 WebGPU 适配器；没有 WebGL 回退。手机以横屏操作为宜。玩家无需安装 Rust 或 Bevy。

正常构建使用仓库中保存的原始素材、规则目录、影片衍生文件和固定字形图集，不需要光盘、原 EXE、Windows 字体、Ghidra、Python 或 FFmpeg。素材使用 Git LFS，部署产物包含实际文件，浏览器不调用 Git LFS 接口。

## 操作与存档

主菜单创建角色后，点击骰子推进人生。原版按钮、文字、人物和动画都绘制在 Bevy 画布内，支持鼠标、键盘及触控。画面保持 4:3，地图支持拖动和方向键。`F6` 打开存档管理，`F10` 返回主菜单，`F11` 请求全屏。

本次重构启用全新存档：IndexedDB 数据库为 `virtual-life-bevy`，导出格式为 `virtual-life-bevy-save`。支持新客户端的保存、读取、导入和导出；旧网页版本及 Windows 原版存档不兼容，也不自动迁移。当前记录和上一份有效备份在同一事务内更新。清除网站数据会删除本地存档，重要进度应导出保存。格式、校验和故障处理见 [存档说明](docs/storage.md)。

客户端按当前场景和临近人生阶段预测下载素材，后台最多并发两个请求，并优先让出前台读取。隐藏页面、离线、省流量和 2G 网络会暂停启动预下载。资源校验后持久缓存；尚未缓存的内容仍需联网。新版本等待旧页面关闭后接管。

## 验证与发布

```sh
pnpm test:formats
pnpm test:platform
cargo test --locked --workspace
cargo run --locked --release -p virtual-life-core --example validate_assets -- assets/game/objects
pnpm test:fixtures
pnpm build:test
pnpm exec playwright install chromium
pnpm test:web
pnpm build
pnpm verify:dist
```

测试站点位于 `output/test-site/`，独立编译确定性输入和时间接口；生产站点位于 `dist/`，不启用测试输入接口。浏览器测试使用实际 Chromium WebGPU，Linux CI 配置软件 GPU。可用 `PLAYWRIGHT_GPU=hardware` 检查实际设备适配器；没有适配器的环境不能计为浏览器测试通过。

`main` 的 GitHub Actions 在资源、Rust、平台、浏览器及静态产物检查通过后发布 Pages，产物上限为 1,000,000,000 字节。生成的 WASM 绑定、`dist/`、测试报告和研究数据库不提交。验收层次与复核方法见 [验证说明](docs/validation.md)。

更多细节：[运行架构](docs/architecture.md) · [资源格式与提取](docs/resources.md) · [依赖冻结](docs/dependencies.md) · [音频时钟](docs/audio.md) · [保真边界](docs/fidelity.md)。
