import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import type { AssetManifest, AssetObject, MapCatalog, RulesCatalog } from '../../src/types';

const manifest = JSON.parse(await readFile('assets/manifest.json', 'utf8')) as AssetManifest;
const maps = JSON.parse(await readFile('assets/catalog/maps.json', 'utf8')) as MapCatalog;
const rules = JSON.parse(await readFile('assets/catalog/rules.json', 'utf8')) as RulesCatalog;
const objects = new Map(manifest.objects.map(object => [object.sha256, object]));

function resource(path: string): AssetObject {
  const normalized = path.replaceAll('\\', '/').toLowerCase().replace(/^\.\//, '');
  for (const id of [normalized, `install/${normalized}`, `cd/data/${normalized}`, `cd/${normalized}`]) {
    const entry = manifest.resources[id];
    if (entry) return objects.get(entry.sha256)!;
  }
  throw new Error(`Missing test resource: ${path}`);
}

async function waitCached(page: Page, paths: string[]): Promise<void> {
  await expect.poll(() => page.evaluate(async paths => {
    const cache = await caches.open('virtual-life-disc-v1');
    const missing = await Promise.all(paths.map(async path =>
      await cache.match(new URL(path, location.href), { ignoreVary: true }) ? null : path));
    return missing.filter(path => path !== null);
  }, [...new Set(paths)]), { timeout: 30_000 }).toEqual([]);
}

interface Probe { calls: number; fail: boolean; release: () => void }
declare global { interface Window { preloadTest: Probe } }

async function observeDownload(page: Page, path: string, options: { blocked?: boolean; fail?: boolean } = {}): Promise<void> {
  await page.addInitScript(({ path, options }) => {
    const original = window.fetch.bind(window);
    let release = (): void => {};
    const gate = new Promise<void>(resolve => { release = resolve; });
    const probe: Probe = { calls: 0, fail: !!options.fail, release };
    window.preloadTest = probe;
    window.fetch = async (input, init) => {
      const url = new URL(input instanceof Request ? input.url : String(input), location.href);
      if (url.pathname.endsWith(`/${path}`)) {
        probe.calls++;
        if (options.blocked) await gate;
        if (probe.fail) return new Response('Temporarily unavailable', { status: 503 });
      }
      return original(input, init);
    };
  }, { path, options });
}

test('menu predownloads the next game scene for offline character creation and play', async ({ page, context }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('./');
  await expect(page).toHaveTitle('虚拟人生');
  await expect(page.locator('#original-menu')).toBeVisible();
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  await expect(page.getByLabel('姓名')).toHaveCount(0);

  // Wait for the resources the first playable scene actually uses, without
  // visiting that scene or waiting for unrelated background game bundles.
  const park = maps.maps.find(map => map.id === 'park')!;
  const directory = park.sourceResource.slice(0, park.sourceResource.lastIndexOf('/') + 1);
  const decorations = Object.entries(park.legacySections)
    .filter(([name, fields]) => /^(Spirit|Floater)_\d+$/.test(name) && typeof fields.File === 'string')
    .map(([, fields]) => directory + String(fields.File).replaceAll('\\', '/').toLowerCase());
  // NewGame starts at native life stage 2 (the park); stage 0 is not the
  // initial playable character despite sharing this map.
  const initialStage = 2;
  const actorDirectory = rules.characters.directories[initialStage]!;
  const actorModel = actorDirectory.replace(/\/$/, '').split('/').at(-1)!;
  const needed = [
    'install/bmp/pm0301.bmp', ...park.backgroundTiles.map(tile => tile.resourceId), ...decorations,
    ...rules.characters.animationSuffixes.map(suffix => actorDirectory + suffix.replace(/^M00/i, actorModel)),
    rules.characters.portraits[initialStage]!, rules.stages.find(stage => stage.id === initialStage)!.musicPath,
    ...['011', '013', '012', '010', '04', '05', '09', '10', '11', '12'].map(id => `install/bmp/pm06a${id}.bmp`),
    'install/ani/pm06a14.flc',
  ];
  await waitCached(page, needed.map(path => resource(path).path));
  await context.setOffline(true);
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await page.getByLabel('姓名').fill('预下载测试');
  await page.getByRole('button', { name: '确定', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await expect(page.locator('#logical-ui')).toContainText('预下载测试');
  await expect(page.locator('#fatal')).toBeHidden();
  await page.screenshot({ path: testInfo.outputPath('prefetched-offline-game.png') });
  expect(errors).toEqual([]);
});

test('foreground navigation shares an unfinished background download', async ({ page }) => {
  const profile = resource('install/bmp/pm0301.bmp');
  await observeDownload(page, profile.path, { blocked: true });
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.preloadTest.calls)).toBe(1);
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await expect(page.locator('#original-menu')).toBeHidden();
  await page.evaluate(() => window.preloadTest.release());
  await expect(page.getByLabel('姓名')).toBeVisible();
  expect(await page.evaluate(() => window.preloadTest.calls)).toBe(1);
  await waitCached(page, [profile.path]);
});

test('a failed background download stays out of cache and foreground navigation retries', async ({ page }) => {
  const profile = resource('install/bmp/pm0301.bmp');
  await observeDownload(page, profile.path, { fail: true });
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  await expect.poll(() => page.evaluate(() => window.preloadTest.calls)).toBeGreaterThan(0);
  expect(await page.evaluate(async path => {
    const cache = await caches.open('virtual-life-disc-v1');
    return !!await cache.match(new URL(path, location.href));
  }, profile.path)).toBe(false);
  await page.evaluate(() => { window.preloadTest.fail = false; });
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await expect(page.getByLabel('姓名')).toBeVisible();
  await expect(page.locator('#fatal')).toBeHidden();
  await waitCached(page, [profile.path]);
  expect(await page.evaluate(() => window.preloadTest.calls)).toBeGreaterThan(1);
});

test('a corrupted cached resource is replaced with verified server bytes', async ({ page }) => {
  const profile = resource('install/bmp/pm0301.bmp');
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  await waitCached(page, [profile.path]);
  await page.evaluate(async ({ path, bytes }) => {
    const cache = await caches.open('virtual-life-disc-v1');
    await cache.put(new URL(path, location.href), new Response(new Uint8Array(bytes)));
  }, profile);
  // Reload discards the private memory cache, so this must inspect and repair
  // the persisted bytes instead of silently reusing a valid decoded bitmap.
  await page.reload();
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await expect(page.getByLabel('姓名')).toBeVisible();
  await expect.poll(() => page.evaluate(async path => {
    const response = await (await caches.open('virtual-life-disc-v1')).match(new URL(path, location.href));
    if (!response) return '';
    const digest = await crypto.subtle.digest('SHA-256', await response.arrayBuffer());
    return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
  }, profile.path)).toBe(profile.sha256);
  await expect(page.locator('#fatal')).toBeHidden();
});

test('cache storage failure still allows online scene loading', async ({ page }) => {
  await page.addInitScript(() => {
    CacheStorage.prototype.open = async () => { throw new DOMException('Storage unavailable', 'SecurityError'); };
  });
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await expect(page.getByLabel('姓名')).toBeVisible();
  await expect(page.locator('#fatal')).toBeHidden();
});
