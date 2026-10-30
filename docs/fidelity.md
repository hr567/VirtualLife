# 还原依据与验证边界

本项目以光盘中的 `N2Game.exe`、资源文件和数据表为依据。`assets/catalog/rules.json` 的 `sourceSha256` 固定原程序版本，各导出工具保留虚拟地址。原安装器、EXE、Ghidra 数据库及反编译输出只保存在忽略的 `research/` 目录。

## 证据链

| 子系统 | 原程序位置 | 实现与对照 |
|---|---|---|
| 随机数、角色初始化、年龄推进 | `0x45dec1`、`0x4144c0`、`0x4159a0` | `rng.rs`、`character.rs`、`original-core.json` |
| 原建角按钮、日期、星座与文字 | `0x421490`、`0x422086`、`0x40e580` | `native-profile.json`、固定 GDI 索引观察、原位图字库像素检查；月切换保留原日值，二月表为 29 日 |
| N1 Group 解释器、条件、结果 | `0x40c2e0` 及其调用函数 | `legacy_vm.rs`、`original-vm.json` |
| N1 对白与选择绘制 | `0x40b430`、`0x409d70`、`0x40b6a0` | 原字库、GBK 字节分行、五行坐标、选择箭头与原按键；最长实际对白的独立像素断言 |
| 地图装饰与神灵 | `0x417550`、`0x41e130`、`0x41f5c0` | `spirits.rs`、`original-map-load.json`、`original-spirits.json` |
| 职业、工资、轮回 | 职业与阶段相关表，`0x41b0c0` | `original_rules.rs`、`original-payday.json`、`original-reincarnation.json` |
| 商店、房产、关系、装备研究 | `0x4399b0`、`0x43a7b0`、`0x42a550`、`0x438480`、`0x43d130` | `shops.rs`、`relationships.rs`、`equipment.rs` 及对应 fixtures |
| 六种小游戏 | `0x4265a0`、`0x429350`、`0x456af0`、`0x451690`、`0x445df0`、`0x40e1e0` | `gas.rs`、`minigames.rs`、`hammer.rs`、`fire.rs`、`firebee.rs`、`fist.rs` |
| 学生大赛 | `0x44c010`、`0x44a4d0`、`0x4495a0` | `competition.rs`、`competition_run.rs`、逐帧大赛 fixtures |
| 普通战斗、最终战 | `0x433680`、`0x443b40` | `battle.rs`、`final_battle.rs`、原始机器码对照 fixtures |
| 日常道具、装备与参数页 | `0x404630`、`0x405c60`、`0x406a30`、`0x401550` | `inventory.rs`、`inventory_view.rs`、`status.rs` 与对应机器码样本 |
| 住宅与投资转场 | `0x457260`、`0x457470` | `transition.rs`、`original-transitions.json`，24 组场景、500 次扫描及 173,701 次原始像素复制 |
| 地图总览与总音量 | `0x402800`、`0x4032a0` | 原素材缩略地图、人物与神灵标记、总音量控件及导出元数据 |
| 人生评价、影片、字幕和轮回选择 | `0x44d010`、`0x452260`、`0x452de0`、`0x41c040` | `ending_summary.rs`、`epilogue.rs` 及对应 fixtures |
| 失败/胜利片尾分支 | `0x452260` | `original-epilogue-outros.json`：真实分支选择 03/04，四个独立机器码边界帧；媒体完成时钟为确定性替身 |
| FLIC 与位图 | `0x458030`、`0x457d00`、`0x457dc0`、`0x457ea0`、`0x45cf40` | 有界解码器、全素材检查及原 GDI 调用采样 |

`tools/original-*-vectors.py` 在 Unicorn 中执行原始 x86 指令，替换图形、输入、声音与系统时间等外设调用，输出可提交的 JSON 样本。Rust 测试将数值、随机数状态、帧时钟和有关绘制状态与这些样本比较。输入、时间和内存基线保存在各生成脚本或 fixture 中；不能把局部机器码对照等同于整个 Windows 程序的端到端验证。

Ghidra 的反编译输出只作为阅读辅助；遇到丢失分支或不准确类型时，以实际指令与隔离执行结果为准。公开的脚本可重新导出证据，但构建和 CI 使用已保存样本，不需要运行原 EXE。

## 已确认的原始数据边界

- 算命记录 32、33、37 存在原始加载器未初始化内存或越过有效分数表的读取。47 条其余记录按原分支还原；这三条没有可证明的唯一结果，进入时会保留进度并明确提示，不虚构数值。
- “阻止政府调查”事件选项的原记录 26288 跳至全零节点 0；原解释器默认分支会反复读取该节点，形成确定的死循环。网页保留原数据并明确提示、保留进度，不猜测修正下一跳。`original-dispatch-boundaries.json` 记录原始机器码的重复读取，另覆盖直接进入节点 0 和未引用的坏节点 661。
- 最终战原奖励卡数组只有 32 字节。第 33–36 张会与原 `itemUsed` 标记发生确定的内存别名，网页按机器码样本保留其行为；从第 37 张起会破坏护盾动画对象指针，无法确定唯一后续结果，因此保留进度并明确提示，不把无界写改成自拟的扩容或截断规则。
- 结局字幕和历史中的部分字段未由原构造函数初始化。差分样本使用清零对象内存与从零开始的相对时钟。这是可复现的基线，不能证明任意旧进程堆内容下的行为相同。
- `play/h.flc`、`play/top.flc`、`play/top_h.flc`、`play/water_2.flc` 未被原版 `play.mmi` 引用，其中存在超过图像范围或声明块范围的数据。原解码器对前两者发生图像越界写，`water_2` 的隔离执行发生非法写。原文件保持字节不变，网页解码器拒绝它们；验证只允许清单中固定哈希、长度与错误类型的四个例外，并检查它们仍未被地图引用。
- 两张 `PM0P` BMP 声明 255 个调色板项但使用索引 255。按原 `LoadImageA(0x2010)` 调用在当前 Windows GDI 中采样，缺失项补黑；对应完整图像哈希已存入 `original-bmp.json`。这不是 Windows 9x 的屏幕采样。
- 参数页心形图标与血条按创建时的完整调色板，采用当前 Windows `LoadImageA` / `StretchBlt` 的实测索引映射。`export-status-gdi.py` 可重新生成或以 `--check` 核验 62 种头像对应的映射、源文件及完整像素哈希；[Microsoft 的 DdGetDC 契约](https://learn.microsoft.com/en-us/windows/win32/api/ddrawgdi/nf-ddrawgdi-ddgetdc) 说明了无独立调色板的 8 位表面如何共享显示设备色表。另对原生属性商店、普通商店、装备研究、职业和学校考试采样六种构造色表下的 151 张 BMP，保留原调色板范围并在转换后检查透明色键，详见[原生界面调色板证据](native-ui-palette.md)。这些均为可复现的现代 GDI 基线，尚非 Windows 9x 全屏画面的像素验收。
- 原程序的整数回绕、符号截断、一次帧更新、随机调用顺序与已验证的游戏错误均按证据保留。网页平台的输入采样和可见帧间隔仍可能与历史设备不同。

## 平台验收范围

本次重构由完整 Bevy WASM 应用接管可见界面、输入、显示、音频和场景生命周期。规则核心继续使用原机器码 fixtures，Rust 表现测试从原素材推导坐标、帧区域与资源引用；新浏览器用例位于 `tests/bevy-web/`。旧 TypeScript 视图与其 DOM/Worker 测试已经移除，旧客户端曾通过的流程不能直接计入新客户端验收结果。

经用户明确允许，原先使用 GDI 的文字改用文泉驿点阵宋体 0.9.9，作为接近原风格的成熟中文点阵字体。`assets/catalog/glyphs.json` 与固定 1bpp 字库记录共享字符顺序、字号来源、来源哈希和许可证；客户端不依赖系统字体，也不再为各字号逐字保存 GDI 度量与 alpha 图集。字号、逻辑字距、行高、裁剪和控件热区继续使用既有布局规则；粗体和缩放采用统一规则，不逐字调整。替代文字的字形允许与原版不同，不能再将其像素结果称为原版 GDI 一致性证明。原版位图字库、图像素材和参数页已采样的 GDI 索引映射继续保留，游戏规则、随机流与时序不因此改变。

新测试构建独立启用确定性输入/时间特性，生产构建不启用该接口。Linux CI 配置软件 WebGPU，触控用例通过移动视口与输入模拟。某次构建或原素材像素检查成功不代表所有新界面、全玩法流程或设备已执行；每次结果应对应测试报告、截图、适配器与退出码。具体检查层次见 [验证说明](validation.md)。

Android Chrome、iPhone/iPad Safari 真机，以及 x32dbg 驱动的原游戏完整 GUI、文字和逐帧像素对照尚未全部完成；这些项目不能标为已通过。

## 本地重建研究资料

```sh
node tools/extract-assets.mjs --disc E:\
python -m pip install --target research/tools/python-packages -r tools/research-requirements.txt
python tools/transcode-ending-videos.py --ffmpeg /path/to/ffmpeg
python tools/export-all-tables.py
```

转码会验证每一帧解码图像及音频样本与源文件一致。然后运行资源、Rust、完整解码及浏览器测试。`export-all-tables.py` 按依赖顺序调用所有规则导出器；不要仅重跑基础规则导出器后把缺少后续表的目录发布。

Ghidra headless 可使用 `tools/ghidra/ExportAnalysis.java` 导出本地函数与交叉引用。各 `original-*-vectors.py` 是独立的验证入口，详细的运行基线以脚本为准。x32dbg 工具已准备，但本次未取得可复核的完整交互验证记录。

## 完整人生流程探针

```sh
cargo run --locked --release -p virtual-life-core --example complete_life -- 1 120000 1
```

参数分别是种子数量、每个种子的命令循环上限和首个种子。探针通过公开游戏命令推进，不修改游戏状态或绕过未定义规则；它会经过事件、战斗、小游戏、最终战、评价和轮回，并反复保存再恢复检查状态一致。它选择可执行的固定策略，不代表胜利策略或所有分支覆盖。报告和中途存档写入 `output/life-probes/`。达到循环上限或遇到规则阻塞会失败退出；仅报告中的 `completed: true` 确认对应种子完成。新增内容目录后应重新开始，旧内容版本的存档不会被强制改写为兼容。

三个位置参数之后可附加 `university` 或 `lost` 路线参数，也可与 `resume` 同时使用。探针公开参加高中毕业考试，读取原题库与显示选项以选择正确或错误答案进入对应路线，不注入阶段或属性。路线报告写入 `output/life-probes/<route>/`，另记录 `routeAchieved` 和 `mapIds`。

以下是 2026-10-29 在本次 Bevy 表现层重构前记录的规则核心公开命令流程结果，作为历史证据保留。它们不构成新 Bevy UI 的全流程或像素验收；新核心与新客户端应按当前代码重新运行对应探针和浏览器用例：

| 种子 | 角色 | 结果 | 命令数 | 存档恢复次数 |
|---|---|---|---|---|
| 1 | 男性 | 完成最终战、评价并进入第二次人生 | 41,887 | 2,141 |
| 4 | 女性 | 完成最终战、评价并进入第二次人生 | 37,246 | 2,282 |
| 2 | 女性 | 第 80 回合命中原算命记录 33，按已知边界保留进度并停止 | 19,026 | 1,285 |

前两条路线均经过全部六种小游戏、普通战斗、关系、任务、学生大赛和旅行事件；没有人为修改属性或跳过最终战。固定策略在高中考试选择进入社会，因此不覆盖大学与迷失阶段的完整人生路线；毕业考试分支另有核心测试。种子 2 的退出属于原始数据阻塞，不能计为完整流程通过。

2026-10-29 在当前 Rust 核心上追加运行大学和迷失路线，两份报告均为 `completed: true`、`routeAchieved: true`，经过最终战、评价和轮回：

| 路线 | 命令 | 命令数 | 存档恢复次数 | 经过阶段 |
|---|---|---|---|---|
| 大学 | `cargo run --locked --release -p virtual-life-core --example complete_life -- 1 120000 1 university` | 49,524 | 2,469 | 2、3、4、6 |
| 迷失 | `cargo run --locked --release -p virtual-life-core --example complete_life -- 1 120000 1 lost` | 41,361 | 2,187 | 2、3、5、6 |

对应本地报告为 `output/life-probes/university/seed-1.json` 和 `output/life-probes/lost/seed-1.json`。这补充了规则核心的路线证据，不构成 Bevy 可见界面、所有分支或原程序全画面的验收。
