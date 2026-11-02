import { installAudioContextTracking, SaveStorage, downloadSave, SAVE_SLOTS } from './platform.js';

const status = document.querySelector('#boot-status');
installAudioContextTracking();
let failed = false;
function showFailure(error) {
  if (failed) return;
  failed = true;
  status.replaceChildren(); status.hidden = false; status.setAttribute('role', 'alert');
  const message = document.createElement('p');
  message.textContent = error instanceof Error ? error.message : String(error);
  const reload = document.createElement('button'); reload.textContent = '重新加载';
  reload.addEventListener('click', () => location.reload());
  const backup = document.createElement('button'); backup.textContent = '导出本版本存档';
  const slots = document.createElement('select'); slots.setAttribute('aria-label', '导出存档槽位');
  for (const [index, slot] of SAVE_SLOTS.entries()) {
    const option = document.createElement('option'); option.value = slot;
    option.textContent = index === 0 ? '槽位 0 · 自动存档' : `槽位 ${index}`;
    slots.append(option);
  }
  const hint = document.createElement('p'); hint.textContent = '可恢复至最近一次成功保存的记录，未保存的进度无法恢复。';
  backup.addEventListener('click', async () => {
    try {
      const storage = await SaveStorage.open();
      const record = await storage.load(slots.value);
      if (!record) throw new Error('尚无本版本存档。');
      downloadSave(record);
    } catch (error) { message.textContent = error.message; }
  });
  status.append(message, hint, reload, slots, backup);
  void SaveStorage.open().then(async storage => {
    const summaries = await storage.list();
    for (const [index, summary] of summaries.entries()) {
      slots.options[index].textContent += summary.valid ? ` · ${summary.name || '已保存'}` : summary.occupied ? ' · 无法读取' : ' · 空';
    }
    slots.value = summaries.find(summary => summary.valid)?.slot ?? 'slot-0';
  }).catch(() => {});
}
window.addEventListener('virtual-life-fatal', event => showFailure(event.detail));
window.addEventListener('error', event => { if (event.error) showFailure(event.error); });
document.addEventListener('keydown', event => {
  if (['F6', 'F10', 'F11'].includes(event.code)) event.preventDefault();
});
try {
  if (!window.isSecureContext) throw new Error('请通过 HTTPS 或 localhost 打开游戏。');
  if (!navigator.gpu) throw new Error('此浏览器未提供 WebGPU，请使用支持 WebGPU 的浏览器。');
  if (!await navigator.gpu.requestAdapter({ powerPreference: 'low-power' })) throw new Error('无法取得 WebGPU 显卡设备，请检查浏览器和显卡支持后重试。');
  // The wasm start function creates the sole Bevy application and event loop.
  // wasm-bindgen captures the AudioContext constructor when its module loads.
  // Import after installing the gesture gate so CPAL uses the tracked context.
  const { default: init } = await import('./wasm/virtual_life_web.js');
  await init();
  if (!failed) status.hidden = true;
  document.querySelector('#game-canvas').focus();
  if ('serviceWorker' in navigator) {
    navigator.serviceWorker.register(`${import.meta.env.BASE_URL}sw.js`, { scope: import.meta.env.BASE_URL }).catch(error => console.warn('离线缓存未启用：', error.message));
  }
} catch (error) {
  showFailure(error);
  console.error(error);
}
