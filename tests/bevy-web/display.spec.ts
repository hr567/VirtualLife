// Browser plugin not available. Exercise the real browser without GPU APIs.
import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { bmp, png, blit, differingPixels, type Pixels } from './images';
import { open, freeze, control, ready } from './helpers';

test.use({ launchOptions: { args: ['--disable-gpu', '--disable-webgpu', '--disable-webgl', '--disable-software-rasterizer', '--disable-accelerated-2d-canvas'] } });
const manifest = JSON.parse(readFileSync('assets/manifest.json', 'utf8'));
function art(id: string): Pixels {
  const object = manifest.objects.find((o: { sha256: string }) => o.sha256 === manifest.resources[id].sha256);
  return bmp(readFileSync(object.path));
}
function originalMenu(): Pixels {
  const image = art('install/bmp/pm0201.bmp');
  for (let i = 0; i < 4; i++) blit(image, art(`install/bmp/pm020${i + 2}.bmp`), 237, [143, 192, 242, 290][i]!, 0, 25);
  return image;
}
test('no-GPU display retains exact original menu pixels and real profile input', async ({ page }, info) => {
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page); await freeze(page); await page.mouse.move(5, 5);
  await expect(page.locator('#game-canvas')).toHaveAttribute('data-renderer', 'canvas2d');
  expect(differingPixels(png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('no-gpu-original-menu.png') })), originalMenu())).toBe(0);
  await control(page, '开始新游戏'); await ready(page); await control(page, '姓名');
  await page.getByRole('textbox', { name: '角色姓名' }).fill('无显卡回归');
  await page.screenshot({ path: info.outputPath('no-gpu-profile.png') });
  expect(errors).toEqual([]);
});
test('a frozen Canvas scene redraws at exact double size after viewport resize', async ({ page }, info) => {
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page); await freeze(page); await page.mouse.move(5, 5);
  const original = originalMenu();
  await page.setViewportSize({ width: 1280, height: 960 });
  await expect.poll(() => page.locator('#game-canvas').evaluate((canvas: HTMLCanvasElement) => [canvas.width, canvas.height])).toEqual([1280, 960]);
  const shot = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('no-gpu-resized-menu.png') }));
  expect([shot.width, shot.height]).toEqual([1280, 960]);
  for (let y = 0; y < shot.height; y++) for (let x = 0; x < shot.width; x++) {
    const actual = (y * shot.width + x) * 4, expected = (Math.floor(y / 2) * 640 + Math.floor(x / 2)) * 4;
    if (shot.rgba[actual] !== original.rgba[expected] || shot.rgba[actual + 1] !== original.rgba[expected + 1] || shot.rgba[actual + 2] !== original.rgba[expected + 2]) throw new Error(`Resized original pixel differs at ${x},${y}`);
  }
  expect(errors).toEqual([]);
});
test('a rejected adapter starts the same game through the automatic fallback', async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, 'gpu', { configurable: true, value: { requestAdapter: async () => { throw new Error('GPU unavailable'); } } }));
  const errors = await open(page);
  await expect(page.locator('#game-canvas')).toHaveAttribute('data-renderer', 'canvas2d');
  expect(errors).toEqual([]);
});

test('a GPU initialization failure retries once before the first usable scene', async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, 'gpu', { configurable: true, value: {
    requestAdapter: async () => { sessionStorage.setItem('gpu-preflight-attempted', 'true'); return {}; },
    getPreferredCanvasFormat: () => { throw new Error('deliberate GPU initialization failure'); },
  } }));
  await page.goto('./', { waitUntil: 'commit' });
  await expect.poll(() => page.locator('#game-canvas').getAttribute('data-renderer'), { timeout: 90_000 }).toBe('canvas2d');
  await ready(page);
  await expect(page.locator('#boot-status')).toBeHidden();
  expect(await page.evaluate(() => sessionStorage.getItem('gpu-preflight-attempted'))).toBe('true');
  expect(await page.evaluate(() => sessionStorage.getItem('virtual-life-display-retry:/VirtualLife/'))).toBeNull();
});

test.describe('high-density portrait display', () => {
  test.use({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, hasTouch: true });
  test('retina backing size and original touch coordinates stay aligned', async ({ page }, info) => {
    const errors = await open(page); await freeze(page);
    await expect.poll(() => page.locator('#game-canvas').evaluate((canvas: HTMLCanvasElement) => [canvas.width, canvas.height])).toEqual([780, 585]);
    const canvas = await page.locator('#game-canvas').boundingBox();
    await page.touchscreen.tap(canvas!.x + 310 * canvas!.width / 640, canvas!.y + 154 * canvas!.height / 480);
    await ready(page); await control(page, '姓名');
    await page.getByRole('textbox', { name: '角色姓名' }).fill('高分屏回归');
    await page.screenshot({ path: info.outputPath('no-gpu-retina-portrait.png') });
    expect(errors).toEqual([]);
  });
});
