import './styles.css';
import { Assets, assetUrl } from './assets';
import { EngineClient } from './engine-client';
import { GameRenderer } from './renderer';
import { GameplayView } from './gameplay';
import { BrowserStorage, checksum } from './storage';
import { setMasterVolume } from './audio';
import { OptionsView } from './options';
import type { Command, EngineResult, GameMap, Profile, Snapshot } from './types';

const app = document.querySelector<HTMLElement>('#app')!;
app.innerHTML = `
  <section class="game-shell" aria-label="虚拟人生">
    <div class="toolbar">
      <span class="game-title">虚拟人生</span>
      <button id="main-menu" disabled>主菜单</button>
      <label class="sr-only" for="map-choice">地图浏览</label>
      <select id="map-choice" disabled><option>读取地图…</option></select>
      <button id="save-menu" disabled>存档管理</button>
      <button id="options">游戏选项</button>
      <button id="fullscreen" aria-label="切换全屏">全屏</button>
    </div>
    <div class="portrait-note">横屏可放大原版画面；拖动画面浏览地图。</div>
    <div class="screen-wrap">
      <canvas id="game-canvas" tabindex="0" aria-label="原版地图。拖动或使用方向键浏览。" width="640" height="480"></canvas>
      <nav id="original-menu" class="original-menu" aria-label="游戏主菜单" hidden>
        <button aria-label="开始新游戏"><span class="sr-only">开始新游戏</span></button>
        <button aria-label="继续游戏"><span class="sr-only">继续游戏</span></button>
        <button aria-label="游戏选项"><span class="sr-only">游戏选项</span></button>
        <button aria-label="退出游戏"><span class="sr-only">退出游戏</span></button>
      </nav>
      <div id="logical-ui" class="logical-ui" hidden></div>
      <div id="loading" class="loading" role="status">正在读取光盘资源…</div>
      <div id="fatal" class="fatal" role="alert" hidden></div>
    </div>
    <div class="below">
      <p>进度保存在此浏览器中，可在存档管理导出备份。</p>
      <button id="center-map" disabled>地图居中</button>
    </div>
    <p id="status" class="status" role="status" aria-live="polite"></p>
  </section>
  <dialog id="dialog" aria-labelledby="dialog-title"></dialog>
  <input id="import-file" type="file" accept=".json,.vlsave" hidden />
`;

const element = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const canvas = element<HTMLCanvasElement>('game-canvas');
const mapChoice = element<HTMLSelectElement>('map-choice');
const loading = element<HTMLDivElement>('loading');
const fatal = element<HTMLDivElement>('fatal');
const status = element<HTMLParagraphElement>('status');
const dialog = element<HTMLDialogElement>('dialog');
const engine = new EngineClient();
let assets: Assets;
let renderer: GameRenderer | undefined;
let gameplay: GameplayView | undefined;
let optionsView: OptionsView | undefined;
let storage: BrowserStorage | undefined;
let snapshot: Snapshot | undefined;
let contentId = '';
let busy = false;
let saveQueue: Promise<unknown> = Promise.resolve();
let storageProblem = '';
let mode: 'menu' | 'map' | 'profile' | 'playing' | 'options' = 'menu';
let optionsReturn: 'menu' | 'map' | 'profile' | 'playing' | 'options' = 'menu';
let optionsMap = '';
let navigation = 0;

function message(text: string): void { status.textContent = text; }
function explain(error: unknown): string { return error instanceof Error ? error.message : String(error); }
function resultError(result: EngineResult): string {
  return typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message;
}
function showDialog(title: string, content: string): void {
  dialog.replaceChildren();
  const heading = document.createElement('h2');
  heading.id = 'dialog-title'; heading.textContent = title;
  const paragraph = document.createElement('p'); paragraph.textContent = content;
  const actions = document.createElement('div'); actions.className = 'dialog-actions';
  const close = document.createElement('button'); close.className = 'close'; close.textContent = '返回'; close.onclick = () => dialog.close();
  actions.append(close); dialog.append(heading, paragraph, actions);
  if (!dialog.open) dialog.showModal();
}
function button(text: string, action: () => void | Promise<void>): HTMLButtonElement {
  const control = document.createElement('button'); control.textContent = text;
  control.onclick = () => { void Promise.resolve(action()).catch(error => message(explain(error))); };
  return control;
}
function setBusy(value: boolean): void {
  busy = value;
  mapChoice.disabled = value || !renderer;
  element<HTMLButtonElement>('center-map').disabled = value || !renderer || mode !== 'map';
  element<HTMLButtonElement>('main-menu').disabled = value || !snapshot;
}
async function save(): Promise<void> {
  if (!storage) throw new Error(storageProblem || '此浏览器无法保存进度。');
  const currentStorage = storage;
  const task = saveQueue.then(async () => currentStorage.save(await engine.serialize(), contentId));
  saveQueue = task.catch(() => {});
  await task;
}
function autosave(): void {
  if (storage) void save().catch(error => message(`自动保存失败：${explain(error)}。请导出备份。`));
}

async function setMap(map: GameMap, dispatch = true): Promise<void> {
  if (!renderer) throw new Error('画面尚未准备好。');
  navigation++;
  setBusy(true);
  optionsView?.hide();
  await gameplay?.hide();
  mode = 'map';
  element('original-menu').hidden = true;
  canvas.parentElement?.classList.remove('menu-mode');
  loading.hidden = false;
  loading.textContent = '正在读取原版地图…';
  try {
    if (dispatch && !snapshot?.player) {
      const result = await engine.dispatch({ type: 'exploreMap', mapId: map.id });
      snapshot = result.snapshot;
      if (!result.ok) throw new Error(resultError(result));
    }
    mapChoice.value = map.id;
    await renderer.setMap(map, assets, (count, total) => { loading.textContent = `正在读取原版地图 ${count} / ${total}`; });
    await gameplay?.showMapDecorations(map, snapshot?.mapId === map.id ? snapshot : undefined);
    message(`地图已载入。拖动或按方向键浏览。${storageProblem ? ` ${storageProblem}` : ''}`);
    autosave();
  } finally { loading.hidden = true; setBusy(false); }
}

async function mainMenu(): Promise<void> {
  if (!renderer) return;
  navigation++;
  setBusy(true);
  optionsView?.hide();
  await gameplay?.hide();
  element('logical-ui').hidden = true;
  mode = 'menu';
  loading.hidden = false; loading.textContent = '正在读取原版画面…';
  try {
    const buttons = [143, 192, 242, 290].map((y, index) => ({ resourceId: `install/bmp/pm020${index + 2}.bmp`, x: 237, y, width: 155, height: 25, frames: 3 }));
    await renderer.setScene([{ resourceId: 'install/bmp/pm0201.bmp', x: 0, y: 0, width: 640, height: 480 }, ...buttons], assets);
    element('original-menu').hidden = false;
    canvas.parentElement?.classList.add('menu-mode');
    mapChoice.value = '';
    message('原版主菜单已载入。进度保存在本机浏览器。');
  } finally { loading.hidden = true; setBusy(false); }
}

async function restore(slot: 'current' | 'previous'): Promise<void> {
  if (!storage) throw new Error(storageProblem);
  await saveQueue;
  const record = await storage.load(slot);
  if (!record) throw new Error('没有找到这份存档。');
  if (record.contentId !== contentId) throw new Error('存档对应的资源版本不同，无法读取。');
  const result = await engine.restore(record.payload);
  if (!result.ok) throw new Error(resultError(result));
  snapshot = result.snapshot;
  dialog.close();
  if (snapshot.player && snapshot.phase !== 'exploring') {
    await showGame(result);
    message(slot === 'previous' ? '已恢复上一份存档。' : '已读取存档。');
    return;
  }
  const map = assets.maps.maps.find(candidate => candidate.id === snapshot?.mapId);
  if (map && renderer) await setMap(map, false);
  message(slot === 'previous' ? '已恢复上一份存档。' : '已读取存档。');
}

async function saveMenu(): Promise<void> {
  showDialog('存档管理', '进度保存在此设备的浏览器中。导出文件可以备份或转移至另一台设备。');
  const actions = dialog.querySelector<HTMLElement>('.dialog-actions')!;
  const info = document.createElement('p'); info.className = 'save-info';
  try {
    const current = await storage?.load();
    info.textContent = current ? `上次保存：${new Date(current.savedAt).toLocaleString('zh-CN')}` : '尚无存档。';
  } catch (error) { info.textContent = explain(error); }
  actions.before(info);
  const controls = [
    button('保存进度', async () => { await save(); message('进度已保存。'); dialog.close(); }),
    button('读取存档', async () => restore('current')),
    button('恢复上一份', async () => restore('previous')),
    button('导出备份', async () => { if (!storage) throw new Error(storageProblem); await storage.exportFile(); message('备份文件已导出。'); }),
    button('导入备份', () => { element<HTMLInputElement>('import-file').click(); }),
  ];
  for (const control of controls) { control.disabled = !storage; actions.prepend(control); }
  if (storageProblem) { const note = document.createElement('p'); note.className = 'warning'; note.textContent = storageProblem; actions.before(note); }
}

async function newGame(): Promise<void> {
  if (!renderer) return;
  const entering = ++navigation;
  optionsView?.hide();
  await gameplay?.hide();
  mode = 'profile';
  element('original-menu').hidden = true;
  setBusy(true);
  await renderer.setScene([{ resourceId: 'install/bmp/pm0301.bmp', x: 0, y: 0, width: 640, height: 480 }], assets);
  setBusy(false);
  const ui = element('logical-ui'); ui.hidden = false;
  const form = document.createElement('form');
  form.className = 'game-profile'; form.setAttribute('aria-label', '个人资料');
  form.innerHTML = `
      <input aria-label="姓名" name="name" maxlength="20" autocomplete="off" required />
      <select aria-label="性别" name="sex"><option value="male">男</option><option value="female">女</option></select>
      <select aria-label="血型" name="bloodType"><option>A</option><option>B</option><option>O</option><option>AB</option></select>
      <input aria-label="出生月份" name="birthMonth" type="number" min="1" max="12" value="1" required />
      <input aria-label="出生日期" name="birthDay" type="number" min="1" max="31" value="1" required />
      <button class="profile-submit" type="submit">确定</button><button class="profile-cancel" type="button" id="cancel-new">返回</button>
  `;
  ui.replaceChildren(form);
  form.querySelector<HTMLButtonElement>('#cancel-new')!.onclick = () => { void mainMenu().catch(error => message(explain(error))); };
  form.onsubmit = event => {
    event.preventDefault();
    const data = new FormData(form);
    const profile: Profile = { name: String(data.get('name')).trim(), sex: data.get('sex') as Profile['sex'], bloodType: data.get('bloodType') as Profile['bloodType'], birthMonth: Number(data.get('birthMonth')), birthDay: Number(data.get('birthDay')) };
    const date = new Date(2000, profile.birthMonth - 1, profile.birthDay);
    if (!profile.name || date.getMonth() !== profile.birthMonth - 1) { message('请输入有效的姓名和生日。'); return; }
    void engine.dispatch({ type: 'newGame', profile, seed: crypto.getRandomValues(new Uint32Array(1))[0]! }).then(async result => {
      snapshot = result.snapshot;
      if (entering !== navigation) return;
      if (!result.ok) { showDialog('暂时无法开始新游戏', resultError(result)); return; }
      await showGame(result);
      autosave();
    }).catch(error => showDialog('无法开始新游戏', explain(error)));
  };
}

element<HTMLButtonElement>('main-menu').onclick = () => { void mainMenu().catch(error => showDialog('画面读取失败', explain(error))); };
const originalMenuButtons = element('original-menu').querySelectorAll<HTMLButtonElement>('button');
originalMenuButtons.forEach((control, index) => {
  const resource = `install/bmp/pm020${index + 2}.bmp`;
  control.onpointerenter = control.onfocus = () => renderer?.setSpriteFrame(resource, 1, 3);
  control.onpointerleave = control.onblur = () => renderer?.setSpriteFrame(resource, 0, 3);
  control.onpointerdown = () => renderer?.setSpriteFrame(resource, 2, 3);
  control.onpointerup = () => renderer?.setSpriteFrame(resource, 1, 3);
});
originalMenuButtons[0]!.onclick = () => { void newGame().catch(error => showDialog('画面读取失败', explain(error))); };
originalMenuButtons[1]!.onclick = () => { void restore('current').catch(error => showDialog('继续游戏', explain(error))); };
originalMenuButtons[2]!.onclick = () => { void openSystemOptions().catch(error => message(explain(error))); };
originalMenuButtons[3]!.onclick = () => {
  void (async () => {
    if (snapshot && storage) await save();
    if (document.fullscreenElement) await document.exitFullscreen();
    showDialog('退出游戏', storage ? '进度已保存在本机。现在可以关闭此页面，或返回主菜单。' : '浏览器存储不可用，本次进度未保存。');
  })().catch(error => showDialog('退出游戏', `保存没有成功：${explain(error)}`));
};
element<HTMLButtonElement>('save-menu').onclick = () => { void saveMenu().catch(error => message(explain(error))); };
mapChoice.onchange = () => {
  const map = assets.maps.maps.find(candidate => candidate.id === mapChoice.value);
  if (map && !busy) void setMap(map).catch(error => showDialog('地图读取失败', explain(error)));
};
element<HTMLButtonElement>('center-map').onclick = () => {
  if (renderer?.map) renderer.center(renderer.map.dimensions.width / 2, renderer.map.dimensions.height / 2);
};
element<HTMLButtonElement>('fullscreen').onclick = () => {
  const action = document.fullscreenElement ? document.exitFullscreen() : document.documentElement.requestFullscreen?.();
  if (action) void action.catch(() => message('此浏览器不允许全屏，可以横屏浏览。'));
  else message('此浏览器不提供全屏模式，可以横屏浏览。');
};
element<HTMLButtonElement>('options').onclick = () => {
  showDialog('游戏选项', '当前画面保持原版 640 × 480 比例。拖动或方向键浏览地图；手机和平板建议横屏。');
  const actions = dialog.querySelector<HTMLElement>('.dialog-actions')!;
  actions.prepend(button('保护本机存档', async () => {
    const granted = await navigator.storage?.persist?.();
    message(granted ? '已获得浏览器持久存储许可。请继续定期导出备份。' : '浏览器未授予持久存储许可，请定期导出备份。');
    dialog.close();
  }));
  const audioLabel = document.createElement('label'), volume = document.createElement('input'), output = document.createElement('output');
  volume.type = 'range'; volume.min = '0'; volume.max = '100'; volume.step = '1'; volume.value = '50'; volume.setAttribute('aria-label', '总音量');
  void storage?.setting('masterVolume', 50).then(value => { volume.value = String(value); output.textContent = `${value}%`; });
  volume.oninput = () => { const value = Number(volume.value); setMasterVolume(value); output.textContent = `${value}%`; void storage?.setSetting('masterVolume', value); };
  audioLabel.append(document.createTextNode('总音量 '), volume, output); actions.before(audioLabel);
};
element<HTMLInputElement>('import-file').onchange = event => {
  const input = event.currentTarget as HTMLInputElement;
  const file = input.files?.[0]; input.value = '';
  if (!file || !storage) return;
  const currentStorage = storage;
  void (async () => {
    const record = await currentStorage.readImport(file, contentId);
    const result = await engine.restore(record.payload);
    if (!result.ok) throw new Error(resultError(result));
    snapshot = result.snapshot;
    await save();
    dialog.close();
    if (snapshot.player && snapshot.phase !== 'exploring') { await showGame(result); message('备份已导入。'); return; }
    const map = assets.maps.maps.find(candidate => candidate.id === snapshot?.mapId);
    if (map && renderer) await setMap(map, false);
    message('备份已导入。');
  })().catch(error => showDialog('导入失败', explain(error)));
};

let drag: { id: number; x: number; y: number } | undefined;
canvas.onpointerdown = event => {
  if (busy || !renderer || mode !== 'map') return;
  canvas.focus({ preventScroll: true }); canvas.setPointerCapture(event.pointerId);
  drag = { id: event.pointerId, x: event.clientX, y: event.clientY };
};
canvas.onpointermove = event => {
  if (!drag || drag.id !== event.pointerId || !renderer) return;
  const rect = canvas.getBoundingClientRect();
  renderer.pan((drag.x - event.clientX) * 640 / rect.width, (drag.y - event.clientY) * 480 / rect.height);
  drag.x = event.clientX; drag.y = event.clientY;
};
canvas.onpointerup = canvas.onpointercancel = () => { drag = undefined; };
canvas.onkeydown = event => {
  const moves: Record<string, [number, number]> = { ArrowUp: [0, -38], ArrowDown: [0, 38], ArrowLeft: [-66, 0], ArrowRight: [66, 0] };
  const move = moves[event.key];
  if (move && !busy && mode === 'map') { event.preventDefault(); renderer?.pan(...move); }
};

async function serviceWorker(): Promise<void> {
  if (!import.meta.env.PROD || !('serviceWorker' in navigator)) return;
  try {
    const registration = await navigator.serviceWorker.register(assetUrl('sw.js'), { scope: import.meta.env.BASE_URL });
    registration.addEventListener('updatefound', () => {
      const worker = registration.installing;
      worker?.addEventListener('statechange', () => {
        if (worker.state === 'installed' && navigator.serviceWorker.controller) message('新版本已准备好；关闭本游戏的所有标签页，再打开即可更新。');
      });
    });
    await navigator.serviceWorker.ready;
    if (!navigator.serviceWorker.controller) await new Promise<void>(resolve => {
      navigator.serviceWorker.addEventListener('controllerchange', () => resolve(), { once: true });
      setTimeout(resolve, 3000);
    });
  } catch { message('离线缓存未能启用；联网时仍可浏览。'); }
}

async function start(): Promise<void> {
  const storageReady = BrowserStorage.open().then(value => { storage = value; }).catch(() => { storageProblem = '浏览器存储不可用，当前进度无法保存。'; });
  assets = await Assets.load(engine);
  snapshot = await engine.init(assets.engineContent);
  contentId = await checksum(JSON.parse(await engine.serialize()).contentHash);
  await storageReady;
  element<HTMLButtonElement>('save-menu').disabled = false;
  const chooseMap = document.createElement('option'); chooseMap.value = ''; chooseMap.textContent = '地图浏览'; chooseMap.disabled = true;
  mapChoice.replaceChildren(chooseMap, ...assets.maps.maps.map(map => {
    const option = document.createElement('option'); option.value = map.id; option.textContent = `地图浏览 · ${map.id.toUpperCase()}`; return option;
  }));
  renderer = await GameRenderer.create(canvas, async detail => {
    navigation++;
    fatal.hidden = false;
    fatal.textContent = `显示设备已中断。${detail} 请刷新页面恢复画面。`;
    await gameplay?.hide();
    try { await save(); } catch { /* The last committed save remains intact. */ }
    setBusy(true);
  });
  optionsView = new OptionsView(renderer, assets, engine, element('logical-ui'), async () => {
    optionsView!.hide();
    if (optionsReturn === 'playing' && snapshot) await showGame({ ok: true, snapshot, events: [] });
    else if (optionsReturn === 'map') { const map = assets.maps.maps.find(item => item.id === optionsMap); if (map) await setMap(map, false); else await mainMenu(); }
    else await mainMenu();
  }, () => { void saveMenu().catch(error => message(explain(error))); }, async () => { await mainMenu(); }, value => { void storage?.setSetting('masterVolume', value); }, message);
  gameplay = new GameplayView(renderer, assets, engine, element('logical-ui'), sendGameCommand, message, () => { void saveMenu().catch(error => message(explain(error))); }, () => { void openSystemOptions().catch(error => message(explain(error))); });
  setMasterVolume(await storage?.setting('masterVolume', 50) ?? 50);
  await serviceWorker();
  const saved = await storage?.load().catch(error => { message(explain(error)); return undefined; });
  if (saved && saved.contentId === contentId) {
    const result = await engine.restore(saved.payload);
    if (result.ok) snapshot = result.snapshot;
  }
  if (!assets.maps.maps.length) throw new Error('光盘中没有可加载的地图。');
  mapChoice.value = '';
  await mainMenu();
}

async function showGame(result: EngineResult): Promise<void> {
  if (!gameplay) return;
  if (result.snapshot.phase === 'completed') { await mainMenu(); message(result.snapshot.message); return; }
  optionsView?.hide();
  mode = 'playing';
  element('original-menu').hidden = true;
  canvas.parentElement?.classList.remove('menu-mode');
  setBusy(true);
  try { await gameplay.show(result.snapshot, result.events); }
  finally { setBusy(false); }
}

async function openSystemOptions(): Promise<void> {
  if (!optionsView || optionsView.active || busy) return;
  optionsReturn = mode; optionsMap = renderer?.map?.id ?? '';
  await gameplay?.hide(); mode = 'options'; element('original-menu').hidden = true; canvas.parentElement?.classList.remove('menu-mode');
  setBusy(true); try { await optionsView.show(); } finally { setBusy(false); }
}

async function sendGameCommand(command: Command): Promise<EngineResult> {
  const currentNavigation = navigation;
  const previousPhase = snapshot?.pending?.state?.phase;
  const result = await engine.dispatch(command);
  snapshot = result.snapshot;
  const timed = command.type === 'minigameTick' || command.type === 'relationshipTick' || command.type === 'competitionTick' || command.type === 'endingTick' || command.type === 'battleTick' || command.type === 'inventoryTick' || command.type === 'statusTick';
  if (currentNavigation === navigation && mode === 'playing' && (result.ok || snapshot.pending)) {
    if (timed) await gameplay?.show(snapshot, result.events);
    else await showGame(result);
  }
  if (!timed || previousPhase !== snapshot.pending?.state?.phase) autosave();
  return result;
}

new ResizeObserver(() => {
  element('logical-ui').style.setProperty('--game-scale', String(canvas.getBoundingClientRect().width / 640));
}).observe(canvas);

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'hidden' && snapshot) autosave();
  if (document.visibilityState === 'visible') renderer?.draw();
});
void start().catch(error => {
  loading.hidden = true; fatal.hidden = false; fatal.textContent = explain(error); message('未能进入画面。');
});
