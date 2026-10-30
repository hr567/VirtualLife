// Winning machine-code evidence re-encoded as a fresh save. This exercises the
// public ending UI; it is separate from the full-life probes' defeat routes.
import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { control, diagnostic, freeze, importScenario, open, ready, scenario } from './helpers';
import { bmp, blit, differingPixels, png, type Pixels } from './images';

interface Credits {
  clockMs: number; offset: number; speed: number; brightness: number;
  fadingIn: boolean; fadingOut: boolean; sPressed: boolean; sReleased: boolean;
}
interface EndingSnapshot {
  phase: string; life: { incarnation: number };
  pending: { kind: string; state: { phase: string; victory: boolean; credits?: Credits } } | null;
}
async function snapshot(page: Page): Promise<EndingSnapshot> {
  return (await diagnostic(page)).snapshot as unknown as EndingSnapshot;
}
async function phase(page: Page): Promise<string | undefined> {
  return (await snapshot(page)).pending?.state.phase;
}
async function frame(page: Page, milliseconds: number): Promise<void> {
  await ready(page);
  await page.evaluate(frameMs => window.__virtualLifeBevy!.input!({ frameMs }), milliseconds);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await ready(page);
}
async function credits(page: Page): Promise<Credits> {
  const value = (await snapshot(page)).pending?.state.credits;
  expect(value, 'The real ending must have reached its credits state').toBeDefined();
  return value!;
}
function endImage(): Pixels {
  const manifest = JSON.parse(readFileSync('assets/manifest.json', 'utf8')) as {
    resources: Record<string, { sha256: string }>; objects: { sha256: string; path: string }[];
  };
  const hash = manifest.resources['cd/data/proepi/end.bmp']!.sha256;
  const object = manifest.objects.find(object => object.sha256 === hash);
  expect(object).toBeDefined();
  const image = bmp(readFileSync(object!.path));
  expect([image.width, image.height]).toEqual([200, 70]);
  const expected: Pixels = { width: 640, height: 480, rgba: new Uint8Array(640 * 480 * 4) };
  for (let alpha = 3; alpha < expected.rgba.length; alpha += 4) expected.rgba[alpha] = 255;
  blit(expected, image, (640 - image.width) / 2, (480 - image.height) / 2);
  return expected;
}

test('winning ending survives credits save and reaches original end image and reincarnation through real inputs', async ({ page }, info) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 640, height: 480 });
  const fixture = JSON.parse(readFileSync(scenario('ending-win'), 'utf8')) as { format: string; version: number; payload: string };
  expect(fixture).toMatchObject({ format: 'virtual-life-bevy-save', version: 1 });
  expect(JSON.parse(fixture.payload)).toMatchObject({ format: 'virtual-life-bevy-save', version: 1 });
  const errors = await open(page); await freeze(page); await importScenario(page, 'ending-win');
  const initial = await snapshot(page);
  expect(initial.pending).toMatchObject({ kind: 'ending', state: { phase: 'summary', victory: true } });
  await frame(page, 1800);
  // The original applies its first post-fade palette on the next >150ms flash.
  await frame(page, 151);
  const summary = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('winning-summary.png') }));
  expect(summary.rgba.some((value, index) => index % 4 !== 3 && value > 0), 'The restored original summary background must be visible').toBe(true);
  await page.keyboard.press('Escape'); await frame(page, 1); await frame(page, 3000); await frame(page, 1501);
  await expect.poll(() => phase(page)).toBe('movie');
  await page.screenshot({ path: info.outputPath('winning-movie.png') });
  await page.keyboard.press('Escape'); await frame(page, 1);
  await expect.poll(() => phase(page)).toBe('credits');
  await frame(page, 2500);
  expect(await credits(page)).toMatchObject({ fadingIn: false, fadingOut: false, brightness: 224, speed: -1 });
  const beforeSpeed = await credits(page);
  await page.keyboard.down('s');
  await expect.poll(async () => (await credits(page)).sPressed).toBe(true);
  await frame(page, 51);
  expect(await credits(page)).toMatchObject({ speed: -40, offset: beforeSpeed.offset - 40 });
  const accelerated = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('winning-credits-first-title.png') }));
  let titlePixels = 0;
  const titleY = 60 + (await credits(page)).offset;
  for (let y = titleY; y < Math.min(titleY + 23, 420); y++) for (let x = 200; x < 440; x++) {
    const at = (y * accelerated.width + x) * 4;
    if (accelerated.rgba[at]! + accelerated.rgba[at + 1]! + accelerated.rgba[at + 2]! > 0) titlePixels++;
  }
  expect(titlePixels, 'The original VIRTUAL LIFE title must be visible after S accelerates the list').toBeGreaterThan(0);
  await page.keyboard.up('s');
  await expect.poll(async () => (await credits(page)).sReleased).toBe(true);
  await frame(page, 1); expect((await credits(page)).speed).toBe(-1);

  const saved = (await diagnostic(page)).snapshot;
  await page.keyboard.press('F6'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  await frame(page, 1000); expect((await diagnostic(page)).snapshot).toEqual(saved);
  await control(page, '保存当前进度'); await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
  await page.reload(); await ready(page); await freeze(page); await control(page, '继续游戏'); await ready(page);
  expect((await diagnostic(page)).snapshot).toEqual(saved);
  await page.screenshot({ path: info.outputPath('winning-credits-restored.png') });

  await page.keyboard.press('Escape'); await frame(page, 1);
  expect(await credits(page)).toMatchObject({ fadingOut: true, brightness: 224 });
  await frame(page, 2500); expect(await credits(page)).toMatchObject({ fadingOut: true, brightness: 0 });
  await frame(page, 1); await expect.poll(() => phase(page)).toBe('endImage');
  const actual = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('original-centered-end-bmp.png') }));
  expect(differingPixels(actual, endImage()), 'The original 200×70 end BMP must be centered on the black 640×480 frame').toBe(0);
  await page.keyboard.press('Enter'); await expect.poll(() => phase(page)).toBe('reincarnation'); await ready(page);
  await page.mouse.move(5, 5); await page.screenshot({ path: info.outputPath('original-reincarnation-choice.png') });
  await control(page, '再次轮回');
  await expect.poll(async () => (await snapshot(page)).life.incarnation).toBe(initial.life.incarnation + 1);
  expect(await snapshot(page)).toMatchObject({ phase: 'playing', pending: null });
  expect(errors).toEqual([]);
});

test('ending the winning life returns to the menu and a completed save cannot resume a stranded game', async ({ page }, info) => {
  test.setTimeout(180_000);
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page); await freeze(page); await importScenario(page, 'ending-win');
  const initial = await snapshot(page);
  expect(initial.pending).toMatchObject({ kind: 'ending', state: { phase: 'summary', victory: true } });
  await frame(page, 1800); await frame(page, 151);
  await page.keyboard.press('Escape'); await frame(page, 1); await frame(page, 3000); await frame(page, 1501);
  await expect.poll(() => phase(page)).toBe('movie');
  await page.keyboard.press('Escape'); await frame(page, 1);
  await expect.poll(() => phase(page)).toBe('credits');
  await frame(page, 2500);
  expect(await credits(page)).toMatchObject({ fadingIn: false, fadingOut: false, brightness: 224 });
  await page.keyboard.press('Escape'); await frame(page, 1);
  expect((await credits(page)).fadingOut).toBe(true);
  await frame(page, 2500); await frame(page, 1);
  await expect.poll(() => phase(page)).toBe('endImage');
  const actual = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('completed-route-end-bmp.png') }));
  expect(differingPixels(actual, endImage())).toBe(0);
  await page.keyboard.press('Enter'); await expect.poll(() => phase(page)).toBe('reincarnation'); await ready(page);
  await page.screenshot({ path: info.outputPath('ending-no-choice.png') });
  await control(page, '结束人生');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Menu');
  expect(await snapshot(page)).toMatchObject({ phase: 'completed', pending: null, life: { incarnation: initial.life.incarnation } });
  const completed = (await diagnostic(page)).snapshot;
  await expect.poll(() => page.evaluate(() => new Promise<string | undefined>((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 1);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, reading = db.transaction('saves').objectStore('saves').get('current');
      reading.onerror = () => reject(reading.error);
      reading.onsuccess = () => {
        db.close();
        const record = reading.result as { payload: string } | undefined;
        resolve(record ? JSON.parse(record.payload).state.phase as string : undefined);
      };
    };
  }))).toBe('completed');
  await page.screenshot({ path: info.outputPath('completed-life-menu.png') });
  await page.reload(); await ready(page); await freeze(page);
  await control(page, '继续游戏'); await ready(page);
  await expect.poll(async () => (await diagnostic(page)).snapshot.phase).toBe('completed');
  expect((await diagnostic(page)).screen).toBe('Menu');
  expect((await diagnostic(page)).snapshot).toEqual(completed);
  await frame(page, 1000);
  expect((await diagnostic(page)).screen).toBe('Menu');
  expect((await diagnostic(page)).snapshot).toEqual(completed);
  expect((await diagnostic(page)).controls.some(control => control.label === '开始新游戏' && control.enabled)).toBe(true);
  await page.screenshot({ path: info.outputPath('completed-save-continue-menu.png') });
  expect(errors).toEqual([]);
});
