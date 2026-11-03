# 开发与构建

## 环境

目标产物是 WebGPU 网页客户端。Rust 工具链及 `wasm32-unknown-unknown` target 由 `rust-toolchain.toml` 固定，Node.js 与 pnpm 要求由 `package.json` 固定。依赖版本及冻结依据见 [依赖基线](dependencies.md)。原生测试还需要当前平台的 Rust 链接器。

先取得 Git LFS 实体素材，再安装 JavaScript 依赖与匹配的绑定工具：

```sh
git lfs pull
git lfs fsck
npm install --global pnpm@12.16.1
pnpm install --frozen-lockfile
cargo install wasm-bindgen-cli --version "$(node tools/wasm-bindgen-version.mjs)" --locked
pnpm verify:assets
```

`tools/build-wasm.mjs` 会核对 CLI 版本是否与 Cargo lockfile 一致。不要单独升级 CLI，也不要手动修改生成的 `web/wasm/` 绑定。

Windows PowerShell 中将绑定工具安装命令写为 `cargo install wasm-bindgen-cli --version (node tools/wasm-bindgen-version.mjs) --locked`。

## 开发循环

```sh
pnpm dev
```

启动前会构建 Rust WASM。Vite 处理网页引导与静态文件；Rust 源码修改后需要另行运行 `pnpm wasm` 并刷新页面。生产构建与本地预览：

```sh
pnpm build
pnpm verify:dist
pnpm preview
```

预览地址为 `http://127.0.0.1:4173/VirtualLife/`。浏览器必须实际取得 WebGPU 适配器；仅存在 `navigator.gpu` 不能证明适配器可用。生产客户端没有 WebGL 回退。

正常构建仅使用已提交的素材、规则、影片衍生文件和固定点阵字库。Python、Ghidra、原程序、FFmpeg 和本机 Windows 字体仅在重新生成相关研究证据或衍生资源时需要；当前文泉驿字库的重建只需 Python 标准库，不需要 Windows 字体或 GDI。`python tools/export-bitmap-font.py --check` 默认从已提交的字体源码档案在内存中读取固定 BDF，核验生成结果；无需准备 `research/`，也不在磁盘解压来源。来源哈希、许可证和字符覆盖记录在字库目录中。显式来源目录可用 `--source-dir` 指定。原版位图文字继续读取原素材。

## 修改边界

- 原版规则、共享随机数、事件执行和存档状态在 `virtual-life-core`。改动应附对应原指令、数据表或固定样本依据，并运行相关差分测试。
- 生命周期、物理输入、原版场景、像素合成、音频实体和预取规划在 `virtual-life-app`。场景中的位置均为原版逻辑像素；新增输入应复用有序输入路径，并明确按下、释放、悬停与暂停语义。
- `virtual-life-web` 与 `web/platform.js` 适配浏览器平台能力。不要在其中加入第二份游戏状态、玩法随机数、独立游戏循环或可见游戏 UI。

原版逐帧规则与毫秒动画不统一改为固定步长模拟。资源路径通过 manifest 解析；原素材和机器码 fixtures 不应为迎合新实现而重新生成。修改原始解释结果前，先核实 [保真边界](fidelity.md)。

## 测试产物

```sh
pnpm test:formats
pnpm test:ci
python tools/test_bitmap_font.py
pnpm test:platform
cargo test --locked --workspace
cargo run --locked --release -p virtual-life-core --example validate_assets -- assets/game/objects
pnpm test:fixtures
pnpm build:test
pnpm exec playwright install chromium
pnpm test:web
```

`test:fixtures` 先执行 `test:lives`：通过公开命令跑男性、女性、大学和迷失路线，保存自然到达的地图状态。随后使用 `test-support` 特性严格恢复并生成当前格式的场景，输出到忽略的 `output/bevy-scenarios/`。历史 `tests/fixtures/browser-*.json` 仍保留为还原与测试证据，不是生产客户端的兼容导入格式。

路线生成最多并行两个进程，分别写入独立目录，全部成功后按 seed 1、seed 4 的顺序合并，保留共享快照的覆盖顺序。`complete_life` 可用 `--output-dir <dir>` 指定本次运行的完整输出目录；省略时沿用默认布局。路线探针和场景导出器均使用 release 构建。

`build:test` 单独启用 `test-api` 并输出到 `output/test-site/`。测试接口只在相应编译特性及 localhost 条件同时满足时接受控件、按键或帧时间输入，不提供任意修改游戏状态的接口。Playwright 默认启动该测试站点；如自行提供服务器，可设置 `PLAYWRIGHT_EXTERNAL_SERVER`。

完成测试后重新运行 `pnpm build` 生成 `dist/`，避免把测试 WASM 当作生产绑定使用。测试产物目录、生成绑定、研究资料和输出证据均不作为源码提交；测试源码与固定 fixtures 应正常保存。各层验收解释见 [验证说明](validation.md)。

GitHub Actions 的 `prepare` 缓存 Rust/WASM 依赖与工具，但每次仍完整执行原生检查、生成四条路线和场景，并分别构建测试及生产站点。四个浏览器分片复用本次生成的素材和测试站点，每台机器只运行一个软件 GPU worker。最终 `build` 门禁合并 blob 报告，核对完整测试清单中的每个 ID 恰好通过一次；缺少分片、测试遗漏、重复或执行失败都会阻止 Pages 部署。`pnpm test:ci` 覆盖这些门禁的失败条件。

CPU 合成基准及逐字节等价性验证见 [Bevy 性能优化记录](performance-2026-10-30.md)。
