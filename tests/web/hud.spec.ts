import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import type { AssetManifest } from '../../src/types';

const manifest = JSON.parse(await readFile('assets/manifest.json', 'utf8')) as AssetManifest;
const labels = ['参数一览', '道具一览', '地图浏览', '系统操作'];

async function nativeBitmap(id: string) {
  const object = manifest.objects.find(object => object.sha256 === manifest.resources[id]?.sha256);
  if (!object) throw new Error(`Missing native HUD artwork: ${id}`);
  const bytes = await readFile(object.path);
  const width = bytes.readInt32LE(18), height = bytes.readInt32LE(22), offset = bytes.readUInt32LE(10);
  expect(bytes.readUInt16LE(28)).toBe(8);
  expect(bytes.readUInt32LE(30)).toBe(0);
  const stride = (width + 3) & ~3, palette = 14 + bytes.readUInt32LE(14);
  const index = (x: number, y: number): number => bytes[offset + (height - y - 1) * stride + x]!;
  const rgb = (index: number): number[] => [bytes[palette + index * 4 + 2]!, bytes[palette + index * 4 + 1]!, bytes[palette + index * 4]!];
  return { width, height, index, rgb };
}

const panel = await nativeBitmap('install/bmp/pm06a012.bmp');
// Measure the four rounded transparent cutouts in the original sidebar, so
// copying the same wrong constants into rendering and hitboxes cannot pass.
const slots: { x: number; y: number; width: number; height: number }[] = [];
for (let y = 240; y < 398; y++) {
  if (panel.index(72, y) !== 1) continue;
  const top = y;
  let left = panel.width, right = 0;
  while (y < 398 && panel.index(72, y) === 1) {
    for (let x = 14; x < panel.width; x++) if (panel.index(x, y) === 1) { left = Math.min(left, x); right = Math.max(right, x); }
    y++;
  }
  slots.push({ x: 512 + left, y: top, width: right - left + 1, height: y - top });
}

async function newGame(page: Page): Promise<void> {
  await page.goto('./');
  await expect(page).toHaveTitle('虚拟人生');
  await expect(page.locator('#original-menu')).toBeVisible();
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await page.getByLabel('姓名').fill('菜单对齐测试');
  await page.getByRole('button', { name: '确定', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await expect(page.locator('#fatal')).toBeHidden();
}

async function settledFrame(page: Page): Promise<void> {
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
}

test('sidebar controls follow their original artwork across desktop and phone layouts', async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  expect(slots).toEqual([247, 284, 321, 358].map(y => ({ x: 558, y, width: 52, height: 30 })));
  await newGame(page);
  for (const [name, viewport] of [
    ['desktop', { width: 1100, height: 820 }],
    ['landscape', { width: 844, height: 390 }],
    ['portrait', { width: 390, height: 844 }],
  ] as const) {
    await page.setViewportSize(viewport);
    await expect.poll(() => page.evaluate(({ labels, slots }) => {
      const canvas = document.querySelector<HTMLCanvasElement>('#game-canvas')!.getBoundingClientRect();
      return Math.max(...labels.map((label, index) => {
        const element = [...document.querySelectorAll<HTMLButtonElement>('#logical-ui button')].find(button => button.getAttribute('aria-label') === label)!;
        const button = element.getBoundingClientRect(), expected = slots[index]!;
        return Math.max(
          Math.abs((button.x - canvas.x) * 640 / canvas.width - expected.x),
          Math.abs((button.y - canvas.y) * 480 / canvas.height - expected.y),
          Math.abs(button.width * 640 / canvas.width - expected.width),
          Math.abs(button.height * 480 / canvas.height - expected.height),
        );
      }));
    }, { labels, slots })).toBeLessThan(.05);
    await settledFrame(page);
    await page.screenshot({ path: testInfo.outputPath(`hud-aligned-${name}.png`) });
  }
  await page.setViewportSize({ width: 1100, height: 820 });
  await page.getByRole('button', { name: '参数一览', exact: true }).click();
  await expect(page.locator('#logical-ui')).toHaveAttribute('data-native-phase', 'overview');
  await expect(page.locator('#logical-ui')).toHaveAttribute('data-native-ready', 'true');
  await page.getByRole('button', { name: '返回地图', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  expect(errors).toEqual([]);
});

test('normal, hover, pressed and released menu graphics match the original frame pixels', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.setViewportSize({ width: 820, height: 635 });
  await newGame(page);
  const canvas = (await page.locator('#game-canvas').boundingBox())!;
  expect(canvas.width).toBe(640);
  expect(canvas.height).toBe(480);

  for (const [index, label] of labels.entries()) {
    const button = page.getByRole('button', { name: label, exact: true }), slot = slots[index]!;
    const bitmap = await nativeBitmap(`install/bmp/pm06a${String(index + 9).padStart(2, '0')}.bmp`);
    const matches = async (frame: number): Promise<void> => {
      const expected: number[] = [];
      for (let y = 0; y < slot.height; y++) for (let x = 0; x < slot.width; x++) {
        const color = bitmap.index(x, y + frame * slot.height);
        // Transparent rounded corners reveal the original panel rather than
        // painting the sprite's palette-1 color on top of its gold border.
        const under = panel.index(slot.x - 512 + x, slot.y + y);
        expected.push(...(color === 1 ? panel.rgb(under) : bitmap.rgb(color)), color === 1 && under === 1 ? 0 : 255);
      }
      await settledFrame(page);
      await expect.poll(async () => {
        const png = await page.screenshot({ clip: { x: canvas.x + slot.x, y: canvas.y + slot.y, width: slot.width, height: slot.height } });
        return page.evaluate(async ({ png, expected, width, height }) => {
          const bytes = Uint8Array.from(atob(png), character => character.charCodeAt(0));
          const image = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
          const canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height;
          const context = canvas.getContext('2d')!; context.drawImage(image, 0, 0); image.close();
          const actual = context.getImageData(0, 0, width, height).data;
          let mismatches = 0;
          for (let pixel = 0; pixel < expected.length; pixel += 4) if (expected[pixel + 3] && [0, 1, 2].some(channel => actual[pixel + channel] !== expected[pixel + channel])) mismatches++;
          return mismatches;
        }, { png: png.toString('base64'), expected, width: slot.width, height: slot.height });
      }, { message: `${label} native frame ${frame}`, timeout: 5000 }).toBe(0);
    };
    await page.mouse.move(0, 0);
    await matches(0);
    await button.hover();
    await matches(1);
    await button.focus();
    await page.mouse.down();
    await matches(2);
    // Exercise a real pointerup while suppressing only this visual test's
    // navigation click; the preceding test covers normal click behavior.
    await button.evaluate(button => button.addEventListener('click', event => event.stopImmediatePropagation(), { capture: true, once: true }));
    await page.mouse.up();
    await matches(1);
    await page.mouse.move(0, 0);
    await button.evaluate(button => button.blur());
  }
  await expect(page.locator('#fatal')).toBeHidden();
  expect(errors).toEqual([]);
});
