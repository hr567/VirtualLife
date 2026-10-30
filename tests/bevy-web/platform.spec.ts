import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { createHash } from 'node:crypto';
import { control, create, diagnostic, freeze, importScenario, open, ready, scenario } from './helpers';

interface SaveRecord { slot: string; format: string; version: number; savedAt: string; contentId: string; payload: string; checksum: string }
interface ObservedDevice { destroy(): void; lost: Promise<{ reason: string; message: string }> }
declare global { interface Window { __qaGpuDevices?: ObservedDevice[]; __qaAudioMeters?: AnalyserNode[] } }

async function records(page: Page): Promise<{ current?: SaveRecord; previous?: SaveRecord }> {
  return page.evaluate(() => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 1);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, transaction = db.transaction('saves'), request = transaction.objectStore('saves').getAll();
      request.onerror = () => reject(request.error);
      request.onsuccess = () => resolve(Object.fromEntries(request.result.map(record => [record.slot, record])));
      transaction.oncomplete = () => db.close();
    };
  }));
}
async function saves(page: Page): Promise<void> {
  await page.keyboard.press('F6');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  await ready(page);
}
async function workerControls(page: Page): Promise<void> {
  await expect.poll(() => page.evaluate(() => !!navigator.serviceWorker.controller), { timeout: 60_000 }).toBe(true);
}
async function installedShell(page:Page):Promise<void>{
  await workerControls(page);
  // A controller alone does not prove this build's precache is complete.
  // Assert completed activation and every file from this build's precache
  // before disconnecting; do not fill the cache from the test itself.
  await page.evaluate(()=>navigator.serviceWorker.ready.then(()=>{}));
  await expect.poll(()=>page.evaluate(async()=>{
    const registration=await navigator.serviceWorker.getRegistration(document.baseURI);
    return {active:registration?.active?.state,controlled:navigator.serviceWorker.controller?.scriptURL===registration?.active?.scriptURL};
  }),{timeout:60_000}).toEqual({active:'activated',controlled:true});
  const files=await page.evaluate(async()=>{const response=await fetch(new URL('shell-files.json',document.baseURI),{cache:'no-store'});if(!response.ok)throw new Error('Offline test requires the actual build shell manifest');return response.json()as Promise<string[]>;});
  expect(files.some(file=>file.endsWith('.wasm'))).toBe(true);
  await expect.poll(()=>page.evaluate(async files=>{
    const names=(await caches.keys()).filter(name=>name.startsWith('virtual-life-shell-'));
    const missing:Record<string,string[]>={};
    for(const name of names){const cache=await caches.open(name);const absent=[];for(const file of files)if(!await cache.match(new URL(file,document.baseURI).href,{ignoreVary:true}))absent.push(file);if(absent.length===0)return [];missing[name]=absent;}
    return missing;
  },files),{timeout:60_000}).toEqual([]);
}
function checkedSave(record:SaveRecord):{state:unknown;presentation?:{screen:string;hover:unknown;pressed:unknown};[key:string]:unknown}{
  expect(record).toMatchObject({format:'virtual-life-bevy-save',version:1});
  expect(record.contentId).toMatch(/^[0-9a-f]{64}$/);expect(Number.isFinite(Date.parse(record.savedAt))).toBe(true);
  expect(record.checksum).toBe(createHash('sha256').update(record.payload).digest('hex'));
  const payload=JSON.parse(record.payload);expect(payload).toMatchObject({format:'virtual-life-bevy-save',version:1,contentHash:record.contentId});return payload;
}

test('a corrupt current save can restore its valid backup and the UI rejects old-format imports', async ({ page }, info) => {
  const errors = await open(page); await freeze(page);
  await importScenario(page, 'map-seed-1');
  await expect.poll(async () => (await records(page)).current?.payload).toContain('原版回归1');
  await importScenario(page, 'map-seed-4');
  await expect.poll(async () => (await records(page)).current?.payload).toContain('原版回归4');
  const previous = (await records(page)).previous;
  expect(previous?.payload).toContain('原版回归1');
  await page.evaluate(() => new Promise<void>((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 1);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, transaction = db.transaction('saves', 'readwrite'), store = transaction.objectStore('saves');
      const request = store.get('current');
      request.onsuccess = () => store.put({ ...request.result, checksum: '0'.repeat(64) });
      transaction.oncomplete = () => { db.close(); resolve(); };
      transaction.onerror = () => reject(transaction.error);
    };
  }));
  await saves(page); await control(page, '读取当前进度');
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('存档校验失败');
  expect((await diagnostic(page)).screen).toBe('Saves');
  await control(page, '恢复上一份备份'); await ready(page);
  expect((await diagnostic(page)).snapshot.player).toMatchObject({ name: '原版回归1' });
  expect((await records(page)).previous?.payload).toBe(previous!.payload);

  await saves(page);
  const picking = page.waitForEvent('filechooser'); await control(page, '导入存档');
  await (await picking).setFiles({ name: 'old-client.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify({ ...previous, format: 'virtual-life-save' })) });
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('不支持的存档格式');
  expect((await diagnostic(page)).screen).toBe('Saves');
  expect((await diagnostic(page)).snapshot.player).toMatchObject({ name: '原版回归1' });
  await page.screenshot({ path: info.outputPath('backup-restored-old-format-rejected.png') });
  expect(errors).toEqual([]);
});

test('cached menu and map reload offline without changing the saved player', async ({ page, context }, info) => {
  const failures:{url:string;failure:string|null}[]=[];
  page.on('requestfailed',request=>failures.push({url:request.url(),failure:request.failure()?.errorText??null}));
  const errors = await open(page); await freeze(page); await importScenario(page, 'map-seed-1');
  await expect.poll(async () => (await records(page)).current?.payload).toContain('原版回归1');
  // Give the displayed map one original frame online so its decoration
  // resources are actually visited, then save that complete presentation.
  // Importing at frameMs=0 alone does not initialize these map animations.
  await page.evaluate(()=>window.__virtualLifeBevy!.input!({frameMs:16}));
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  await ready(page);await saves(page);await control(page,'保存当前进度');
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('进度已保存');
  await control(page,'返回游戏');await ready(page);
  const before=(await diagnostic(page)).snapshot;
  await installedShell(page);
  await context.setOffline(true);
  await page.reload();await ready(page);
  expect((await diagnostic(page)).screen).toBe('Menu');
  // Reload starts a real clock in a fresh WASM instance. Freeze before loading
  // the cached snapshot, rather than advancing it into new resource requests.
  await freeze(page);
  try{await control(page,'继续游戏');await ready(page);}finally{await info.attach('offline-network-failures',{body:JSON.stringify(failures,null,2),contentType:'application/json'});}
  expect((await diagnostic(page)).snapshot).toEqual(before);
  expect((await diagnostic(page)).snapshot.player).toMatchObject({ name: '原版回归1' });
  expect((await diagnostic(page)).snapshot.mapId).toBe('park');
  await page.screenshot({ path: info.outputPath('offline-cached-map.png') });
  expect(errors).toEqual([]);
});

test('an uncached scene fails visibly offline and its retry loads the same scene after reconnecting', async ({ page, context }, info) => {
  const errors = await open(page); await freeze(page); await workerControls(page);
  const manifest = JSON.parse(readFileSync('assets/manifest.json', 'utf8'));
  const sha = manifest.resources['install/gas/bg01.bmp'].sha256;
  const object = manifest.objects.find((entry: { sha256: string }) => entry.sha256 === sha);
  const cached = await page.evaluate(async path => {
    const cache = await caches.open('virtual-life-disc-v1');
    return !!await cache.match(new URL(path, document.baseURI).href, { ignoreVary: true });
  }, object.path);
  expect(cached, 'The offline case must request a resource that was never cached').toBe(false);
  await saves(page);
  // The save screen's original fonts must be cached while online; only the
  // imported game scene is meant to exercise an offline cache miss.
  await context.setOffline(true);
  const picking = page.waitForEvent('filechooser'); await control(page, '导入存档');
  await (await picking).setFiles(scenario('gas'));
  await expect.poll(async () => (await diagnostic(page)).error).toBeTruthy();
  const failed = await diagnostic(page);
  expect(failed.snapshot.pending).toMatchObject({ kind: 'minigame', game: 'gas' });
  expect(failed.controls).toContainEqual(expect.objectContaining({ label: '重试 / 返回', enabled: true }));
  await page.screenshot({ path: info.outputPath('offline-uncached-scene-error.png') });
  await context.setOffline(false);
  await page.keyboard.press('Enter'); await ready(page);
  expect((await diagnostic(page)).snapshot.pending).toMatchObject({ kind: 'minigame', game: 'gas' });
  expect((await diagnostic(page)).controls).toContainEqual(expect.objectContaining({ label: '开始狗熊捕鱼', enabled: true }));
  await page.screenshot({ path: info.outputPath('reconnected-scene-retry.png') });
  expect(errors).toEqual([]);
});

for (const unavailable of ['missing', 'null-adapter'] as const) {
  test(`startup explains WebGPU ${unavailable} and offers reload and save export`, async ({ page }, info) => {
    await page.addInitScript(mode => {
      if (mode === 'missing') Object.defineProperty(Navigator.prototype, 'gpu', { configurable: true, get: () => undefined });
      else {
        const gpu = Reflect.get(navigator, 'gpu');
        if (!gpu) throw new Error('This test requires the configured real WebGPU browser before replacing requestAdapter');
        Object.defineProperty(gpu, 'requestAdapter', { configurable: true, value: async () => null });
      }
    }, unavailable);
    await page.goto('./');
    const status = page.locator('#boot-status');
    await expect(status).toHaveAttribute('role', 'alert');
    await expect(status).toContainText(unavailable === 'missing' ? '此浏览器未提供 WebGPU' : '无法取得 WebGPU 显卡设备');
    await expect(status.getByRole('button', { name: '重新加载', exact: true })).toBeVisible();
    await expect(status.getByRole('button', { name: '导出本版本存档', exact: true })).toBeVisible();
    expect(await page.title()).toBe('虚拟人生');
    expect(await page.evaluate(() => window.__virtualLifeBevy)).toBeUndefined();
    await page.screenshot({ path: info.outputPath(`webgpu-${unavailable}.png`) });
  });
}

test('destroying the actual Bevy GPU device preserves an exportable save and reload creates a new device', async ({ page }, info) => {
  await page.addInitScript(() => {
    window.__qaGpuDevices = [];
    const gpu = Reflect.get(navigator, 'gpu');
    if (!gpu) throw new Error('The device-loss case requires WebGPU');
    const requestAdapter = gpu.requestAdapter.bind(gpu);
    gpu.requestAdapter = async (options?: GPURequestAdapterOptions) => {
      const adapter = await requestAdapter(options);
      if (adapter) {
        const requestDevice = adapter.requestDevice.bind(adapter);
        adapter.requestDevice = async (descriptor?: GPUDeviceDescriptor) => {
          const device = await requestDevice(descriptor); window.__qaGpuDevices!.push(device); return device;
        };
      }
      return adapter;
    };
  });
  await open(page); await create(page); await freeze(page);
  await saves(page); await control(page, '保存当前进度');
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
  await control(page, '返回游戏');
  const before = (await records(page)).current!;
  const beforePayload=checkedSave(before);
  const beforeSnapshot=(await diagnostic(page)).snapshot;
  await expect.poll(() => page.evaluate(() => window.__qaGpuDevices?.length ?? 0)).toBeGreaterThan(0);
  const loss = await page.evaluate(async () => {
    const device = window.__qaGpuDevices!.at(-1)!; device.destroy();
    const info = await device.lost; return { reason: info.reason, message: info.message };
  });
  expect(loss.reason).toBe('destroyed');
  await expect(page.locator('#boot-status')).toHaveAttribute('role', 'alert');
  await expect.poll(async () => (await records(page)).current?.savedAt).not.toBe(before.savedAt);
  const saved = (await records(page)).current!;
  const savedPayload=checkedSave(saved);
  const{presentation:beforePresentation,...beforeDomain}=beforePayload;
  const{presentation:savedPresentation,...savedDomain}=savedPayload;
  expect(beforePresentation).toBeDefined();expect(savedDomain).toEqual(beforeDomain);
  expect(savedPresentation).toMatchObject({screen:'Game',hover:null,pressed:null});
  // The fatal-device path disposes the platform diagnostics together with
  // Bevy. Compare the complete persisted domain above, then restore it below.
  const downloading = page.waitForEvent('download');
  await page.getByRole('button', { name: '导出本版本存档', exact: true }).click();
  const download = await downloading;
  const stream = await download.createReadStream(); expect(stream).not.toBeNull();
  const chunks = []; for await (const chunk of stream!) chunks.push(chunk);
  expect(JSON.parse(Buffer.concat(chunks).toString())).toMatchObject({ format: 'virtual-life-bevy-save', payload: saved.payload, checksum: saved.checksum });
  await page.screenshot({ path: info.outputPath('device-loss-save-export.png') });
  await page.getByRole('button', { name: '重新加载', exact: true }).click();
  await expect.poll(async () => (await diagnostic(page)).screen, { timeout: 90_000 }).toBe('Menu');
  await ready(page);
  await freeze(page);
  await expect.poll(() => page.evaluate(() => window.__qaGpuDevices?.length ?? 0)).toBeGreaterThan(0);
  await control(page, '继续游戏'); await ready(page);
  expect((await diagnostic(page)).snapshot).toEqual(beforeSnapshot);
});

async function audioEnergy(page: Page): Promise<number> {
  return page.evaluate(() => Math.max(0, ...(window.__qaAudioMeters ?? []).map(meter => {
    const samples = new Float32Array(meter.fftSize); meter.getFloatTimeDomainData(samples);
    return Math.sqrt(samples.reduce((sum, value) => sum + value * value, 0) / samples.length);
  })));
}
test('a real gesture resumes Bevy audio and opening the save screen pauses and restores its signal', async ({ page }, info) => {
  await page.addInitScript(() => {
    window.__qaAudioMeters = [];
    const observed = new WeakSet<AudioNode>();
    const connect = AudioNode.prototype.connect;
    AudioNode.prototype.connect = new Proxy(connect, {
      apply(target, node: AudioNode, arguments_: unknown[]) {
        const result = Reflect.apply(target, node, arguments_);
        if (arguments_[0] === node.context.destination && !observed.has(node)) {
          observed.add(node);
          const meter = node.context.createAnalyser(); meter.fftSize = 2048;
          Reflect.apply(target, node, [meter]); window.__qaAudioMeters!.push(meter);
        }
        return result;
      },
    });
  });
  const errors = await open(page);
  await expect.poll(() => page.evaluate(() => (Reflect.get(globalThis, Symbol.for('virtual-life-bevy.audio-contexts')) as Set<AudioContext> | undefined)?.size ?? 0)).toBeGreaterThan(0);
  await page.evaluate(async () => {
    const contexts = Reflect.get(globalThis, Symbol.for('virtual-life-bevy.audio-contexts')) as Set<AudioContext>;
    await Promise.all([...contexts].map(context => context.suspend()));
  });
  await create(page);
  await expect.poll(() => page.evaluate(() => [...Reflect.get(globalThis, Symbol.for('virtual-life-bevy.audio-contexts')) as Set<AudioContext>].every(context => context.state === 'running'))).toBe(true);
  await expect.poll(() => audioEnergy(page), { timeout: 15_000 }).toBeGreaterThan(0.00001);
  await saves(page);
  await expect.poll(() => audioEnergy(page), { timeout: 15_000 }).toBeLessThan(0.000001);
  await page.screenshot({ path: info.outputPath('bevy-audio-paused-in-saves.png') });
  await control(page, '返回游戏'); await ready(page);
  await expect.poll(() => audioEnergy(page), { timeout: 15_000 }).toBeGreaterThan(0.00001);
  expect(errors).toEqual([]);
});

test('service-worker install and activate preserve default waiting semantics (mechanism, not update E2E)', async () => {
  const listeners = new Map<string, (event: unknown) => void>();
  const names = new Set(['virtual-life-shell-old', 'virtual-life-disc-v1']);
  const cachedFiles: string[] = []; let skipWaiting = 0, claimed = 0;
  const cache = { addAll: async (files: string[]) => cachedFiles.push(...files) };
  const self = {
    location: { href: 'https://example.test/VirtualLife/sw.js' },
    addEventListener: (type: string, callback: (event: unknown) => void) => listeners.set(type, callback),
    skipWaiting: () => { skipWaiting++; }, clients: { claim: async () => { claimed++; } },
  };
  runInNewContext(readFileSync('public/sw.js', 'utf8').replace('__BUILD_ID__', 'new'), {
    self, URL, caches: {
      open: async (name: string) => { names.add(name); return cache; },
      keys: async () => [...names], delete: async (name: string) => names.delete(name),
    },
    fetch: async () => ({ ok: true, json: async () => ['index.html', 'assets/catalog/rules.json'] }),
  });
  let completion: Promise<unknown> = Promise.resolve();
  const event = { waitUntil: (work: Promise<unknown>) => { completion = work; } };
  listeners.get('install')!(event); await completion;
  expect(skipWaiting).toBe(0); expect(claimed).toBe(0);
  expect(names).toEqual(new Set(['virtual-life-shell-old', 'virtual-life-disc-v1', 'virtual-life-shell-new']));
  expect(cachedFiles).toEqual(['https://example.test/VirtualLife/index.html', 'https://example.test/VirtualLife/assets/catalog/rules.json']);
  // Only the browser's later activation event may clean the old application
  // shell. This isolated check does not claim an old game tab was closed E2E.
  listeners.get('activate')!(event); await completion;
  expect(claimed).toBe(1); expect(skipWaiting).toBe(0);
  expect(names).toEqual(new Set(['virtual-life-disc-v1', 'virtual-life-shell-new']));
});
