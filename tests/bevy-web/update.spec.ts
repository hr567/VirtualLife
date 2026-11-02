// Real browser lifecycle coverage. The local server reads the built site and
// changes only sw.js's BUILD response in memory; no build artifact is written.
import { test, expect, type Page } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { createReadStream, readFileSync } from 'node:fs';
import { stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { control, diagnostic, freeze, importScenario, open, ready, saveSelected } from './helpers';

interface SaveRecord { format: string; version: number; contentId: string; payload: string; checksum: string; savedAt: string }
interface WorkerState { scope: string | null; active: string | null; waiting: string | null; installing: string | null; caches: string[]; controller: boolean }
interface VersionedSite { url: string; setBuild(build: 'qa-old' | 'qa-new'): void; requestedVersions: string[]; close(): Promise<void> }

async function versionedSite(): Promise<VersionedSite> {
  const root = path.resolve('output/test-site'), base = '/VirtualLife/';
  const worker = readFileSync(path.join(root, 'sw.js'), 'utf8');
  const buildDeclaration = /const\s+BUILD\s*=\s*(['"])[^'"\r\n]*\1\s*;/g;
  expect([...worker.matchAll(buildDeclaration)], 'Built SW must contain one BUILD declaration').toHaveLength(1);
  let build: 'qa-old' | 'qa-new' = 'qa-old';
  const requestedVersions: string[] = [];
  const mime: Record<string, string> = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.json': 'application/json', '.wasm': 'application/wasm', '.webm': 'video/webm', '.wav': 'audio/wav', '.css': 'text/css; charset=utf-8' };
  const server: Server = createServer(async (request, response) => {
    try {
      if (!['GET', 'HEAD'].includes(request.method ?? '')) { response.writeHead(405).end(); return; }
      const url = new URL(request.url ?? '/', 'http://127.0.0.1');
      if (!url.pathname.startsWith(base)) { response.writeHead(404).end(); return; }
      const relative = decodeURIComponent(url.pathname.slice(base.length)) || 'index.html';
      const file = path.resolve(root, relative);
      if (!file.startsWith(root + path.sep)) { response.writeHead(403).end(); return; }
      const info = await stat(file);
      if (!info.isFile()) { response.writeHead(404).end(); return; }
      let replacement: Buffer | undefined;
      if (relative === 'sw.js') {
        requestedVersions.push(build);
        replacement = Buffer.from(worker.replace(buildDeclaration, `const BUILD = '${build}';`));
      }
      const size = replacement?.length ?? info.size;
      response.setHeader('Content-Type', mime[path.extname(file)] ?? 'application/octet-stream');
      response.setHeader('Cache-Control', 'no-cache');
      response.setHeader('Accept-Ranges', 'bytes');
      let start = 0, end = size - 1;
      if (request.headers.range) {
        const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range);
        if (!range) { response.writeHead(416, { 'Content-Range': `bytes */${size}` }).end(); return; }
        start = Number(range[1]); end = range[2] ? Number(range[2]) : end;
        if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start > end || end >= size) { response.writeHead(416, { 'Content-Range': `bytes */${size}` }).end(); return; }
        response.statusCode = 206;
        response.setHeader('Content-Range', `bytes ${start}-${end}/${size}`);
      }
      response.setHeader('Content-Length', Math.max(0, end - start + 1));
      if (request.method === 'HEAD' || size === 0) response.end();
      else if (replacement) response.end(replacement.subarray(start, end + 1));
      else createReadStream(file, { start, end }).on('error', () => response.destroy()).pipe(response);
    } catch {
      if (!response.headersSent) response.writeHead(404).end();
      else response.destroy();
    }
  });
  await new Promise<void>((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Missing test server port');
  return {
    url: `http://127.0.0.1:${address.port}${base}`, requestedVersions,
    setBuild(value) { build = value; },
    async close() {
      server.closeIdleConnections();
      await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    },
  };
}
async function workerState(page: Page, scope: string): Promise<WorkerState> {
  return page.evaluate(async scope => {
    const registration = await navigator.serviceWorker.getRegistration(scope);
    return { scope: registration?.scope ?? null, active: registration?.active?.state ?? null, waiting: registration?.waiting?.state ?? null, installing: registration?.installing?.state ?? null, caches: await caches.keys(), controller: !!navigator.serviceWorker.controller };
  }, scope);
}
async function record(page: Page): Promise<SaveRecord | null> {
  return page.evaluate(() => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 2);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, transaction = db.transaction('saves'), request = transaction.objectStore('saves').get('slot-1');
      request.onsuccess = () => resolve(request.result ?? null);
      request.onerror = () => reject(request.error);
      transaction.oncomplete = () => db.close();
    };
  }));
}
async function saveCurrent(page: Page): Promise<void> {
  const previous = (await record(page))?.savedAt;
  await page.keyboard.press('F6');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  await saveSelected(page);
  await expect.poll(async () => (await record(page))?.savedAt).not.toBe(previous);
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
  await page.keyboard.press('F6');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Game');
  await ready(page);
}

test('a real SW update waits for both old game pages to close and preserves disc objects and fresh saves', async ({ browser }, info) => {
  test.setTimeout(180_000);
  const site = await versionedSite();
  const context = await browser.newContext({ baseURL: site.url, viewport: { width: 1100, height: 820 } });
  try {
    const page1 = await context.newPage();
    const errors1 = await open(page1); await freeze(page1); await importScenario(page1, 'map-seed-1');
    await expect.poll(async () => !!(await page1.evaluate(() => navigator.serviceWorker.controller))).toBe(true);
    await saveCurrent(page1);

    const page2 = await context.newPage();
    const errors2 = await open(page2); await freeze(page2); await control(page2, '继续游戏'); await control(page2, '读取选中槽位'); await ready(page2);
    await expect.poll(async () => !!(await page2.evaluate(() => navigator.serviceWorker.controller))).toBe(true);
    await saveCurrent(page2);
    const before = (await diagnostic(page2)).snapshot;
    const oldController = await page2.evaluateHandle(() => navigator.serviceWorker.controller);

    // A normal 404 document outside /VirtualLife/ observes the real origin's
    // registration. It has no controller and cannot keep the old worker alive.
    const observer = await context.newPage();
    await observer.goto(new URL('/qa-worker-observer', site.url).href);
    expect(await observer.evaluate(() => navigator.serviceWorker.controller)).toBeNull();
    const oldActive = await observer.evaluateHandle(async scope => (await navigator.serviceWorker.getRegistration(scope))!.active, site.url);
    await expect.poll(async () => (await workerState(observer, site.url)).active).toBe('activated');
    expect((await workerState(observer, site.url)).caches).toContain('virtual-life-shell-qa-old');
    expect((await workerState(observer, site.url)).waiting).toBeNull();
    const savedBefore = (await record(observer))!;
    expect(savedBefore).toMatchObject({ format: 'virtual-life-bevy-save', version: 1 });
    const domainBefore = JSON.parse(savedBefore.payload).state;
    const discBefore = await observer.evaluate(async () => (await (await caches.open('virtual-life-disc-v1')).keys()).map(request => request.url).sort());
    expect(discBefore.length).toBeGreaterThan(0);

    site.setBuild('qa-new');
    await page2.evaluate(async () => {
      const registration = await navigator.serviceWorker.getRegistration();
      if (!registration) throw new Error('The actual old registration was not installed');
      await registration.update();
    });
    await expect.poll(async () => (await workerState(observer, site.url)).waiting, { timeout: 60_000 }).toBe('installed');
    const waiting = await workerState(observer, site.url);
    expect(waiting.caches).toEqual(expect.arrayContaining(['virtual-life-shell-qa-old', 'virtual-life-shell-qa-new', 'virtual-life-disc-v1']));
    expect(await page2.evaluate(old => navigator.serviceWorker.controller === old, oldController)).toBe(true);
    expect(await observer.evaluate(async ({ scope, old }) => (await navigator.serviceWorker.getRegistration(scope))!.active === old, { scope: site.url, old: oldActive })).toBe(true);
    expect((await diagnostic(page2)).snapshot).toEqual(before);

    await page1.close();
    // The second old page alone must still prevent activation and old-cache deletion.
    await page2.bringToFront();
    await ready(page2);
    await saveCurrent(page2); // The remaining old client still works while its update waits.
    expect((await workerState(observer, site.url)).waiting).toBe('installed');
    expect((await workerState(observer, site.url)).caches).toEqual(expect.arrayContaining(['virtual-life-shell-qa-old', 'virtual-life-shell-qa-new']));
    expect(await page2.evaluate(old => navigator.serviceWorker.controller === old, oldController)).toBe(true);
    expect(await observer.evaluate(async ({ scope, old }) => (await navigator.serviceWorker.getRegistration(scope))!.active === old, { scope: site.url, old: oldActive })).toBe(true);
    expect((await diagnostic(page2)).snapshot).toEqual(before);
    expect(errors1).toEqual([]); expect(errors2).toEqual([]);
    await oldController.dispose();
    await page2.close();

    await expect.poll(async () => {
      const state = await workerState(observer, site.url);
      return { active: state.active, waiting: state.waiting, old: state.caches.includes('virtual-life-shell-qa-old'), fresh: state.caches.includes('virtual-life-shell-qa-new') };
    }, { timeout: 30_000 }).toEqual({ active: 'activated', waiting: null, old: false, fresh: true });
    expect(await observer.evaluate(async ({ scope, old }) => (await navigator.serviceWorker.getRegistration(scope))!.active !== old, { scope: site.url, old: oldActive })).toBe(true);
    expect(await oldActive.evaluate(worker => worker?.state)).toBe('redundant');
    await oldActive.dispose();

    const after = (await record(observer))!;
    expect(after).toMatchObject({ format: 'virtual-life-bevy-save', version: 1, contentId: savedBefore.contentId });
    expect(JSON.parse(after.payload).state).toEqual(domainBefore);
    expect(createHash('sha256').update(after.payload).digest('hex')).toBe(after.checksum);
    const discAfter = await observer.evaluate(async () => (await (await caches.open('virtual-life-disc-v1')).keys()).map(request => request.url));
    expect(discAfter).toEqual(expect.arrayContaining(discBefore));

    const current = await context.newPage();
    const errors3 = await open(current); await freeze(current);
    await expect.poll(async () => !!(await current.evaluate(() => navigator.serviceWorker.controller))).toBe(true);
    await control(current, '继续游戏'); await control(current, '读取选中槽位'); await ready(current);
    expect((await diagnostic(current)).snapshot).toEqual(before);
    const final = await workerState(current, site.url);
    expect(final).toMatchObject({ active: 'activated', waiting: null, controller: true });
    expect(final.caches.filter(name => name.startsWith('virtual-life-shell-'))).toEqual(['virtual-life-shell-qa-new']);
    expect(site.requestedVersions).toEqual(expect.arrayContaining(['qa-old', 'qa-new']));
    await current.locator('#game-canvas').screenshot({ path: info.outputPath('updated-worker-preserved-game.png') });
    expect(errors3).toEqual([]);
  } finally {
    try { await context.close(); } finally { await site.close(); }
  }
});

test('real F11 presses enter and leave DOM fullscreen without changing the frozen game domain', async ({ page }, info) => {
  test.setTimeout(180_000);
  const errors = await open(page); await freeze(page); await importScenario(page, 'map-seed-1');
  const before = (await diagnostic(page)).snapshot;
  expect(await page.evaluate(() => document.fullscreenEnabled), 'Configured browser must support the actual Fullscreen API').toBe(true);
  expect(await page.evaluate(() => document.fullscreenElement)).toBeNull();
  await page.locator('#game-canvas').focus();
  await page.keyboard.press('F11');
  await expect.poll(() => page.evaluate(() => document.fullscreenElement?.tagName ?? null)).toBe('HTML');
  await ready(page);
  expect((await diagnostic(page)).snapshot).toEqual(before);
  await page.locator('#game-canvas').screenshot({ path: info.outputPath('f11-fullscreen.png') });
  await page.keyboard.press('F11');
  await expect.poll(() => page.evaluate(() => document.fullscreenElement)).toBeNull();
  await ready(page);
  expect((await diagnostic(page)).snapshot).toEqual(before);
  expect((await diagnostic(page)).screen).toBe('Game');
  expect(errors).toEqual([]);
});
