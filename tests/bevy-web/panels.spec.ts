// Real pointer/keyboard input; frame-time input only controls the original clock.
import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { control, diagnostic, freeze, importScenario, open, ready, scenario } from './helpers';

interface RelationshipSnapshot {
  lastSeq: number;
  pending: { kind: string; state: { phase: string; clockMs: number; questionIndex: number; hoveredOffer: number | null } };
}
async function frames(page: Page): Promise<void> {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
async function move(page: Page, x: number, y: number): Promise<void> {
  const canvas = await page.locator('#game-canvas').boundingBox();
  expect(canvas).not.toBeNull();
  // Invert Bevy's uniform letterboxed transform. Independent width/height
  // scaling misses 1px slider controls on a non-4:3 window (258 became 257).
  const scale = Math.min(canvas!.width / 640, canvas!.height / 480);
  const left = (canvas!.width - 640 * scale) / 2, top = (canvas!.height - 480 * scale) / 2;
  await page.mouse.move(canvas!.x + left + (Math.floor(x) + 0.5) * scale, canvas!.y + top + (Math.floor(y) + 0.5) * scale);
  await frames(page);
}
async function stored(page: Page, store: 'settings' | 'saves', key: string): Promise<unknown> {
  return page.evaluate(({ store, key }) => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 1);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, reading = db.transaction(store).objectStore(store).get(key);
      reading.onsuccess = () => { db.close(); resolve(reading.result); };
      reading.onerror = () => reject(reading.error);
    };
  }), { store, key });
}
async function openVolume(page: Page): Promise<void> {
  await control(page, '游戏选项');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Options');
  await control(page, '音量设置');
  await expect.poll(async () => (await diagnostic(page)).controls.filter(control => control.enabled && control.id.startsWith('volume-')).length).toBe(155);
  // The volume page introduces its background, track and thumb after release.
  // A fixed number of RAFs does not acknowledge their foreground asset loads.
  await ready(page);
}
async function relationship(page: Page): Promise<RelationshipSnapshot> {
  return (await diagnostic(page)).snapshot as unknown as RelationshipSnapshot;
}
async function enterQuestion(page: Page): Promise<void> {
  await importScenario(page, 'relationship');
  const beforeMeet = await relationship(page);
  await control(page, '结识新朋友');
  await expect.poll(async () => {
    const state = await relationship(page);
    return { sequence: state.lastSeq, phase: state.pending.state.phase };
  }).toEqual({ sequence: beforeMeet.lastSeq + 1, phase: 'meet' });
  // Meet introduces portraits and the board. Wait for their foreground loads
  // before supplying a single clock frame: loading consumes manual time as 0.
  await ready(page);
  const candidate = (await diagnostic(page)).controls.find(control => control.id === 'relationship-offer-0' && control.enabled);
  expect(candidate).toBeDefined();
  await move(page, candidate!.bounds.x + candidate!.bounds.width / 2, candidate!.bounds.y + candidate!.bounds.height / 2);
  await ready(page);
  const beforeHover = await relationship(page);
  // Original introductions consume their RNG when hovered in a visible frame.
  await page.evaluate(() => window.__virtualLifeBevy!.input!({ frameMs: 1 }));
  await expect.poll(async () => (await relationship(page)).lastSeq).toBe(beforeHover.lastSeq + 1);
  expect((await relationship(page)).pending.state).toMatchObject({ hoveredOffer: 0, clockMs: beforeHover.pending.state.clockMs + 1 });
  await control(page, '选择第 1 位人物');
  await expect.poll(async () => (await relationship(page)).pending.state.phase).toBe('question');
  expect((await relationship(page)).pending.state.questionIndex).toBe(0);
}

test('original system slider follows held pointer and persists its exact native travel', async ({ page }, info) => {
  test.setTimeout(180_000);
  const errors = await open(page); await freeze(page);
  const domain = (await diagnostic(page)).snapshot;
  await openVolume(page);
  await move(page, 258, 230); await page.mouse.down(); await frames(page);
  await expect.poll(() => stored(page, 'settings', 'volume')).toBe(17);
  await move(page, 343, 230);
  await expect.poll(() => stored(page, 'settings', 'volume')).toBe(75);
  await move(page, 343, 100);
  expect(await stored(page, 'settings', 'volume')).toBe(75);
  await move(page, 378, 230);
  await expect.poll(() => stored(page, 'settings', 'volume')).toBe(100);
  await page.mouse.up(); await frames(page);
  expect(await stored(page, 'settings', 'volume')).toBe(100);
  expect((await diagnostic(page)).snapshot).toEqual(domain);
  await page.screenshot({ path: info.outputPath('original-volume-slider.png') });
  await page.reload(); await ready(page); await freeze(page);
  expect(await stored(page, 'settings', 'volume')).toBe(100);
  await openVolume(page);
  await page.screenshot({ path: info.outputPath('original-volume-restored.png') });
  expect(errors).toEqual([]);
});

test('relationship answers keep click selection through save and accept original A B C keys', async ({ page }, info) => {
  test.setTimeout(180_000);
  const fixture = JSON.parse(readFileSync(scenario('relationship'), 'utf8')) as { payload: string };
  const initialRngDraws = JSON.parse(fixture.payload).state.rng.draws as number;
  const errors = await open(page); await freeze(page); await enterQuestion(page);
  const first = await relationship(page);
  const choice = (await diagnostic(page)).controls.find(control => control.id === 'relationship-answer-1');
  expect(choice).toBeDefined();
  await control(page, choice!.label);
  expect(await relationship(page)).toEqual(first);
  await page.keyboard.press('F6'); await control(page, '保存当前进度');
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
  const record = await stored(page, 'saves', 'current') as { payload: string };
  expect(JSON.parse(record.payload).presentation.profile._relationshipAnswer).toBe(1);
  // One hovered frame and selectOffer each draw two original answer positions.
  expect(JSON.parse(record.payload).state.rng.draws).toBe(initialRngDraws + 4);
  await page.reload(); await ready(page); await freeze(page); await control(page, '继续游戏'); await ready(page);
  expect(await relationship(page)).toEqual(first);
  await page.screenshot({ path: info.outputPath('relationship-selection-restored.png') });
  const selected = (await diagnostic(page)).controls.find(control => control.id === 'relationship-answer-1');
  await control(page, selected!.label);
  await expect.poll(async () => (await relationship(page)).pending.state.questionIndex).toBe(1);
  expect((await relationship(page)).lastSeq).toBe(first.lastSeq + 1);
  for (const key of ['a', 'b', 'c']) {
    await enterQuestion(page);
    const before = await relationship(page);
    await page.keyboard.press(key);
    await expect.poll(async () => (await relationship(page)).pending.state.questionIndex).toBe(1);
    expect((await relationship(page)).lastSeq).toBe(before.lastSeq + 1);
  }
  expect(errors).toEqual([]);
});
