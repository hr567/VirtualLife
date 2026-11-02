import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { control, diagnostic, freeze, importScenario, open, ready, scenario, saveSelected } from './helpers';

async function savedAt(page: Page): Promise<string | null> {
  return page.evaluate(() => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 2);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, transaction = db.transaction('saves');
      const request = transaction.objectStore('saves').get('slot-1');
      request.onsuccess = () => resolve(request.result?.savedAt ?? null);
      request.onerror = () => reject(request.error);
      transaction.oncomplete = () => db.close();
    };
  }));
}

for (const mapId of ['acient', 'business', 'desert', 'lost', 'park', 'play', 'social', 'study']) {
  test(`public-route ${mapId} map renders and its idle state survives F6 save and reload`, async ({ page }, info) => {
    const fixture = JSON.parse(readFileSync(scenario(`map-${mapId}`), 'utf8'));
    const expected = JSON.parse(fixture.payload).state;
    expect(expected).toMatchObject({ mapId, phase: 'playing', pending: null });
    expect(expected.player).not.toBeNull();
    const errors = await open(page); await freeze(page); await importScenario(page, `map-${mapId}`);
    const state = await diagnostic(page);
    expect(state.screen).toBe('Game');
    expect(state.snapshot).toMatchObject({ mapId, phase: 'playing', pending: null, player: expected.player, tileId: expected.tileId });
    expect(state.controls.some(control => control.enabled)).toBe(true);
    // These are evidence from the Bevy client, not original-game golden images.
    await page.locator('#game-canvas').screenshot({ path: info.outputPath(`bevy-map-${mapId}.png`) });
    await expect.poll(() => savedAt(page)).not.toBeNull();
    const before = await savedAt(page);
    await page.keyboard.press('F6');
    await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
    await saveSelected(page);
    await expect.poll(() => savedAt(page)).not.toBe(before);
    await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
    await page.reload(); await ready(page); await freeze(page);
    expect((await diagnostic(page)).screen).toBe('Menu');
    await control(page, '继续游戏'); await control(page, '读取选中槽位'); await ready(page);
    expect((await diagnostic(page)).snapshot).toEqual(state.snapshot);
    expect(errors).toEqual([]);
  });
}
