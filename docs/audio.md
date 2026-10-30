# 网页音频时钟与非阻塞恢复

游戏音频由 Bevy 音频实体管理。原始 WAV 的样本、循环周期、保存位置和主音量属于游戏表现契约；网页主线程必须持续处理输入、绘制和浏览器音频回调。

## 同步 seek 的死锁证据

结局评价会播放 `cd/data/evaluate/end01.wav`，同时以保存的结局时钟指定播放位置。原实现发现 `AudioSink::position()` 与该位置相差超过 250 ms 后，在 Bevy 更新系统中调用 `AudioSink::try_seek()`。结局导入后浏览器曾出现 `loading=false` 已发布、随后动画帧和诊断读取一起停止的现象。

本次依赖锁定的实现具有以下调用链：

- `rodio 0.22.2` 的 `src/player.rs:238–256`：`Player::try_seek()` 建立 `SeekOrder`，将其交给音频源，然后在第 247 行通过 `feedback.recv()` 同步等待确认。确认由同文件第 130 行起的音频源 `periodic_access` 回调发出。[Rodio 源码](https://github.com/RustAudio/rodio/blob/v0.22.2/src/player.rs#L238)
- `cpal 0.17.3` 的 `src/host/webaudio/mod.rs:302` 明确说明该后端的 WASM 是单线程；第 310 行建立浏览器回调，第 332–337 行调用 Rust 音频数据回调，第 440 行通过浏览器 `setTimeout` 调度。因此，从网页主线程等待该回调完成会阻止回调本身执行。[CPAL 源码](https://github.com/RustAudio/cpal/blob/v0.17.3/src/host/webaudio/mod.rs#L302)

初始播放位置本身不是这个同步等待：`bevy_audio 0.20.1/src/audio_output.rs:225–226` 通过 `decoder.skip_duration(position).repeat_infinite()` 构造带起点的循环源。不过，这会反复播放起点之后的尾段，不能直接替代原游戏“定位后继续循环完整音轨”的语义。[Bevy 音频源码](https://github.com/bevyengine/bevy/blob/v0.20.1/crates/bevy_audio/src/audio_output.rs#L225)

## 当前处理

`virtual-life-app` 在创建解码源之前处理 PCM 位置，不在网页更新系统中调用同步 seek：

- 解析 WAV 的 PCM 数据区、字节率和样本帧对齐；所有恢复位置对齐到完整样本帧。
- 循环音轨将 PCM 数据按恢复位置旋转为“当前位置到结尾，再接原开头到当前位置”，保留全部样本和原总周期。
- 单次音效只保留尚未播放的尾段；恢复位置已到结尾时不再启动。
- 播放实体记录原音轨起点与周期。需要纠正位置偏差时，销毁旧实体并用已定位的新源替换。暂停和恢复使用实体的 `pause` / `play`，不推进游戏时钟。

对应实现是 `crates/virtual-life-app/src/audio.rs` 的 `WaveInfo` 和 `positioned_wave`，由 Bevy 客户端的音频同步系统使用。这里处理的是已验证的原始 PCM WAV，不把不支持的压缩格式伪装成 PCM。

同步层按资源版本和原字节 `Arc` 身份缓存 WAV 元数据，零起点的原始 `AudioSource` 可共享，每个 Bevy `AudioPlayer` 仍拥有独立解码器。定位后的单次尾段和旋转循环源保持独立。缓存随场景回收；缺失或无效资源在版本变化后重新检查。音量和暂停仅在变化时设置，延后出现的 sink 首次接收完整状态；逐帧漂移检查及严格超过 250ms 的纠偏保持不变。

原建角的 `btnmove.wav` 与 `btndown.wav` 使用 `ui-` 音效实体：九个按钮的悬停边沿及真实按下事件触发，确认和取消只触发原点击声。按下序号可捕获同一显示帧内完成的快速点击；序号不会随存档重放，已开始的尾音按原 WAV 时长保留。界面音效可在建角和主菜单播放，隐藏、加载及确定性暂停仍会冻结；存档、选项和地图总览同时冻结玩法与界面尾音，避免冻结的保存位置触发反复纠偏重播。

## 回归要求

样本级测试覆盖循环恢复后样本顺序和完整周期、单次音效尾段、结束位置与循环取模。浏览器回归必须覆盖胜利和失败结局导入、持续可用的动画帧与诊断接口、暂停恢复、保存后重新载入及影片阶段的音频同步。音频设备尚未解锁时也必须保持页面可响应。

升级 Bevy、Rodio 或 CPAL 时应重新检查以上调度边界，不能通过放宽浏览器测试超时掩盖主线程等待。
