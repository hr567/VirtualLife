# 虚拟人生 · Web 重制

从 1999 年《虚拟人生》原光盘恢复的浏览器版本。Rust 编译为 WebAssembly，WebGPU 按原版 640×480 坐标绘制；游戏状态、随机数和存档都在设备本地运行。网站只分发静态文件。

[在线试玩](https://hr567.github.io/VirtualLife/) · [项目仓库](https://github.com/hr567/VirtualLife)。GitHub Pages 发布路径为 `/VirtualLife/`。

已接入建角、人生阶段、事件与职业、六种小游戏、学生大赛、普通与最终战斗、物品装备、房产关系、参数页、结局和轮回。原始程序存在的未定义行为与尚未验证的平台记录在 [还原依据与边界](docs/fidelity.md) 中；局部机器码对照和 Chromium 测试不代表所有原版分支与设备均已验收。

## 本地构建

需要 Git LFS、Node.js、pnpm、Rust 和 `wasm-bindgen-cli`。Rust 版本及 WASM target 由 `rust-toolchain.toml` 固定，pnpm 版本由 `package.json` 固定。

```sh
git lfs install
git clone https://github.com/hr567/VirtualLife.git
cd VirtualLife
git lfs pull
git lfs fsck
corepack enable
pnpm install --frozen-lockfile
cargo install wasm-bindgen-cli --version 0.2.129 --locked
pnpm verify:assets
pnpm build
pnpm verify:dist
pnpm preview
```

打开 `http://127.0.0.1:4173/VirtualLife/`。开发模式使用 `pnpm dev`。浏览器必须在 HTTPS 或 localhost 环境中成功初始化 WebGPU；移动设备以横屏为宜。

克隆后构建不需要光盘、原 EXE、Ghidra、Python 或 FFmpeg。素材使用 Git LFS 保存，Actions 检出后把实际素材文件放入 Pages 产物，浏览器不调用 Git LFS 接口。

## 操作与存档

主菜单创建角色后，点击骰子推进人生；原版界面按钮支持鼠标和触控，小游戏提供对应的键盘与触控操作。画面保持 4:3，地图浏览支持拖动和方向键。

游戏使用项目专属 IndexedDB 保存当前进度、上一份有效备份和设置。存档管理提供手动保存、恢复、导入及导出。导入文件同时检查格式、内容版本与校验值；不能导入 Windows 原版的存档。清除网站数据会删除本地存档，重要进度可导出为文件保存。

离线缓存只保存应用壳及已经加载成功的素材。遇到尚未缓存的内容，需要联网重试；游戏不会预先下载全部光盘。新版本等待旧页面关闭后接管，避免运行中混用不同的引擎和目录。

## 验证与发布

```sh
pnpm test:formats
cargo test --locked --workspace
cargo run --locked --release -p virtual-life-core --example validate_assets -- assets/game/objects
pnpm build
pnpm verify:dist
pnpm exec playwright install chromium
pnpm test:web
```

浏览器测试启动真实 Chromium WebGPU；Linux CI 使用软件 GPU。截图与失败证据写入系统临时目录，可通过 `PLAYWRIGHT_OUTPUT_DIR` 改变。`PLAYWRIGHT_GPU=hardware` 可使用当前设备的实际适配器。

`main` 的 GitHub Actions 验证 LFS、全部资源 SHA-256、Rust 与浏览器测试、静态产物完整性及 1 GB 体积预算后发布 Pages。生成的 `dist/`、WASM 绑定、分析数据库和原始可执行文件不提交。

更多细节：[架构](docs/architecture.md) · [资源格式与提取](docs/resources.md) · [还原依据与边界](docs/fidelity.md)。
