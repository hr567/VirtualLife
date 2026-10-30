import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { bmp, png, blit, differingPixels, type Pixels } from './images';
import { diagnostic, ready, control, open, freeze, create, importScenario } from './helpers';

const manifest = JSON.parse(readFileSync('assets/manifest.json', 'utf8'));
function original(resource: string): Pixels {
  const sha = manifest.resources[resource].sha256, object = manifest.objects.find((object: { sha256: string }) => object.sha256 === sha);
  return bmp(readFileSync(object.path));
}
test('compact Chinese bitmap font loads without fetching its source archive', async ({ page }, info) => {
  const metadata = JSON.parse(readFileSync('assets/catalog/glyphs.json', 'utf8'));
  const object = manifest.objects.find((item: { sha256: string }) => item.sha256 === metadata.sha256);
  const source = manifest.objects.find((item: { sha256: string }) => item.sha256 === manifest.resources['derived/font/wqy-source.tar.gz'].sha256);
  expect(metadata).toMatchObject({ formatVersion: 2, pixelFormat: 'mono1' });
  expect(Buffer.byteLength(JSON.stringify(metadata))).toBeLessThan(50_000);
  expect(metadata.bytes).toBeLessThan(1_000_000);
  const requests: string[] = [];
  page.on('request', request => requests.push(request.url()));
  const font = page.waitForResponse(response => response.url().endsWith(object.path));
  const errors = await open(page);
  await control(page, '开始新游戏'); await ready(page);
  const response = await font;
  expect(response.ok()).toBe(true);
  expect((await response.body()).length).toBe(metadata.bytes);
  await expect(page).toHaveTitle(/虚拟人生/);
  await expect(page.locator('#game-canvas')).toBeVisible();
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  await page.screenshot({ path: info.outputPath('bitmap-font-desktop.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: info.outputPath('bitmap-font-portrait.png') });
  expect((await diagnostic(page)).screen).toBe('Profile');
  expect(requests.some(url => url.endsWith(source.path))).toBe(false);
  expect(errors).toEqual([]);
});

test('Bevy renders original menu pixels and all three button states exactly', async ({ page }, info) => {
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page);
  const expected = original('install/bmp/pm0201.bmp');
  for (let i = 0; i < 4; i++) blit(expected, original(`install/bmp/pm020${i + 2}.bmp`), 237, [143, 192, 242, 290][i]!, 0, 25);
  await page.mouse.move(5, 5);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  let actual = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('original-menu.png') }));
  expect(differingPixels(actual, expected)).toBe(0);
  const button = original('install/bmp/pm0202.bmp');
  await page.mouse.move(250, 150);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  blit(expected, button, 237, 143, 25, 25);
  actual = png(await page.locator('#game-canvas').screenshot()); expect(differingPixels(actual, expected)).toBe(0);
  await page.mouse.down();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  blit(expected, button, 237, 143, 50, 25);
  actual = png(await page.locator('#game-canvas').screenshot()); expect(differingPixels(actual, expected)).toBe(0);
  await page.mouse.up(); await ready(page); expect((await diagnostic(page)).screen).toBe('Profile');
  expect(errors).toEqual([]);
});

test('Chinese input, original map, fresh save and reload work through Bevy controls', async ({ page }, info) => {
  const errors = await open(page); await create(page); await freeze(page);
  expect((await diagnostic(page)).snapshot.player).toMatchObject({ name: '原版回归' });
  await page.screenshot({ path: info.outputPath('original-map.png') });
  await page.keyboard.press('F6'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  await control(page, '保存当前进度'); await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
  const stored = await page.evaluate(() => new Promise<unknown>((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 1);
    opening.onsuccess = () => { const db = opening.result, request = db.transaction('saves').objectStore('saves').get('current'); request.onsuccess = () => { db.close(); resolve(request.result); }; request.onerror = () => reject(request.error); };
    opening.onerror = () => reject(opening.error);
  }));
  expect(stored).toMatchObject({ format: 'virtual-life-bevy-save', version: 1 });
  await page.reload(); await ready(page); await control(page, '继续游戏'); await ready(page);
  expect((await diagnostic(page)).snapshot.player).toMatchObject({ name: '原版回归' }); expect(errors).toEqual([]);
});

for (const name of ['gas', 'fist', 'fire', 'firebee', 'spirit-slots', 'relationship', 'competition', 'battle', 'final-battle', 'final-shop', 'final-combat', 'inventory', 'status', 'ending-win', 'ending-loss', 'controlled-dice']) {
  test(`original ${name} scene restores from the new save format`, async ({ page }, info) => {
    test.setTimeout(120_000);
    const errors = await open(page); await freeze(page); await importScenario(page, name);
    const state = await diagnostic(page);
    if (name === 'battle') {
      // The original versus doors are an unskippable timed phase.
      expect(state.snapshot.pending).toMatchObject({ kind: 'battle', game: 'ordinary', state: { showOpening: true } });
    } else expect(state.controls.some(control => control.enabled)).toBe(true);
    await page.screenshot({ path: info.outputPath(`${name}.png`) });
    await page.keyboard.press('F6'); await control(page, '保存当前进度');
    await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
    await page.reload(); await ready(page); await freeze(page); await control(page, '继续游戏'); await ready(page);
    expect((await diagnostic(page)).snapshot.pending).toEqual(state.snapshot.pending); expect(errors).toEqual([]);
  });
}

test('the previous website database is neither loaded nor exported', async ({ page }) => {
  const sentinel = { slot: 'current', format: 'virtual-life-web-save', version: 1, payload: 'legacy-data-must-stay', checksum: 'legacy-checksum' };
  await page.goto('assets/catalog/maps.json');
  await page.evaluate(record => new Promise<void>((resolve, reject) => {
    const opening = indexedDB.open('virtual-life', 1);
    opening.onupgradeneeded = () => opening.result.createObjectStore('saves', { keyPath: 'slot' });
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, transaction = db.transaction('saves', 'readwrite');
      transaction.objectStore('saves').put(record);
      transaction.oncomplete = () => { db.close(); resolve(); };
      transaction.onerror = () => reject(transaction.error);
    };
  }), sentinel);
  await page.addInitScript(() => {
    const audit = { opens: [] as string[], deletions: [] as string[] };
    Object.defineProperty(window, '__legacyDatabaseAudit', { value: audit });
    const original = indexedDB.open.bind(indexedDB);
    indexedDB.open = ((name: string, version?: number) => {
      if (name === 'virtual-life') { audit.opens.push(name); throw new Error('Legacy database must never be accessed'); }
      return version === undefined ? original(name) : original(name, version);
    }) as typeof indexedDB.open;
    const remove = indexedDB.deleteDatabase.bind(indexedDB);
    indexedDB.deleteDatabase = ((name: string) => {
      if (name === 'virtual-life') { audit.deletions.push(name); throw new Error('Legacy database must never be deleted'); }
      return remove(name);
    }) as typeof indexedDB.deleteDatabase;
  });
  const downloads: string[] = [];
  page.on('download', download => downloads.push(download.suggestedFilename()));
  const errors = await open(page); await control(page, '继续游戏');
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('尚无本版本存档');
  await page.keyboard.press('F6');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  await control(page, '导出存档');
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('还没有可以导出的存档');
  expect(await page.evaluate(() => (window as unknown as { __legacyDatabaseAudit: unknown }).__legacyDatabaseAudit)).toEqual({ opens: [], deletions: [] });
  expect(downloads).toEqual([]);
  // This page has no application init script, so it can independently verify
  // the pre-existing record without relaxing the application's access guard.
  const inspecting = await page.context().newPage();
  await inspecting.goto('assets/catalog/maps.json');
  const legacy = await inspecting.evaluate(() => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life', 1);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, reading = db.transaction('saves').objectStore('saves').get('current');
      reading.onsuccess = () => { db.close(); resolve(reading.result); };
      reading.onerror = () => reject(reading.error);
    };
  }));
  expect(legacy).toEqual(sentinel);
  await inspecting.close(); expect(errors).toEqual([]);
});

test('touch coordinates preserve original controls in landscape and portrait', async ({ browser }) => {
  for (const viewport of [{ width: 844, height: 390 }, { width: 390, height: 844 }]) {
    const context = await browser.newContext({ viewport, hasTouch: true });
    const page = await context.newPage(); const errors = await open(page); const canvas = await page.locator('#game-canvas').boundingBox();
    await page.touchscreen.tap(canvas!.x + 310 * canvas!.width / 640, canvas!.y + 154 * canvas!.height / 480);
    await expect.poll(async () => (await diagnostic(page)).screen).toBe('Profile'); await ready(page); expect(errors).toEqual([]); await context.close();
  }
});
