import { test, expect, type Page } from '@playwright/test';
import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const hash = (text: string): string => createHash('sha256').update(text).digest('hex');
const profile = { name: '测试', sex: 'male', birthMonth: 1, birthDay: 1, bloodType: 'A' };
let runtime: Promise<{ Engine: typeof import('../../src/wasm/virtual_life_core.js').Engine; content: string }> | undefined;
function testRuntime() {
  return runtime ??= (async () => {
    const { initSync, Engine } = await import('../../src/wasm/virtual_life_core.js');
    const wasm = (await readdir('dist/assets')).find(name => /^virtual_life_core_bg-.*\.wasm$/.test(name));
    if (!wasm) throw new Error('Build the game before running browser tests.');
    initSync({ module: await readFile(`dist/assets/${wasm}`) });
    const catalog = async (name: string) => JSON.parse(await readFile(`dist/assets/catalog/${name}.json`, 'utf8'));
    return { Engine, content: JSON.stringify({ ...await catalog('maps'), rules: await catalog('rules'), events: await catalog('events'), questionnaires: await catalog('questionnaires') }) };
  })();
}

// Locate an actual first-turn route through the public API. Core parity tests own fixed RNG vectors.
const firstSeeds = new Map<string, number>();
async function firstTurnSeed(route: string): Promise<number> {
  const existing = firstSeeds.get(route); if (existing !== undefined) return existing;
  const { Engine, content } = await testRuntime();
  for (let seed = 1; seed <= 2000; seed++) {
    const engine = new Engine(content);
    engine.dispatch(JSON.stringify({ seq: 1, type: 'newGame', profile, seed }));
    const result = JSON.parse(engine.dispatch(JSON.stringify({ seq: 2, type: 'rollDice' }))); engine.free();
    const pending = result.snapshot.pending;
    const found = pending?.game ?? pending?.text;
    if (found && !firstSeeds.has(found)) firstSeeds.set(found, seed);
    if (found === route) return seed;
  }
  throw new Error(`No first-turn route found: ${route}`);
}

test('native browser fixtures restore unchanged in the built WASM engine', async () => {
  const { Engine, content } = await testRuntime();
  const fixtures = (await readdir('tests/fixtures')).filter(name => /^browser-.*\.json$/.test(name)).sort();
  expect(fixtures).toHaveLength(16);
  for (const fixture of fixtures) {
    const engine = new Engine(content);
    try { expect(JSON.parse(engine.restore(await readFile(`tests/fixtures/${fixture}`, 'utf8'))), fixture).toMatchObject({ ok: true }); }
    finally { engine.free(); }
  }
});

async function importFixture(page: Page, name: string): Promise<void> {
  const payload = await readFile(`tests/fixtures/browser-${name}.json`, 'utf8');
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.goto('./'); await expect(page.locator('#original-menu')).toBeVisible();
  await page.locator('#import-file').setInputFiles({ name: `${name}.vlsave.json`, mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
}

async function storedSaveSlots(page: Page): Promise<{ current?: string; previous?: string }> {
  return page.evaluate(() => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life', 1);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, transaction = db.transaction('saves'), store = transaction.objectStore('saves');
      const current = store.get('current'), previous = store.get('previous');
      transaction.oncomplete = () => { resolve({ current: current.result?.payload, previous: previous.result?.payload }); db.close(); };
      transaction.onerror = () => { reject(transaction.error); db.close(); };
    };
  }));
}

async function seedGame(page: Page, seed: number): Promise<void> {
  await page.addInitScript(seed => {
    const random = crypto.getRandomValues.bind(crypto);
    crypto.getRandomValues = ((values: ArrayBufferView) => {
      if (values instanceof Uint32Array && values.length === 1) { values[0] = seed; return values; }
      return random(values);
    }) as typeof crypto.getRandomValues;
  }, seed);
}

test('original menu, character creation and recovered event complete a playable turn', async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await seedGame(page, await firstTurnSeed('趁父母外出独自到地下室玩'));
  await page.goto('./');
  await expect(page).toHaveTitle('虚拟人生');
  await expect(page.locator('#original-menu')).toBeVisible();
  await expect(page.locator('#fatal')).toBeHidden();
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('original-menu.png') });
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await page.getByLabel('姓名').fill('测试人物');
  await page.getByRole('button', { name: '确定', exact: true }).click();
  const roll = page.getByRole('button', { name: '掷骰子', exact: true });
  await expect(roll).toBeEnabled();
  await expect(page.locator('#logical-ui')).toContainText('测试人物');
  await page.screenshot({ path: testInfo.outputPath('new-game.png') });
  await roll.click();
  await expect(page.locator('.event-text')).toHaveText('趁父母外出独自到地下室玩');
  await expect(roll).toBeHidden();
  await page.screenshot({ path: testInfo.outputPath('original-event.png') });
  for (let step = 0; step < 8; step++) {
    const previous = await page.locator('.event-text').textContent();
    await page.getByRole('button', { name: '继续对话', exact: true }).click();
    if (step < 7) await expect(page.locator('.event-text')).not.toHaveText(previous!);
  }
  await expect(roll).toBeEnabled();
  await expect(page.locator('#status')).toContainText('可以继续掷骰子');
  await expect(page.locator('#fatal')).toBeHidden();
  expect(errors).toEqual([]);
});

test('tracking minigame animates, restores and finishes three rounds', async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await seedGame(page, await firstTurnSeed('tracking'));
  await page.goto('./');
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await page.getByLabel('姓名').fill('追踪测试');
  await page.getByRole('button', { name: '确定', exact: true }).click();
  await page.getByRole('button', { name: '掷骰子', exact: true }).click();
  await expect(page.locator('.tracking-status')).toContainText('第 1 / 3 轮');
  await page.screenshot({ path: testInfo.outputPath('tracking-reveal.png') });
  await expect(page.getByRole('button', { name: '开始追踪', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await expect(page.locator('#status')).toContainText('进度已保存');
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '开始追踪', exact: true })).toBeVisible();
  for (let round = 1; round <= 3; round++) {
    await page.getByRole('button', { name: '开始追踪', exact: true }).click();
    const choice = page.getByRole('button', { name: '选择第 1 个', exact: true });
    await expect(choice).toBeVisible();
    if (round === 1) await page.screenshot({ path: testInfo.outputPath('tracking-choice.png') });
    await choice.click();
    await page.getByRole('button', { name: round < 3 ? '下一轮' : '返回人生', exact: true }).click();
    await expect(page.locator('#fatal')).toBeHidden();
    if (round < 3) await expect(page.getByRole('button', { name: '开始追踪', exact: true })).toBeVisible();
  }
  await expect(page.locator('.tracking-status')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('hammer minigame starts, restores its timer and returns after the deadline', async ({ page }, testInfo) => {
  test.setTimeout(100_000);
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await seedGame(page, await firstTurnSeed('hammer'));
  await page.goto('./');
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await page.getByLabel('姓名').fill('打地鼠测试');
  await page.getByRole('button', { name: '确定', exact: true }).click();
  await page.getByRole('button', { name: '掷骰子', exact: true }).click();
  await expect(page.getByRole('button', { name: '开始打地鼠', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('hammer-help.png') });
  await page.getByRole('button', { name: '开始打地鼠', exact: true }).click();
  await expect(page.locator('.hammer-status')).toContainText('剩余 45 秒');
  await page.getByRole('button', { name: '击打第 1 个洞', exact: true }).focus();
  await page.keyboard.press('1');
  await expect(page.locator('.hammer-status')).toContainText('剩余 43 秒');
  await page.screenshot({ path: testInfo.outputPath('hammer-playing.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  const beforeSave = await page.locator('.hammer-status').textContent();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await expect(page.locator('#status')).toContainText('进度已保存');
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.hammer-status')).toHaveText(beforeSave!);
  await expect(page.locator('.hammer-active')).toHaveCount(0, { timeout: 55_000 });
  await expect(page.locator('#status')).not.toContainText('小游戏暂停');
  await expect(page.locator('#fatal')).toBeHidden();
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  expect(errors).toEqual([]);
});

test('earned hammer reward renders original counters and completes after import', async ({ page }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  const { Engine, content } = await testRuntime();
  const engine = new Engine(content);
  let sequence = 0;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  send({ type: 'newGame', profile: { name: '奖励测试', sex: 'male', birthMonth: 1, birthDay: 1, bloodType: 'A' }, seed: await firstTurnSeed('hammer') });
  send({ type: 'rollDice' });
  send({ type: 'minigameTick', elapsedMs: 2000 });
  let result = send({ type: 'minigameAction', action: 'start' });
  // Build a real winning save with the public command API; no state or checksum is patched.
  for (let tick = 0; tick < 3000 && result.snapshot.pending?.state?.phase === 'playing'; tick++) {
    result = send({ type: 'minigameTick', elapsedMs: 16 });
    for (const [index, target] of (result.snapshot.pending?.state?.targets ?? []).entries()) {
      if (result.snapshot.pending.state.phase === 'playing' && target.phase === 2 && target.kind !== 2) result = send({ type: 'minigameAction', action: 'strikeIndex', index });
    }
  }
  expect(result.snapshot.pending.state.phase).toBe('won');
  expect(result.snapshot.pending.state.score).toBeGreaterThanOrEqual(80);
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.locator('#import-file').setInputFiles({ name: 'earned-reward.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.hammer-status')).toContainText('道德');
  await page.screenshot({ path: testInfo.outputPath('hammer-reward.png') });
  await expect(page.locator('.hammer-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden();
  await expect(page.locator('#status')).not.toContainText('小游戏暂停');
  expect(errors).toEqual([]);
});

test('fist minigame accepts held input, pauses for help and restores that phase', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await seedGame(page, await firstTurnSeed('fist'));
  await page.goto('./');
  await page.getByRole('button', { name: '开始新游戏', exact: true }).click();
  await page.getByLabel('姓名').fill('拳击测试');
  await page.getByRole('button', { name: '确定', exact: true }).click();
  await page.getByRole('button', { name: '掷骰子', exact: true }).click();
  await expect(page.getByRole('button', { name: '开始拳击', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('fist-help.png') });
  await page.getByRole('button', { name: '开始拳击', exact: true }).click();
  await expect(page.locator('.fist-status')).toContainText('剩余 60 秒');
  await page.locator('#game-canvas').focus();
  await page.keyboard.down('ArrowRight'); await page.waitForTimeout(180); await page.keyboard.up('ArrowRight');
  await page.keyboard.press('Space');
  await expect(page.locator('.fist-status')).toContainText('剩余 59 秒');
  await page.screenshot({ path: testInfo.outputPath('fist-playing.png') });
  await page.keyboard.press('F1');
  await expect(page.getByRole('button', { name: '继续拳击', exact: true })).toBeVisible();
  const paused = await page.locator('.fist-status').textContent();
  await page.waitForTimeout(1200); await expect(page.locator('.fist-status')).toHaveText(paused!);
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '继续拳击', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '继续拳击', exact: true }).click();
  await expect(page.getByRole('button', { name: '出拳', exact: true })).toBeVisible();
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('relationship house renders offers and questionnaire and restores an open question', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'relationship');
  await expect(page.getByRole('button', { name: '结识新朋友', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('relationship-main.png') });
  await page.getByRole('button', { name: '结识新朋友', exact: true }).click();
  await page.getByRole('button', { name: '选择第 1 位人物', exact: true }).hover();
  await expect(page.locator('.relationship-text')).not.toBeEmpty();
  await page.screenshot({ path: testInfo.outputPath('relationship-offers.png') });
  await page.getByRole('button', { name: '选择第 1 位人物', exact: true }).click();
  await expect(page.locator('.relationship-answer')).toHaveCount(3);
  const question = await page.locator('.relationship-text').textContent();
  await page.screenshot({ path: testInfo.outputPath('relationship-question.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.relationship-text')).toHaveText(question!);
  await page.locator('.relationship-answer').first().click();
  await expect(page.locator('.relationship-text')).not.toHaveText(question!);
  await page.locator('.relationship-answer').first().click();
  await expect(page.locator('.relationship-answer')).toHaveCount(0);
  await page.locator('#game-canvas').focus(); await page.keyboard.press('Escape');
  await expect(page.locator('.relationship-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('spirit lottery stops its original counters and returns to the landing event', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'spirit-slots');
  await expect(page.getByRole('button', { name: '停止神灵转盘', exact: true })).toBeEnabled();
  await page.screenshot({ path: testInfo.outputPath('spirit-spinning.png') });
  await page.getByRole('button', { name: '停止神灵转盘', exact: true }).click();
  await expect(page.locator('.spirit-result')).not.toBeEmpty();
  await page.screenshot({ path: testInfo.outputPath('spirit-result.png') });
  await expect(page.locator('.spirit-slots-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('gas timing game plays a real attempt and restores its help state', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'gas');
  await expect(page.getByRole('button', { name: '开始狗熊捕鱼', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('gas-help.png') });
  await page.getByRole('button', { name: '开始狗熊捕鱼', exact: true }).click();
  await page.getByRole('button', { name: '捕鱼', exact: true }).click();
  await expect(page.locator('.gas-status')).toContainText('已尝试 1 / 3 次');
  await page.screenshot({ path: testInfo.outputPath('gas-playing.png') });
  await page.locator('#game-canvas').focus(); await page.keyboard.press('F1');
  await expect(page.getByRole('button', { name: '继续狗熊捕鱼', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '继续狗熊捕鱼', exact: true })).toBeVisible();
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('controlled dice displays original eight buttons and accepts a chosen value', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'controlled-dice');
  await page.getByRole('button', { name: '掷骰子', exact: true }).click();
  await expect(page.getByRole('button', { name: '选择 8 点', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('controlled-dice.png') });
  await page.getByRole('button', { name: '选择 1 点', exact: true }).click();
  await expect(page.getByRole('button', { name: '选择 8 点', exact: true })).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('firebee shooter moves, fires, displays help and restores the paused screen', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'firebee');
  await expect(page.getByRole('button', { name: '开始射击', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('firebee-help.png') });
  await page.getByRole('button', { name: '开始射击', exact: true }).click();
  await page.locator('#game-canvas').focus();
  await page.keyboard.down('ArrowLeft'); await page.waitForTimeout(200); await page.keyboard.up('ArrowLeft');
  await page.keyboard.press('Space');
  await expect(page.locator('.firebee-status')).toContainText('得分');
  await page.screenshot({ path: testInfo.outputPath('firebee-playing.png') });
  await page.keyboard.press('F1');
  await expect(page.getByRole('button', { name: '继续射击', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '继续射击', exact: true })).toBeVisible();
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('firebee earned score displays the native money reward and resumes play', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  const { Engine, content } = await testRuntime();
  const engine = new Engine(content);
  const fixture = (await readFile('tests/fixtures/browser-firebee.json', 'utf8'));
  expect(JSON.parse(engine.restore(fixture)).ok).toBe(true);
  let sequence = JSON.parse(fixture).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  let result = send({ type: 'minigameAction', action: 'start' });
  for (let frame = 0; frame < 6000 && result.snapshot.pending?.state?.phase === 'playing'; frame++) {
    if (frame % 3 === 0) send({ type: 'minigameAction', action: 'input', fire: true });
    result = send({ type: 'minigameTick', elapsedMs: 16 });
  }
  expect(result.snapshot.pending.state.phase).toBe('reward'); expect(result.snapshot.pending.state.score).toBeGreaterThan(0);
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.goto('./'); await expect(page.locator('#original-menu')).toBeVisible();
  await page.locator('#import-file').setInputFiles({ name: 'earned-firebee.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.firebee-status')).toContainText('金钱');
  await page.screenshot({ path: testInfo.outputPath('firebee-reward.png') });
  await expect(page.locator('.firebee-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('fire game accepts alternating hands, renders indexed artwork and restores progress', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'fire');
  await expect(page.getByRole('button', { name: '开始钻木取火', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('fire-help.png') });
  await page.getByRole('button', { name: '开始钻木取火', exact: true }).click();
  await page.locator('#game-canvas').focus();
  for (let index = 0; index < 8; index++) await page.keyboard.press(index % 2 ? 'ArrowRight' : 'ArrowLeft');
  await expect(page.locator('.fire-status')).toContainText('火力');
  await expect(page.locator('.fire-status')).not.toContainText('火力 0 /');
  await page.screenshot({ path: testInfo.outputPath('fire-playing.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  const paused = await page.locator('.fire-status').textContent();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.fire-status')).toContainText('火力');
  const restoredGauge = Number((await page.locator('.fire-status').textContent())!.match(/火力 (\d+)/)![1]);
  const savedGauge = Number(paused!.match(/火力 (\d+)/)![1]);
  expect(restoredGauge).toBeGreaterThanOrEqual(savedGauge - 20);
  expect(restoredGauge).toBeLessThanOrEqual(savedGauge);
  await expect(page.locator('#fatal')).toBeHidden();
  await expect(page.locator('#status')).not.toContainText('小游戏暂停');
  expect(errors).toEqual([]);
});

test('earned fire reward uses original palette animation and returns to the event', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  const { Engine, content } = await testRuntime();
  const engine = new Engine(content);
  const fixture = (await readFile('tests/fixtures/browser-fire.json', 'utf8'));
  engine.restore(fixture);
  let sequence = JSON.parse(fixture).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  let result = send({ type: 'minigameAction', action: 'start' });
  for (let index = 0; index < 120 && !result.snapshot.pending.state.winStage; index++) {
    send({ type: 'minigameAction', action: index % 2 ? 'right' : 'left' });
    result = send({ type: 'minigameTick', elapsedMs: 16 });
  }
  expect(result.ok).toBe(true); expect(result.snapshot.pending.state.winStage).toBeGreaterThan(0);
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.goto('./'); await expect(page.locator('#original-menu')).toBeVisible();
  await page.locator('#import-file').setInputFiles({ name: 'earned-fire.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.fire-status')).toContainText('火力 1000');
  await page.screenshot({ path: testInfo.outputPath('fire-winning.png') });
  await expect(page.locator('.fire-status')).toContainText('魅力');
  await page.screenshot({ path: testInfo.outputPath('fire-reward.png') });
  await expect(page.locator('.fire-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden();
  await expect(page.locator('#status')).not.toContainText('小游戏暂停'); expect(errors).toEqual([]);
});

test('fire timeout fades to the original loss sequence and resumes the event', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  const { Engine, content } = await testRuntime();
  const engine = new Engine(content);
  const fixture = (await readFile('tests/fixtures/browser-fire.json', 'utf8'));
  expect(JSON.parse(engine.restore(fixture)).ok).toBe(true);
  let sequence = JSON.parse(fixture).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  send({ type: 'minigameAction', action: 'start' }); send({ type: 'minigameTick', elapsedMs: 20501 });
  const result = send({ type: 'minigameTick', elapsedMs: 1001 }); expect(result.snapshot.pending.state.failed).toBe(true);
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.goto('./'); await expect(page.locator('#original-menu')).toBeVisible();
  await page.locator('#import-file').setInputFiles({ name: 'fire-timeout.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.fire-active[data-native-phase="lossAnimation"]')).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('fire-loss-animation.png') });
  await expect(page.locator('.fire-active[data-native-phase="lossBanner"]')).toBeVisible();
  await page.waitForTimeout(750);
  await page.screenshot({ path: testInfo.outputPath('fire-loss-banner.png') });
  await expect(page.locator('.fire-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('小游戏暂停'); expect(errors).toEqual([]);
});

test('student quiz competition restores a question and settles the earned first prize', async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'competition');
  await expect(page.getByRole('button', { name: '智力大赛', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('competition-menu.png') });
  await page.getByRole('button', { name: '智力大赛', exact: true }).click();
  await page.getByRole('button', { name: '继续比赛', exact: true }).click();
  await expect(page.locator('.competition-answer:enabled')).toHaveCount(4);
  const prompt = await page.locator('.competition-question p').textContent();
  await page.screenshot({ path: testInfo.outputPath('competition-question.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.competition-question p')).toHaveText(prompt!);
  const { highTest } = JSON.parse(await readFile('dist/assets/catalog/questionnaires.json', 'utf8'));
  for (let answer = 0; answer < 10; answer++) {
    await expect(page.locator('.competition-answer:enabled')).toHaveCount(4);
    const question = await page.locator('.competition-question p').textContent();
    const record = highTest.records.find((row: { question: string }) => row.question === question);
    expect(record).toBeDefined();
    const options = await page.locator('.competition-answer').allTextContents();
    const index = options.findIndex(option => option.slice(5) === record.options[0]); expect(index).toBeGreaterThanOrEqual(0);
    await page.locator('.competition-answer').nth(index).click();
    await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'quizFeedback');
    await expect(page.locator('.competition-question p')).toHaveText(question!);
    if (answer === 0) await page.screenshot({ path: testInfo.outputPath('competition-feedback.png') });
  }
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'quizResult');
  await page.screenshot({ path: testInfo.outputPath('competition-result.png') });
  await page.getByRole('button', { name: '继续比赛', exact: true }).click();
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'rankPrize');
  await page.screenshot({ path: testInfo.outputPath('competition-rank.png') });
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'moneyPrize');
  await page.screenshot({ path: testInfo.outputPath('competition-money.png') });
  await expect(page.locator('.competition-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('比赛暂停'); expect(errors).toEqual([]);
});

test('student beauty competition renders original catwalk, resumes and shows its podium', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'competition');
  await page.getByRole('button', { name: '造型大赛', exact: true }).click();
  await page.screenshot({ path: testInfo.outputPath('competition-intro.png') });
  for (let pageIndex = 0; pageIndex < 6; pageIndex++) await page.getByRole('button', { name: '继续比赛', exact: true }).click();
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'beauty');
  await page.waitForTimeout(700);
  await page.screenshot({ path: testInfo.outputPath('competition-catwalk.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'beauty');
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  const { Engine, content } = await testRuntime();
  const engine = new Engine(content);
  const fixture = (await readFile('tests/fixtures/browser-competition.json', 'utf8'));
  expect(JSON.parse(engine.restore(fixture)).ok).toBe(true);
  let sequence = JSON.parse(fixture).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  send({ type: 'competitionAction', action: 'choose', index: 0 });
  for (let intro = 0; intro < 6; intro++) send({ type: 'competitionAction', action: 'continue' });
  let result = send({ type: 'competitionTick', elapsedMs: 16 });
  for (let frame = 0; frame < 6000 && result.snapshot.pending.state.beauty.phase !== 'podium'; frame++) result = send({ type: 'competitionTick', elapsedMs: 100 });
  expect(result.snapshot.pending.state.beauty.phase).toBe('podium');
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.locator('#import-file').setInputFiles({ name: 'beauty-podium.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await page.waitForTimeout(6000);
  await page.screenshot({ path: testInfo.outputPath('competition-podium.png') });
  await expect(page.locator('.competition-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('比赛暂停'); expect(errors).toEqual([]);
});

test('student flying competition accepts alternating steps and restores before its native podium', async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'competition');
  await page.getByRole('button', { name: '飞人大赛', exact: true }).click();
  for (let intro = 0; intro < 6; intro++) await page.getByRole('button', { name: '继续比赛', exact: true }).click();
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-detail', 'running');
  await page.locator('#game-canvas').focus();
  for (let step = 0; step < 8; step++) await page.keyboard.press(step % 2 ? 'ArrowRight' : 'ArrowLeft');
  await page.screenshot({ path: testInfo.outputPath('competition-running.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-phase', 'running');
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  const { Engine, content } = await testRuntime();
  const engine = new Engine(content);
  const fixture = (await readFile('tests/fixtures/browser-competition.json', 'utf8'));
  expect(JSON.parse(engine.restore(fixture)).ok).toBe(true);
  let sequence = JSON.parse(fixture).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  send({ type: 'competitionAction', action: 'choose', index: 1 });
  for (let intro = 0; intro < 6; intro++) send({ type: 'competitionAction', action: 'continue' });
  let result = send({ type: 'competitionTick', elapsedMs: 16 });
  for (let frame = 0; frame < 8000 && result.snapshot.pending.state.running.phase !== 'podium'; frame++) {
    if (result.snapshot.pending.state.running.contestant === 0) send({ type: 'competitionAction', action: frame % 2 ? 'right' : 'left' });
    result = send({ type: 'competitionTick', elapsedMs: 100 });
  }
  expect(result.snapshot.pending.state.running.phase).toBe('podium');
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.locator('#import-file').setInputFiles({ name: 'running-podium.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.competition-active')).toHaveAttribute('data-native-detail', 'podium');
  const importedClock = await page.locator('.competition-active').getAttribute('data-native-clock');
  await page.waitForTimeout(4000);
  await page.screenshot({ path: testInfo.outputPath('competition-running-podium.png') });
  await expect(page.locator('.competition-active')).not.toHaveAttribute('data-native-clock', importedClock!);
  await expect(page.locator('.competition-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('比赛暂停'); expect(errors).toEqual([]);
});

test('ending defeat preserves the history scroll and narrated movie before reincarnation', async ({ page }, testInfo) => {
  test.setTimeout(90_000);
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'ending-loss');
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'summary');
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-ready', 'true');
  await page.locator('#game-canvas').focus(); await page.keyboard.down('s'); await page.waitForTimeout(450); await page.keyboard.up('s');
  await page.waitForTimeout(1900); await page.screenshot({ path: testInfo.outputPath('ending-history.png') });
  const beforeSave = Number(await page.locator('.ending-active').getAttribute('data-native-clock'));
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'summary');
  expect(Number(await page.locator('.ending-active').getAttribute('data-native-clock'))).toBeGreaterThanOrEqual(beforeSave);
  await page.getByRole('button', { name: '继续结局', exact: true }).click();
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'movie');
  await expect.poll(() => page.locator('.ending-movie video').evaluate((video: HTMLVideoElement) => video.readyState)).toBeGreaterThanOrEqual(2);
  await expect(page.locator('.ending-subtitle')).not.toHaveText('');
  await page.screenshot({ path: testInfo.outputPath('ending-movie.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'movie');
  await expect.poll(() => page.locator('.ending-movie video').evaluate((video: HTMLVideoElement) => video.currentTime)).toBeGreaterThan(.2);
  await page.locator('#game-canvas').focus(); await page.keyboard.press('Escape');
  await expect(page.locator('.ending-active')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('ending-reincarnated.png') });
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('结局播放暂停'); expect(errors).toEqual([]);
});

test('ending victory shows the original credits and allows declining another life', async ({ page }, testInfo) => {
  test.setTimeout(90_000);
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'ending-win');
  await page.getByRole('button', { name: '继续结局', exact: true }).click();
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'movie');
  const { Engine, content } = await testRuntime(), engine = new Engine(content);
  const raw = (await readFile('tests/fixtures/browser-ending-win.json', 'utf8'));
  expect(JSON.parse(engine.restore(raw)).ok).toBe(true);
  let sequence = JSON.parse(raw).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  send({ type: 'endingAction', action: 'continue' });
  let result = send({ type: 'endingTick', elapsedMs: 1000 });
  for (let frame = 0; frame < 100 && result.snapshot.pending.state.movie?.phase !== 'outro'; frame++) result = send({ type: 'endingTick', elapsedMs: 1000 });
  expect(result.snapshot.pending.state.movie.phase).toBe('outro');
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.locator('#import-file').setInputFiles({ name: 'ending-outro.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-detail', 'outro');
  await page.waitForTimeout(500); await page.screenshot({ path: testInfo.outputPath('ending-outro.png') });
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'credits');
  await page.locator('#game-canvas').focus();
  await page.waitForTimeout(2700); await page.keyboard.down('s'); await page.waitForTimeout(300); await page.keyboard.up('s');
  await page.keyboard.press('s'); await page.waitForTimeout(150);
  const scrollOffset = () => page.locator('.ending-credit-content').evaluate(element => new DOMMatrixReadOnly(getComputedStyle(element).transform).m42);
  const releasedOffset = await scrollOffset(); await page.waitForTimeout(200);
  expect(releasedOffset - await scrollOffset()).toBeLessThan(10);
  await page.screenshot({ path: testInfo.outputPath('ending-credits.png') });
  await page.keyboard.press('Escape');
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'endImage');
  await expect(page.getByRole('button', { name: '继续', exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('ending-end-image.png') });
  await page.getByRole('button', { name: '继续', exact: true }).click();
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'reincarnation');
  await expect(page.getByRole('button', { name: '结束人生', exact: true })).toBeVisible();
  const canvas = page.locator('#game-canvas'), bounds = await canvas.boundingBox();
  await page.mouse.move(bounds!.x + bounds!.width * .8, bounds!.y + bounds!.height * .2);
  const cursorFrame = await canvas.screenshot();
  await expect.poll(async () => (await canvas.screenshot()).equals(cursorFrame), { timeout: 3000 }).toBe(false);
  await page.screenshot({ path: testInfo.outputPath('ending-choice.png') });
  await page.getByRole('button', { name: '结束人生', exact: true }).click();
  await expect(page.locator('#original-menu')).toBeVisible();
  await expect(page.locator('.ending-active')).toHaveCount(0);
  await expect(page.locator('#fatal')).toBeHidden(); expect(errors).toEqual([]);
});

test('ending history renders the original animated career portrait and edge palette', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  const { Engine, content } = await testRuntime(), engine = new Engine(content);
  const raw = (await readFile('tests/fixtures/browser-ending-loss.json', 'utf8'));
  expect(JSON.parse(engine.restore(raw)).ok).toBe(true);
  let sequence = JSON.parse(raw).state.lastSeq;
  const send = (command: object) => JSON.parse(engine.dispatch(JSON.stringify({ seq: ++sequence, ...command })));
  send({ type: 'endingAction', action: 'sDown' });
  let result = send({ type: 'endingTick', elapsedMs: 50 });
  for (let frame = 0; frame < 200 && !(result.snapshot.pending.state.summary.showActor && result.snapshot.pending.state.summary.actorY <= 220); frame++) result = send({ type: 'endingTick', elapsedMs: 50 });
  expect(result.snapshot.pending.state.summary.showActor).toBe(true);
  expect(result.snapshot.pending.state.summary.actorY).toBeLessThanOrEqual(220);
  send({ type: 'endingAction', action: 'sUp' });
  const payload = engine.serialize(); engine.free();
  const backup = { format: 'virtual-life-web-save', version: 1, savedAt: new Date().toISOString(), contentId: hash(JSON.parse(payload).contentHash), payload, checksum: hash(payload) };
  await page.goto('./'); await expect(page.locator('#original-menu')).toBeVisible();
  await page.locator('#import-file').setInputFiles({ name: 'ending-portrait.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(backup)) });
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-phase', 'summary');
  await expect(page.locator('.ending-active')).toHaveAttribute('data-native-ready', 'true');
  await page.screenshot({ path: testInfo.outputPath('ending-career-portrait.png') });
  await page.waitForTimeout(250); await page.screenshot({ path: testInfo.outputPath('ending-career-portrait-next.png') });
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('结局播放暂停'); expect(errors).toEqual([]);
});

test('map panning, browser save, backup integrity, restore and offline reload', async ({ page, context }, testInfo) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByLabel('地图浏览', { exact: true }).selectOption('park');
  await expect(page.locator('#status')).toContainText('地图已载入');
  const canvas = page.locator('#game-canvas');
  const beforePan = await canvas.screenshot();
  await canvas.focus();
  await page.keyboard.press('ArrowRight');
  await page.waitForTimeout(100);
  expect(Buffer.compare(beforePan, await canvas.screenshot())).not.toBe(0);
  await page.screenshot({ path: testInfo.outputPath('park-map.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await expect(page.locator('#status')).toContainText('进度已保存');
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: '导出备份', exact: true }).click();
  const download = await downloadPromise;
  const exported = JSON.parse(await readFile((await download.path())!, 'utf8'));
  expect(exported.format).toBe('virtual-life-web-save');
  const badSave = { ...exported, payload: `${exported.payload}corruption` };
  await page.locator('#import-file').setInputFiles({ name: 'bad.vlsave.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(badSave)) });
  await expect(page.getByRole('heading', { name: '导入失败' })).toBeVisible();
  await expect(page.locator('#dialog')).toContainText('校验失败');
  await page.getByRole('button', { name: '返回', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('#status')).toContainText('已读取存档');
  await expect(page.locator('#map-choice')).toHaveValue('park');
  await context.setOffline(true);
  await page.reload();
  await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.locator('#status')).toContainText('已读取存档');
  expect(errors).toEqual([]);
});

test('map decorations animate behind foreground scenery and native palette cycles remain live', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('./'); await expect(page.locator('#original-menu')).toBeVisible();
  await page.getByRole('combobox', { name: '地图浏览', exact: true }).selectOption('park');
  await expect(page.locator('#status')).toContainText('地图已载入');
  const before = await page.locator('#game-canvas').screenshot({ path: testInfo.outputPath('park-decorations.png') });
  await page.waitForTimeout(500);
  const after = await page.locator('#game-canvas').screenshot();
  expect(after.equals(before)).toBe(false);
  await page.getByRole('combobox', { name: '地图浏览', exact: true }).selectOption('acient');
  await expect(page.locator('#status')).toContainText('地图已载入');
  await page.locator('#game-canvas').screenshot({ path: testInfo.outputPath('acient-palette.png') });
  await page.waitForTimeout(600);
  await page.locator('#game-canvas').screenshot({ path: testInfo.outputPath('acient-palette-cycled.png') });
  await expect(page.locator('#fatal')).toBeHidden(); await expect(page.locator('#status')).not.toContainText('地图动画暂停'); expect(errors).toEqual([]);
});

test('phone landscape and portrait preserve the original aspect ratio without overflow', async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 844, height: 390 });
  await page.goto('./');
  await expect(page.locator('#original-menu')).toBeVisible();
  const landscape = await page.locator('#game-canvas').boundingBox();
  expect(landscape!.width / landscape!.height).toBeCloseTo(4 / 3, 2);
  await page.screenshot({ path: testInfo.outputPath('phone-landscape.png') });
  await expect(page.locator('#fatal')).toBeHidden();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.locator('.portrait-note')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('phone-portrait.png') });
  await expect(page.locator('#fatal')).toBeHidden();
});

test('unsupported WebGPU displays a clear error and leaves save management available', async ({ page }) => {
  await page.addInitScript(() => Object.defineProperty(navigator, 'gpu', { value: undefined }));
  await page.goto('./');
  await expect(page.locator('#fatal')).toContainText('此浏览器未提供 WebGPU');
  await expect(page.getByRole('button', { name: '存档管理', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await expect(page.getByRole('heading', { name: '存档管理' })).toBeVisible();
});

test('ordinary battle plays its native tutorial, accepts cards, and restores a live encounter', async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'battle');
  const ui = page.locator('#logical-ui');
  await expect(ui).toHaveClass(/battle-active/);
  await expect(page.getByRole('button', { name: '下一页战斗教学' })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('battle-tutorial.png') });
  for (let index = 0; index < 9; index++) await page.getByRole('button', { name: '下一页战斗教学' }).click();
  await expect(page.getByRole('button', { name: '开始战斗', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '开始战斗', exact: true }).click();
  await expect(ui).toHaveAttribute('data-native-phase', 'playing');
  const card = page.getByRole('button', { name: '选择战斗卡片 1', exact: true });
  await expect(card).toBeEnabled(); await card.click(); await expect(card).toHaveAttribute('aria-pressed', 'true');
  await page.screenshot({ path: testInfo.outputPath('battle-playing.png') });
  await expect(page.getByRole('button', { name: '出招', exact: true })).toBeVisible();
  await page.getByRole('button', { name: '出招', exact: true }).click();
  await expect(card).toBeDisabled();
  await page.getByRole('button', { name: '存档管理', exact: true }).click();
  await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click();
  await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(ui).toHaveClass(/battle-active/); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await expect(page.locator('#status')).not.toContainText('战斗暂停'); expect(errors).toEqual([]);
});

test('final battle shop purchases and sells original cards before the opening and first attack', async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'final-shop');
  const ui = page.locator('#logical-ui');
  await expect(ui).toHaveAttribute('data-native-phase', 'shop'); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await expect(ui).toHaveAttribute('data-native-items', '3');
  await page.screenshot({ path: testInfo.outputPath('final-shop.png') });
  await page.getByRole('button', { name: '查看最终战道具 1', exact: true }).click();
  await page.getByRole('button', { name: '购买最终战道具', exact: true }).click();
  await expect(ui).toHaveAttribute('data-native-items', '4');
  const held = page.getByRole('button', { name: '选择最终战道具 4', exact: true }); await expect(held).toBeEnabled(); await held.click();
  await page.getByRole('button', { name: '出售最终战道具', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-items', '3');
  await expect(ui).toHaveAttribute('data-native-money', '518600');
  await page.getByRole('button', { name: '开始最终战', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'opening');
  await expect(page.locator('.ending-movie video')).toBeVisible(); await page.screenshot({ path: testInfo.outputPath('final-opening.png') });
  await page.getByRole('button', { name: '跳过最终战开场', exact: true }).click();
  await expect(ui).toHaveAttribute('data-native-phase', 'playing');
  const card = page.getByRole('button', { name: '选择最终战卡片 1', exact: true }); await expect(card).toBeEnabled(); await card.click(); await expect(card).toHaveAttribute('aria-pressed', 'true');
  await page.getByRole('button', { name: '选择最终战道具 1', exact: true }).click();
  await expect(page.getByRole('button', { name: '使用最终战道具', exact: true })).toBeEnabled();
  await page.screenshot({ path: testInfo.outputPath('final-combat.png') });
  await page.getByRole('button', { name: '最终战出招', exact: true }).click();
  await expect(page.getByRole('button', { name: '最终战出招', exact: true })).toBeDisabled();
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click(); await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(ui).toHaveClass(/final-battle-active/); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await expect(page.locator('#status')).not.toContainText('最终战暂停'); expect(errors).toEqual([]);
});

test('inventory uses healing, confirms discards, equips and restores all original item pages', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'inventory');
  const ui = page.locator('#logical-ui'); await expect(ui).toHaveClass(/inventory-active/); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await expect(ui).toHaveAttribute('data-native-items', '5'); await expect(ui).toHaveAttribute('data-native-hp', '30');
  await page.screenshot({ path: testInfo.outputPath('inventory-ordinary.png') });
  await page.getByRole('button', { name: '使用选中道具', exact: true }).click();
  await expect(ui).toHaveAttribute('data-native-items', '4'); await expect(ui).toHaveAttribute('data-native-hp', '40');
  await page.getByRole('button', { name: '特殊道具', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'special');
  await page.getByRole('button', { name: '丢弃选中道具', exact: true }).click(); await expect(page.getByRole('button', { name: '确定丢弃', exact: true })).toBeEnabled();
  await page.screenshot({ path: testInfo.outputPath('inventory-discard-confirm.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click(); await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '取消丢弃', exact: true })).toBeEnabled(); await page.getByRole('button', { name: '取消丢弃', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-items', '2');
  await page.getByRole('button', { name: '丢弃选中道具', exact: true }).click(); await page.getByRole('button', { name: '确定丢弃', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-items', '1');
  await page.getByRole('button', { name: '装备道具', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'equipment');
  await page.getByRole('button', { name: '装备选中道具', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-equipped', '0,-1,-1,-1,-1,-1');
  await page.screenshot({ path: testInfo.outputPath('inventory-equipment.png') });
  await page.getByRole('button', { name: '卸下选中装备', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-equipped', '-1,-1,-1,-1,-1,-1');
  await page.getByRole('button', { name: /^选择道具 / }).nth(1).click(); await page.getByRole('button', { name: '装备选中道具', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-equipped', '-1,3,-1,-1,-1,-1');
  await page.getByRole('button', { name: '丢弃选中道具', exact: true }).click(); await page.getByRole('button', { name: '确定丢弃', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-items', '1'); await expect(ui).toHaveAttribute('data-native-equipped', '-1,-1,-1,-1,-1,-1');
  await page.getByRole('button', { name: '返回地图', exact: true }).click(); await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '道具一览', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'ordinary'); await expect(ui).toHaveAttribute('data-native-items', '4');
  await expect(page.locator('#status')).not.toContainText('暂停'); expect(errors).toEqual([]);
});

test('parameter overview restores original skills, career history, home and investment pages', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await importFixture(page, 'status');
  const ui = page.locator('#logical-ui'); await expect(ui).toHaveClass(/parameters-active/); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await expect(ui).toHaveAttribute('data-native-phase', 'overview'); await page.screenshot({ path: testInfo.outputPath('parameters-overview.png') });
  await page.getByRole('button', { name: '职业记录', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'careerHistory');
  await page.getByRole('button', { name: '参数列表向下', exact: true }).click(); await page.getByRole('button', { name: '参数列表向下', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-scroll', '2');
  await page.screenshot({ path: testInfo.outputPath('parameters-career-history.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click();
  await page.getByRole('button', { name: '主菜单', exact: true }).click(); await page.getByRole('button', { name: '继续游戏', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-scroll', '2');
  await page.getByRole('button', { name: '返回参数一览', exact: true }).click(); await page.getByRole('button', { name: '技能一览', exact: true }).click();
  await expect(ui).toHaveAttribute('data-native-phase', 'skills'); await page.screenshot({ path: testInfo.outputPath('parameters-skills.png') });
  for (let row = 0; row < 22; row++) await page.getByRole('button', { name: '参数列表向下', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-scroll', '22');
  await page.getByRole('button', { name: '返回参数一览', exact: true }).click(); await page.getByRole('button', { name: '住宅一览', exact: true }).click();
  await expect(ui).toHaveAttribute('data-native-phase', 'house'); await expect(ui).toHaveAttribute('data-native-ready', 'true'); await expect(ui).toHaveAttribute('data-native-transition', 'none'); await page.screenshot({ path: testInfo.outputPath('parameters-house.png') });
  await page.keyboard.press('Escape'); await expect(ui).toHaveAttribute('data-native-phase', 'overview');
  await page.getByRole('button', { name: '投资一览', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'investment'); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await page.screenshot({ path: testInfo.outputPath('parameters-investment.png') }); await expect(ui).toHaveAttribute('data-native-phase', 'overview', { timeout: 12_000 });
  await page.getByRole('button', { name: '返回地图', exact: true }).click(); await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await expect(page.locator('#status')).not.toContainText('暂停'); expect(errors).toEqual([]);
});

test('current map overview and original system volume preserve the life state', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => { const audio: HTMLAudioElement[] = [], NativeAudio = window.Audio; Object.assign(window, { testAudio: audio }); window.Audio = class extends NativeAudio { constructor(source?: string) { super(source); audio.push(this); } }; });
  await page.goto('./'); await page.getByRole('button', { name: '开始新游戏', exact: true }).click(); await page.getByLabel('姓名').fill('浏览测试'); await page.getByRole('button', { name: '确定', exact: true }).click();
  const readSave = async (): Promise<string> => page.evaluate(() => new Promise<string>((resolve, reject) => { const opening = indexedDB.open('virtual-life', 1); opening.onsuccess = () => { const db = opening.result, request = db.transaction('saves').objectStore('saves').get('current'); request.onsuccess = () => { resolve(request.result?.payload ?? ''); db.close(); }; request.onerror = () => reject(request.error); }; opening.onerror = () => reject(opening.error); }));
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click(); await expect.poll(readSave).not.toBe(''); const before = await readSave();
  const ui = page.locator('#logical-ui'); await page.getByRole('button', { name: '地图浏览', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'mapOverview'); await expect(ui).toHaveAttribute('data-native-ready', 'true');
  await page.screenshot({ path: testInfo.outputPath('current-map-overview.png') }); await page.keyboard.press('Escape'); await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '系统操作', exact: true }).click(); await expect(ui).toHaveAttribute('data-native-phase', 'options-menu'); await expect(ui).toHaveAttribute('data-native-ready', 'true'); await page.screenshot({ path: testInfo.outputPath('original-system-menu.png') });
  await page.getByRole('button', { name: '音量设置', exact: true }).click(); const volume = page.getByRole('slider', { name: '总音量', exact: true }); await expect(volume).toHaveValue('50');
  await volume.focus(); await volume.press('Home'); for (let step = 0; step < 25; step++) await volume.press('ArrowRight'); await expect(ui).toHaveAttribute('data-native-volume', '25');
  expect(await page.evaluate(() => (window as unknown as { testAudio: HTMLAudioElement[] }).testAudio.every(audio => audio.volume === .25))).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('original-volume.png') }); await page.getByRole('button', { name: '返回系统操作', exact: true }).click();
  await page.getByRole('button', { name: '离开游戏', exact: true }).click(); await expect(page.getByRole('button', { name: '取消离开游戏', exact: true })).toBeVisible(); await page.keyboard.press('n');
  await page.getByRole('button', { name: '返回游戏', exact: true }).click(); await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click(); await expect.poll(readSave).toBe(before);
  await page.reload(); await expect(page.locator('#original-menu')).toBeVisible(); await page.locator('#original-menu').getByRole('button', { name: '游戏选项', exact: true }).click(); await page.getByRole('button', { name: '音量设置', exact: true }).click(); await expect(page.getByRole('slider', { name: '总音量', exact: true })).toHaveValue('25');
  expect(errors).toEqual([]);
});

test('a destroyed WebGPU device preserves the live game and recovers after reload', async ({ page }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
    navigator.gpu.requestAdapter = async options => {
      const adapter = await requestAdapter(options); if (!adapter) return adapter;
      const requestDevice = adapter.requestDevice.bind(adapter);
      adapter.requestDevice = async descriptor => { const device = await requestDevice(descriptor); Object.assign(window, { testGpuDevice: device }); return device; };
      return adapter;
    };
  });
  await page.goto('./'); await page.getByRole('button', { name: '开始新游戏', exact: true }).click(); await page.getByLabel('姓名').fill('设备恢复'); await page.getByRole('button', { name: '确定', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click(); await expect(page.locator('#status')).toContainText('进度已保存');
  const before = (await storedSaveSlots(page)).current; expect(before).toBeTruthy();
  await page.evaluate(() => (window as unknown as { testGpuDevice: GPUDevice }).testGpuDevice.destroy());
  await expect(page.locator('#fatal')).toContainText('显示设备已中断'); await expect(page.getByRole('button', { name: '主菜单', exact: true })).toBeDisabled();
  expect((await storedSaveSlots(page)).current).toBe(before); await page.screenshot({ path: testInfo.outputPath('gpu-device-lost.png') });
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); const downloadPromise = page.waitForEvent('download'); await page.getByRole('button', { name: '导出备份', exact: true }).click();
  const download = await downloadPromise; expect(JSON.parse(await readFile((await download.path())!, 'utf8')).payload).toBe(before);
  await page.reload(); await expect(page.locator('#original-menu')).toBeVisible(); await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled(); await expect(page.locator('#fatal')).toBeHidden(); expect((await storedSaveSlots(page)).current).toBe(before);
  await page.getByRole('button', { name: '参数一览', exact: true }).click(); await expect(page.locator('#logical-ui')).toHaveAttribute('data-native-phase', 'overview'); await expect(page.locator('#logical-ui')).toHaveAttribute('data-native-ready', 'true');
  await page.screenshot({ path: testInfo.outputPath('gpu-restored-parameters.png') }); expect(errors).toEqual([]);
});

test('an uncached offline map reports the failure and keeps both save slots intact', async ({ page, context }, testInfo) => {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('./'); await page.getByRole('button', { name: '开始新游戏', exact: true }).click(); await page.getByLabel('姓名').fill('离线恢复'); await page.getByRole('button', { name: '确定', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: '存档管理', exact: true }).click(); await page.getByRole('button', { name: '保存进度', exact: true }).click(); await expect(page.locator('#status')).toContainText('进度已保存');
  const before = await storedSaveSlots(page); expect(before.current).toBeTruthy();
  expect(await page.evaluate(async () => {
    const maps = await (await fetch('./assets/catalog/maps.json')).json(), manifest = await (await fetch('./assets/manifest.json')).json();
    const resource = maps.maps.find((map: { id: string }) => map.id === 'business').backgroundTiles[0].resourceId;
    const object = manifest.objects.find((entry: { sha256: string }) => entry.sha256 === manifest.resources[resource].sha256);
    return !!await caches.match(new URL(object.path, location.href));
  })).toBe(false);
  await context.setOffline(true); await page.getByRole('combobox', { name: '地图浏览', exact: true }).selectOption('business');
  await expect(page.getByRole('heading', { name: '地图读取失败', exact: true })).toBeVisible(); await expect(page.locator('#dialog')).toContainText('资源下载失败，请检查网络连接后重试。'); expect(await storedSaveSlots(page)).toEqual(before); await page.screenshot({ path: testInfo.outputPath('offline-uncached-map.png') });
  await page.getByRole('button', { name: '返回', exact: true }).click(); await page.getByRole('button', { name: '主菜单', exact: true }).click(); await page.getByRole('button', { name: '继续游戏', exact: true }).click();
  await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled(); expect(await storedSaveSlots(page)).toEqual(before);
  await context.setOffline(false); await page.getByRole('combobox', { name: '地图浏览', exact: true }).selectOption('business'); await expect(page.locator('#status')).toContainText('地图已载入');
  await page.getByRole('button', { name: '主菜单', exact: true }).click(); await page.getByRole('button', { name: '继续游戏', exact: true }).click(); await expect(page.getByRole('button', { name: '掷骰子', exact: true })).toBeEnabled();
  expect((await storedSaveSlots(page)).current).toBe(before.current); expect(errors).toEqual([]);
});
