// Browser plugin is unavailable; use the repository's real WebGPU Playwright
// harness. These checks compare original BMP/font bytes and observed modern
// Windows GDI index conversions, not an unavailable historical GUI screenshot.
import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { png, differingPixels, type Pixels } from './images';
import { control, diagnostic, freeze, open, ready } from './helpers';

interface Graphic { resource: string; x: number; y: number; width: number; height: number; frameHeight?: number; transparent: number | null; bloodType?: string; index?: number }
interface Creator {
  background: Graphic; buttons: (Graphic & { id: string })[]; blood: Graphic[]; zodiac: Graphic[];
  gdiIndexMaps: Record<string, [number, number][]>;
  gdiObservation: { targetPalette: number[]; images: Record<string, { sourceSha256: string }> };
  defaultName: string;
}
interface Manifest { resources: Record<string, { sha256: string }>; objects: { sha256: string; path: string }[] }
const manifest = JSON.parse(readFileSync('assets/manifest.json', 'utf8')) as Manifest;
const metadata = JSON.parse(readFileSync('assets/catalog/native-profile.json', 'utf8')) as Creator;
function bytes(resource: string): Buffer {
  const hash = manifest.resources[resource]!.sha256;
  const object = manifest.objects.find(object => object.sha256 === hash);
  if (!object) throw new Error(`Missing original creator resource: ${resource}`);
  return readFileSync(object.path);
}
function draw(target: Pixels, graphic: Graphic, frame = 0): void {
  const source = bytes(graphic.resource), width = source.readInt32LE(18), signedHeight = source.readInt32LE(22);
  expect(source.readUInt16LE(28)).toBe(8); expect(source.readUInt32LE(30)).toBe(0);
  const height = Math.abs(signedHeight), stride = Math.ceil(width / 4) * 4, start = source.readUInt32LE(10);
  const map = new Map(metadata.gdiIndexMaps[graphic.resource] ?? []), palette = metadata.gdiObservation.targetPalette;
  for (let y = 0; y < (graphic.frameHeight ?? graphic.height); y++) for (let x = 0; x < width; x++) {
    const sy = y + frame * (graphic.frameHeight ?? graphic.height);
    const raw = source[start + (signedHeight > 0 ? height - 1 - sy : sy) * stride + x]!;
    const index = map.get(raw) ?? raw;
    if (index === graphic.transparent) continue;
    const to = ((y + graphic.y) * target.width + x + graphic.x) * 4;
    target.rgba.set([palette[index * 4]!, palette[index * 4 + 1]!, palette[index * 4 + 2]!, 255], to);
  }
}
const gb2312 = new Map<string, number>();
const decoder = new TextDecoder('gbk');
for (let high = 0xa1; high <= 0xf7; high++) for (let low = 0xa1; low <= 0xfe; low++) {
  const ch = decoder.decode(Uint8Array.of(high, low));
  if (ch !== '\ufffd') gb2312.set(ch, ((high - 0xa1) * 94 + low - 0xa1) * 32);
}
function label(target: Pixels, value: string, x: number, y: number): void {
  const ascii = bytes('install/font/ascfont.16'), chinese = bytes('install/font/stdfont.16');
  const palette = metadata.gdiObservation.targetPalette, color = palette.slice(31 * 4, 31 * 4 + 4);
  for (const ch of value) {
    const wide = ch.codePointAt(0)! >= 128, font = wide ? chinese : ascii;
    const start = wide ? gb2312.get(ch) : ch.codePointAt(0)! * 16;
    if (start === undefined) throw new Error(`Missing original font glyph ${ch}`);
    for (let gy = 0; gy < 16; gy++) for (let gx = 0; gx < (wide ? 16 : 8); gx++) {
      if (font[start + gy * (wide ? 2 : 1) + (gx >> 3)]! & (128 >> (gx & 7))) target.rgba.set(color, ((y + gy) * target.width + x + gx) * 4);
    }
    x += wide ? 16 : 8;
  }
}
function expected(name: string, overrides: Record<string, number> = {}, blood = 'A', zodiacIndex = 9, month = 1, day = 1): Pixels {
  const image: Pixels = { width: 640, height: 480, rgba: new Uint8Array(640 * 480 * 4) };
  draw(image, metadata.background);
  for (const button of metadata.buttons) draw(image, button, overrides[button.id] ?? (button.id === 'male' ? 2 : 0));
  draw(image, metadata.zodiac[zodiacIndex]!);
  draw(image, metadata.blood.find(row => row.bloodType === blood)!);
  label(image, String(month), 296, 135); label(image, String(day), 296, 184);
  const byteLength = [...name].reduce((sum, ch) => sum + (ch.codePointAt(0)! < 128 ? 1 : 2), 0);
  label(image, name, 92 + (11 - byteLength) * 4, 128);
  return image;
}
function region(image: Pixels, x: number, y: number, width: number, height: number): Pixels {
  const rgba = new Uint8Array(width * height * 4);
  for (let row = 0; row < height; row++) rgba.set(image.rgba.subarray(((y + row) * image.width + x) * 4, ((y + row) * image.width + x + width) * 4), row * width * 4);
  return { width, height, rgba };
}
async function frames(page: Page): Promise<void> {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
async function canvas(page: Page): Promise<Pixels> { await frames(page); return png(await page.locator('#game-canvas').screenshot()); }
async function creator(page: Page): Promise<void> { await control(page, '开始新游戏'); await freeze(page); await ready(page); await page.mouse.move(5, 5); await frames(page); }

test('original creator BMP palette, button states, zodiac, blood image and bitmap font match source pixels', async ({ page }, info) => {
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page); await creator(page);
  expect(metadata.buttons).toHaveLength(9); expect(metadata.zodiac).toHaveLength(12); expect(metadata.blood).toHaveLength(4);
  const reference = expected(metadata.defaultName), first = await canvas(page);
  // Exclude the genuinely animated actor and two animated confirm/cancel
  // objects. All remaining sampled areas use an independent indexed oracle.
  for (const [x, y, width, height] of [[0, 0, 640, 90], [220, 90, 420, 192], [139, 91, 82, 26], [92, 128, 88, 16], [280, 282, 250, 98], [0, 380, 470, 100]]) {
    expect(differingPixels(region(first, x!, y!, width!, height!), region(reference, x!, y!, width!, height!)), `original creator rectangle ${x},${y}`).toBe(0);
  }
  for (const [pressed, frame] of [[false, 1], [true, 2]] as const) {
    await page.mouse.move(150, 100); if (pressed) await page.mouse.down();
    const actual = await canvas(page), hovered = expected(metadata.defaultName, { name: frame });
    expect(differingPixels(region(actual, 139, 91, 82, 26), region(hovered, 139, 91, 82, 26))).toBe(0);
  }
  await page.mouse.up(); await frames(page);
  const input = page.getByRole('textbox', { name: '角色姓名' });
  await expect(input).toHaveValue(metadata.defaultName); await input.fill('原版回归');
  await page.mouse.move(5, 5); await frames(page);
  expect(differingPixels(region(await canvas(page), 92, 128, 88, 16), region(expected('原版回归'), 92, 128, 88, 16))).toBe(0);
  await page.locator('#game-canvas').screenshot({ path: info.outputPath('original-creator-bmp-font-and-controls.png') });
  expect(errors).toEqual([]);
});

test('original creator calendar wraps, keeps February 31, cycles A AB B O, accepts IME and resets on reentry', async ({ page }, info) => {
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page); await creator(page);
  await control(page, '上一日期'); // January 1 -> January 31.
  await control(page, '下一月份'); // Original month handler leaves day 31.
  await control(page, '下一血型'); await control(page, '下一血型'); // A -> AB -> B.
  await control(page, '女性');
  await control(page, '姓名'); await page.getByRole('textbox', { name: '角色姓名' }).fill('原版回归');
  await page.mouse.move(5, 5); await frames(page);
  const reference = expected('原版回归', { male: 0, female: 2 }, 'B', 11, 2, 31), image = await canvas(page);
  for (const [x, y, width, height] of [[220, 90, 420, 192], [280, 282, 250, 98], [120, 391, 41, 42]]) {
    expect(differingPixels(region(image, x!, y!, width!, height!), region(reference, x!, y!, width!, height!))).toBe(0);
  }
  await page.locator('#game-canvas').screenshot({ path: info.outputPath('original-creator-female-february31-bloodB.png') });
  await control(page, '确定'); await ready(page);
  expect((await diagnostic(page)).screen).toBe('Game');
  expect((await diagnostic(page)).snapshot.player).toMatchObject({ name: '原版回归', sex: 'female', birthMonth: 2, birthDay: 31, bloodType: 'B' });
  await page.keyboard.press('F10'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Menu');
  await creator(page); const reset = await canvas(page), defaults = expected(metadata.defaultName);
  for (const [x, y, width, height] of [[220, 90, 420, 192], [280, 282, 250, 98], [120, 391, 41, 42], [92, 128, 88, 16]]) {
    expect(differingPixels(region(reset, x!, y!, width!, height!), region(defaults, x!, y!, width!, height!))).toBe(0);
  }
  expect(errors).toEqual([]);
});
