# 2026-10-30 Bevy 架构与性能优化

本记录的前后测量与像素基准对应字库替换前的 `74f3f1d`。随后经用户允许，GDI 文字改用文泉驿点阵宋体，当前字体资源和保真例外见 [资源说明](resources.md#fixed-chinese-bitmap-font)。下面的历史 GDI 字形像素基准不作为替代字体的验收要求。

本轮减少表现层的重复计算、分配和资产操作。原版规则、共享随机流、命令顺序、帧与毫秒语义、存档格式和素材均沿用既有实现。原程序完整 GUI 一致性验收的限制仍见 [前轮验证记录](verification-2026-10-29.md)。

## 实现

- `render.rs` 独立管理视觉投影。Bevy `Local` 缓存精确比较绘制字段与资源版本，替代每帧 JSON 签名；音效、控件和时钟不参与像素去重。影片时钟继续推进，解码帧到达或实际视觉变化才重绘。稳定帧不修改 `PixelCanvas`，沿用 Bevy 图像资产的变更检测和上传门控。
- 像素合成复用图层缓冲，按完整 `(z, insertion-order)` 键原位排序；提前求屏幕与裁剪交集，减少屏外像素循环；将索引调色板、映射和原浮点取整预计算到查找表。满足原样复制条件的全不透明行直接复制，其余仍用原 alpha 算术。FLIC 直接使用已解码帧，不再为取消条带截取而克隆精灵。GDI 字形以数值码点查询，原字库文件不变。
- 诊断发布先检查 Bevy 资源变更，再比较公开投影；稳定画面不再构造控件 JSON、克隆测试快照或生成签名字符串。测试接口仍完整比较领域快照。
- Bevy 音频资源缓存 WAV 元数据和未定位的原始 `AudioSource`，每个 `AudioPlayer` 保持独立解码器；定位后的尾段、完整周期旋转音轨仍独立持有。缓存随场景回收，资源版本变化后按 `Arc` 身份检查失效。音量和暂停只在状态改变时同步，延后创建的 sink 仍接收完整当前状态；逐帧同步检查和严格大于 250ms 的纠偏保留。
- 地图无移动序列时直接借用快照，有移动时仍构造原注记。声音前态仅在完整 JSON 表示变化时复制；共享比较保留整数、浮点及嵌套 `+0.0/-0.0` 的序列化差别。

## CPU 合成基准

本机 Windows x64、Rust 1.99.1、仓库 `release` 配置（`opt-level = "s"`、LTO）。每个真实场景先完成资源加载和解码，再测 80 次暖重绘及 2,000 次静态 Bevy 更新；两轮均启用同一分配计数器。表中为平均耗时，原始结果还包含重绘 p50/p95。

| 场景 | 重绘前 ms | 重绘后 ms | 合成加速 | 静态检查前→后 µs |
|---|---:|---:|---:|---:|
| 菜单 | 1.943 | 0.281 | 6.90× | 8.50 → 4.61 |
| 公园 | 2.816 | 0.903 | 3.12× | 24.28 → 5.04 |
| 职业选择 | 2.866 | 2.193 | 1.31× | 15.99 → 4.77 |
| 学校考试 | 2.735 | 2.042 | 1.34× | 11.39 → 4.49 |
| 小游戏 gas | 2.936 | 0.585 | 5.02× | 11.02 → 4.65 |
| 人生评价 | 1.900 | 1.491 | 1.27× | 15.86 → 4.50 |

公园每次重绘分配由 83 次降为 7 次，职业页由 164 次降为 6 次，其余四个场景降为 0 次。静态更新原有每帧约 5–7 次分配降至本次 2,000 帧累计 1 次；这些帧前后均没有重绘。

这些数据仅说明同机原生 CPU 合成与去重路径的变化，未计资源下载、初次解码、完整游戏更新、浏览器 WASM、GPU 或显示器等待时间，不能换算成浏览器 FPS。重绘加速也不表示此前静态帧会上传纹理：旧版已有正确上传门控，本轮减少了门控之前的工作。

## 正确性与验证

- 六个场景与优化前保存的 **全部 RGBA 字节逐字节相等**，不是仅比较摘要；摘要同时记录在基准 JSON。
- 保留优化前标量精灵绘制器作为仅测试 oracle，256 个正交组合覆盖透明键、索引/非索引、调色板空槽、重映射、浮点颜色取整、半透明、裁剪、重复与条带。
- 原图与现代 GDI 样本、稳定图层顺序、Bevy 画布脏标记、视频帧失效、音轨周期与纠偏、同序号声音边沿及带符号零均纳入原生测试。旧诊断发布实现作为 wire oracle 核对新缓存的内容与发布帧。
- Windows 最终 Rust 工作区 **241 项通过**（app 77、调度 15、core 146、最终战集成 1、人生示例 2）；Linux 同样 241 项通过，Linux `test-api` 另有 78 项单元与 15 项调度回归通过。
- 最终 Windows WASM、TypeScript、Vite 构建与发布资源校验通过，生产包 **773,202,180 字节**，低于 1 GB。Linux 最终源码的 `wasm32 --release` 检查和 TSC 通过；冷检查首次超时 124 后，使用已建立缓存的增量检查成功，历史日志保留。Linux 本轮没有重新执行 Vite 全资源复制；中途版本的 release/绑定证据没有标为最终源码构建。
- Linux 真实 Chromium 软件 WebGPU **67 项全部通过**（9.4 分钟，0 失败、0 跳过、0 不稳定重试），包括固定像素、影片、真实 WebAudio、输入、存档、离线、更新切换和 GPU 丢失。
- 最终生产包另通过真实建角、保存、SHA-256 校验、刷新及恢复，测试输入为 `undefined`、完整领域快照诊断关闭，页面异常为空。测试服务器由检查自动管理生命周期。首轮浏览器任务因旧服务器已退出而连接拒绝，保留为环境失败记录，没有将其计为通过。

这些结果不替代尚缺少的原版完整 GUI、Windows 浏览器可用适配器及手机/平板真机验收；本轮没有提交、推送或发布。

本地证据：`output/performance-before.json`、`output/performance-after.json`、`output/performance-reference/*.rgba`、`output/performance-native-final.log`、`output/performance-browser-results.json`、`output/performance-production-smoke.json` 和 `output/performance-linux/2026-10-29T16-32-25-345Z/result.json`（目录使用 UTC 时间）。前轮 2026-10-29 的验收记录与 Linux 构建汇总没有覆盖。

## 复现

先按 [开发说明](development.md) 安装冻结依赖并生成场景。修改绘制代码前记录基准；修改后使用相同的素材、输入和工具链执行验证：

```sh
pnpm test:fixtures
cargo run --locked --release -p virtual-life-app --example render_benchmark -- 80 output/performance-before.json --record output/performance-reference
# 修改实现后，--verify 要求旧参考已存在；不会把缺失参考自动当成通过。
cargo run --locked --release -p virtual-life-app --example render_benchmark -- 80 output/performance-after.json --verify output/performance-reference
cargo test --locked --workspace --all-targets
pnpm build:test
pnpm test:web
pnpm build
pnpm verify:dist
```

继续优化时应分别测量场景构建、首次解码和浏览器/GPU 开销；本轮仍保留原版更新与动画的完整调度，未把不同模块统一为固定步长。
