// Browser plugin not available. These Playwright cases use the production file
// picker, canvas controls and keyboard; the test API only supplies frame time.
import { test, expect, type Page, type TestInfo } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { control, diagnostic, freeze, importScenario, open, ready, scenario, type Control, saveSelected } from './helpers';

interface Choice { index: number; text: string; disabled?: boolean }
interface Offer { index: number; name?: string; description?: string; category?: number; price: number; bought?: boolean; purchased?: boolean; disabled?: boolean }
interface Career { index: number; id: number; name: string; eligible: boolean; disabled: boolean }
interface NativeView {
  kind: string; options: Choice[]; offers?: Offer[]; careers?: Career[]; purchases?: number;
  mode?: string | null; outcome?: string | null; offeredId?: number; phase?: string;
  questionIndex?: number; selectedAnswer?: number | null; currentQuestion?: { prompt: string; options: string[] };
  points?: number; money?: number; answered?: number; started?: boolean;
  offered?: { id: number; price: number }; entries?: { id: number; index: number; price: number }[];
  known?: { learned: boolean; level: number }[]; carried?: { id: number; level: number }[];
  exitIndex?: number; questionCount?: number; format?: number; answers?: number[];
}
interface Snapshot {
  lastSeq: number; stats: { money: number; [field: string]: number }; nativeView?: NativeView;
  pending: { kind: string; game?: string; state?: Record<string, unknown>; options?: Choice[] } | null;
  life: { profession: number };
}
interface SavedRecord { savedAt: string; payload: string }
interface SavedPayload {
  format: string; version: number; state: Record<string, unknown>;
  presentation?: { profile: Record<string, unknown>; allocations?: Record<string, number>; sceneMs: number; runtime: Record<string, unknown> };
}
interface NativePanel { scope: string; selection: number; tab: number; scroll: number; phase: string; elapsed: number }
interface ScenarioManifest { scenarios: { id: string; file: string; source: string; nativeKind?: string | null }[] }

const nativeScenes = [
  ['statShop', 'activity-native-stat-shop-browse'],
  ['itemShop', 'activity-native-item-shop-browse'],
  ['equipmentWorkshop', 'activity-native-equipment-workshop-purchase'],
  ['career', 'activity-native-career-select'],
  ['housing', 'activity-native-housing-purchase'],
  ['fortune', 'activity-native-fortune-question'],
] as const;

function publicFixture(name: string): SavedPayload {
  const manifest = JSON.parse(readFileSync('output/bevy-scenarios/manifest.json', 'utf8')) as ScenarioManifest;
  const entry = manifest.scenarios.find(entry => entry.id === name);
  expect(entry, `Missing public-command fixture ${name}; run pnpm test:fixtures`).toBeDefined();
  expect(entry!.source).toMatch(/^output\/life-probes\//);
  const envelope = JSON.parse(readFileSync(scenario(name), 'utf8')) as SavedRecord & { format: string; version: number };
  expect(envelope).toMatchObject({ format: 'virtual-life-bevy-save', version: 1 });
  const payload = JSON.parse(envelope.payload) as SavedPayload;
  expect(payload).toMatchObject({ format: 'virtual-life-bevy-save', version: 1 });
  // nativeView is derived at read time, not injected into a save's domain state.
  expect(payload.state).not.toHaveProperty('nativeView');
  return payload;
}
function publicKind(kind: 'schoolExam' | 'allocation'): string {
  const manifest = JSON.parse(readFileSync('output/bevy-scenarios/manifest.json', 'utf8')) as ScenarioManifest;
  const candidates = manifest.scenarios.filter(entry => /^output\/life-probes\//.test(entry.source));
  const current = candidates.find(entry => entry.nativeKind === kind && (kind !== 'schoolExam' || entry.id.endsWith('-question')));
  // A previous probe may have written the same public state before nativeView
  // was added. Regeneration derives its view; filenames are not a save schema.
  const previous = candidates.find(entry => kind === 'schoolExam' ? /school-exam-question$/.test(entry.id) : /allocation(?:-preview)?$/.test(entry.id));
  const selected = current ?? previous;
  expect(selected, `No public ${kind} scenario; run pnpm test:fixtures`).toBeDefined();
  publicFixture(selected!.id);
  return selected!.id;
}
async function snapshot(page: Page): Promise<Snapshot> {
  return (await diagnostic(page)).snapshot as unknown as Snapshot;
}
async function native(page: Page): Promise<NativeView> {
  const view = (await snapshot(page)).nativeView;
  expect(view, 'Expected an active native panel').toBeDefined();
  return view!;
}
async function clock(page: Page, milliseconds: number): Promise<void> {
  await ready(page);
  await page.evaluate(frameMs => window.__virtualLifeBevy!.input!({ frameMs }), milliseconds);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await ready(page);
}
async function clickId(page: Page, id: string): Promise<void> {
  await ready(page);
  const target = (await diagnostic(page)).controls.find(control => control.id === id && control.enabled);
  expect(target, `Missing enabled original control ${id}`).toBeDefined();
  await clickLogical(page, target!.bounds.x + target!.bounds.width / 2, target!.bounds.y + target!.bounds.height / 2);
}
async function clickLogical(page: Page, x: number, y: number): Promise<void> {
  const canvas = await page.locator('#game-canvas').boundingBox();
  expect(canvas).not.toBeNull();
  await page.mouse.click(canvas!.x + x * canvas!.width / 640, canvas!.y + y * canvas!.height / 480);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await ready(page);
}
async function reachableControls(page: Page, filter: (control: Control) => boolean = () => true): Promise<Control[]> {
  const controls = (await diagnostic(page)).controls.filter(filter);
  expect(controls.length).toBeGreaterThan(0);
  for (const control of controls) {
    const b = control.bounds;
    expect(b.width, control.id).toBeGreaterThan(0);
    expect(b.height, control.id).toBeGreaterThan(0);
    expect(b.x, control.id).toBeGreaterThanOrEqual(0);
    expect(b.y, control.id).toBeGreaterThanOrEqual(0);
    expect(b.x + b.width, control.id).toBeLessThanOrEqual(640);
    expect(b.y + b.height, control.id).toBeLessThanOrEqual(480);
  }
  return controls;
}
async function currentRecord(page: Page): Promise<SavedRecord | null> {
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
async function save(page: Page): Promise<SavedPayload> {
  const domainBeforeMenu = await snapshot(page);
  const previous = (await currentRecord(page))?.savedAt;
  await page.keyboard.press('F6');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  // Time on the save screen must not advance a native timer or core clock.
  await clock(page, 1234);
  expect(await snapshot(page)).toEqual(domainBeforeMenu);
  await saveSelected(page);
  await expect.poll(async () => (await currentRecord(page))?.savedAt).not.toBe(previous);
  await expect.poll(async () => (await diagnostic(page)).notice).toContain('进度已保存');
  return JSON.parse((await currentRecord(page))!.payload) as SavedPayload;
}
async function saveReload(page: Page, info: TestInfo, name: string): Promise<SavedPayload> {
  const before = await snapshot(page);
  await page.locator('#game-canvas').screenshot({ path: info.outputPath(`${name}-before.png`) });
  const saved = await save(page);
  expect(saved.presentation, 'Production saves must retain Bevy presentation state').toBeDefined();
  await page.reload(); await ready(page); await freeze(page);
  expect((await diagnostic(page)).screen).toBe('Menu');
  await control(page, '继续游戏'); await control(page, '读取选中槽位'); await ready(page);
  expect(await snapshot(page)).toEqual(before);
  await page.locator('#game-canvas').screenshot({ path: info.outputPath(`${name}-restored.png`) });
  return saved;
}

for (const [kind, fixture] of nativeScenes) {
  test(`public native ${kind} controls stay in the original viewport and restore through F6`, async ({ page }, info) => {
    publicFixture(fixture);
    const errors = await open(page); await freeze(page); await importScenario(page, fixture);
    expect((await native(page)).kind).toBe(kind);
    expect((await native(page)).options).toEqual((await snapshot(page)).pending!.options);
    await reachableControls(page);
    await saveReload(page, info, `native-${kind}`);
    expect((await native(page)).kind).toBe(kind);
    await reachableControls(page);
    expect(errors).toEqual([]);
  });
}

test('the eight stat-shop slots retain selection and elapsed time, then buy their real index', async ({ page }, info) => {
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-native-stat-shop-browse');
  const before = await snapshot(page), offers = (await native(page)).offers!;
  expect(await reachableControls(page, control => /^native-stat-\d+$/.test(control.id))).toHaveLength(8);
  const offer = [...offers].reverse().find(offer => !offer.disabled && offer.price <= before.stats.money)!;
  expect(offer).toBeDefined();
  await clickId(page, `native-stat-${offer.index}`);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq);
  await clock(page, 450);
  const saved = await saveReload(page, info, 'stat-shop-selected');
  const selected = saved.presentation!.profile._nativePanel as NativePanel;
  expect(selected).toMatchObject({ selection: offer.index, elapsed: 450 });
  expect((await diagnostic(page)).controls).toContainEqual(expect.objectContaining({ id: 'native-yes', enabled: true }));
  const savedAgain = await save(page);
  expect(savedAgain.presentation!.profile._nativePanel).toEqual(selected);
  await page.keyboard.press('F6'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Game');
  await clickId(page, 'native-yes');
  await expect.poll(async () => (await native(page)).offers![offer.index]!.bought).toBe(true);
  expect((await native(page)).purchases).toBe(1);
  expect((await snapshot(page)).stats.money).toBe(before.stats.money - offer.price);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 1);
  expect((await native(page)).offers!.filter(offer => offer.bought)).toHaveLength(1);
  expect(errors).toEqual([]);
});

test('the item shop has four tabs, three products per stocked tab and uses original purchase indices', async ({ page }, info) => {
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-native-item-shop-browse');
  await page.keyboard.press('Enter'); await ready(page);
  expect(await reachableControls(page, control => /^native-category-\d+$/.test(control.id))).toHaveLength(4);
  await clickId(page, 'native-category-2');
  expect(await reachableControls(page, control => /^native-item-\d+$/.test(control.id))).toHaveLength(3);
  await clickId(page, 'native-category-1');
  const equipment = (await native(page)).offers!.filter(offer => offer.category === 1 && !offer.purchased);
  expect((await diagnostic(page)).controls.filter(control => /^native-item-\d+$/.test(control.id))).toHaveLength(equipment.length);
  expect(equipment.length).toBeLessThanOrEqual(3);
  await clickId(page, 'native-category-3');
  const before = await snapshot(page), offers = (await native(page)).offers!.filter(offer => offer.category === 3);
  expect(await reachableControls(page, control => /^native-item-\d+$/.test(control.id))).toHaveLength(3);
  const offer = offers.find(offer => offer.price <= before.stats.money)!;
  expect(offer).toBeDefined();
  await clickId(page, `native-item-${offer.index}`);
  const saved = await saveReload(page, info, 'item-shop-career-product');
  expect(saved.presentation!.profile._nativePanel).toMatchObject({ tab: 3, selection: offer.index });
  await clickId(page, 'native-yes');
  await expect.poll(async () => (await native(page)).offers!.find(item => item.index === offer.index)!.purchased).toBe(true);
  expect((await snapshot(page)).stats.money).toBe(before.stats.money - offer.price);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 1);
  expect((await native(page)).options.some(option => option.index === offer.index)).toBe(false);
  await clock(page, 2001);
  expect((await diagnostic(page)).controls.some(control => control.id === `native-item-${offer.index}`)).toBe(false);
  expect(errors).toEqual([]);
});

test('all 24 careers can be reached by scrolling, with eligibility enforced at confirmation', async ({ page }, info) => {
  const original = publicFixture('activity-native-career-select');
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-native-career-select');
  await page.keyboard.press('Enter'); await ready(page);
  const before = await snapshot(page), careers = (await native(page)).careers!;
  expect(careers).toHaveLength(24);
  const expected = careers.filter(career => career.id !== before.life.profession);
  const seen = new Set<number>();
  for (let step = 0; step <= 24; step++) {
    const visible = await reachableControls(page, control => /^native-career-\d+$/.test(control.id));
    expect(visible.length).toBeLessThanOrEqual(10);
    for (const control of visible) seen.add(Number(control.id.slice('native-career-'.length)));
    if (seen.size === expected.length) break;
    await clickId(page, 'native-career-scroll-down');
  }
  expect([...seen].sort((a, b) => a - b)).toEqual(expected.map(career => career.index).sort((a, b) => a - b));
  expect(await snapshot(page)).toEqual(before);
  const saved = await saveReload(page, info, 'career-scrolled');
  expect((saved.presentation!.profile._nativePanel as NativePanel).scroll).toBeGreaterThan(0);
  expect(saved.state).toEqual(original.state);
  expect(saved.state.rng).toEqual(original.state.rng);
  const visibleIds = (await diagnostic(page)).controls.map(control => control.id);
  const disabled = careers.find(career => !career.eligible && visibleIds.includes(`native-career-${career.index}`))!;
  expect(disabled).toBeDefined();
  await clickId(page, `native-career-${disabled.index}`);
  expect((await diagnostic(page)).controls).toContainEqual(expect.objectContaining({ id: 'native-yes', enabled: false }));
  await clickId(page, 'native-no');
  expect(await snapshot(page)).toEqual(before);
  // Eligibility belongs to the whole real catalog, not to whichever ten
  // entries happen to occupy the last page of this public-route capture.
  const eligible = expected.find(career => career.eligible)!;
  expect(eligible, 'The public career fixture must contain an eligible career other than the current profession').toBeDefined();
  for (let step = 0; step <= expected.length; step++) {
    const visible = await reachableControls(page, control => /^native-career-\d+$/.test(control.id));
    if (visible.some(control => control.id === `native-career-${eligible.index}`)) break;
    const first = Math.min(...visible.map(control => Number(control.id.slice('native-career-'.length))));
    await clickId(page, eligible.index < first ? 'native-career-scroll-up' : 'native-career-scroll-down');
    expect(await snapshot(page)).toEqual(before);
  }
  expect((await diagnostic(page)).controls).toContainEqual(expect.objectContaining({ id: `native-career-${eligible.index}`, enabled: true }));
  await clickId(page, `native-career-${eligible.index}`); await clickId(page, 'native-yes');
  expect(await snapshot(page)).toEqual(before);
  const accepted = await save(page);
  expect(accepted.state).toEqual(original.state);
  expect(accepted.state.rng).toEqual(original.state.rng);
  await page.keyboard.press('F6'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Game'); await ready(page);
  await clock(page, 10_000); // Original acceptance message precedes the career transition.
  for (let frame = 0; frame < 120 && (await snapshot(page)).life.profession !== eligible.id; frame++) await clock(page, 86);
  await expect.poll(async () => (await snapshot(page)).life.profession).toBe(eligible.id);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 1);
  expect(errors).toEqual([]);
});

test('the equipment workshop restores its intro timer and confirms the real purchase option', async ({ page }, info) => {
  const original = publicFixture('activity-native-equipment-workshop-purchase');
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-native-equipment-workshop-purchase');
  const before = await snapshot(page), view = await native(page);
  expect(view.mode).toBe('purchase');
  expect(view.offered).toBeDefined(); expect(view.known).toHaveLength(18); expect(view.carried).toHaveLength(8);
  const offered = view.offered!, purchase = view.entries!.find(entry => entry.id === offered.id)!;
  expect(purchase, 'The actual offered item must retain its real purchase option').toBeDefined();
  expect(view.options.some(option => option.index === purchase.index && !option.disabled)).toBe(true);
  expect(view.known![offered.id]!.learned).toBe(false);
  const affordable = before.stats.money >= offered.price;
  const expectedOutcome = !affordable ? 'insufficientMoney' : view.known!.filter(item => item.learned).length >= 8 ? 'capacityReached' : 'purchased';
  await clock(page, 5000);
  const saved = await saveReload(page, info, 'workshop-intro-halfway');
  expect(saved.presentation!.profile._nativePanel).toMatchObject({ phase: 'intro', elapsed: 5000 });
  expect(saved.state).toEqual(original.state);
  await clock(page, 5000); // The original intro now completes; description remains a separate phase.
  await clickId(page, 'native-equipment-description');
  expect(await snapshot(page)).toEqual(before);
  expect((await diagnostic(page)).controls).toContainEqual(expect.objectContaining({ id: 'native-yes', label: '购买装备', enabled: true }));
  await clickId(page, 'native-yes');
  await expect.poll(async () => (await native(page)).outcome).toBe(expectedOutcome);
  const expectedKnown = view.known!.map((item, index) => index === offered.id && expectedOutcome === 'purchased' ? { ...item, learned: true } : item);
  expect((await snapshot(page)).stats).toEqual({ ...before.stats, money: before.stats.money - (expectedOutcome === 'purchased' ? offered.price : 0) });
  expect((await native(page)).known).toEqual(expectedKnown);
  expect((await native(page)).carried).toEqual(view.carried);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 1);
  const result = await saveReload(page, info, `workshop-${expectedOutcome}`);
  expect(result.state.knownEquipment).toEqual(expectedKnown);
  expect(result.state.carriedEquipment).toEqual(original.state.carriedEquipment);
  expect(result.state.firstEquipmentVisit).toBe(affordable ? false : original.state.firstEquipmentVisit);
  expect(result.state.rng).toEqual(original.state.rng);
  await clickId(page, 'native-equipment-continue');
  await expect.poll(async () => (await snapshot(page)).nativeView?.kind).not.toBe('equipmentWorkshop');
  expect(errors).toEqual([]);
});

test('housing restores the selected original style and offers a real exit after an unaffordable confirmation', async ({ page }, info) => {
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-native-housing-purchase');
  await clickId(page, 'native-housing-intro');
  await clickId(page, 'native-housing-next');
  expect((await diagnostic(page)).controls.some(control => control.id === 'native-housing-style-1')).toBe(true);
  const before = await snapshot(page);
  await clock(page, 500);
  const selected = await saveReload(page, info, 'housing-selected-style');
  expect(selected.presentation!.profile._nativePanel).toMatchObject({ selection: 1 });
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq);
  await clickId(page, 'native-housing-yes');
  await expect.poll(async () => (await diagnostic(page)).controls.some(control => control.id === 'native-housing-continue')).toBe(true);
  expect((await snapshot(page)).stats.money).toBe(before.stats.money);
  const rejected = await saveReload(page, info, 'housing-insufficient-money');
  expect(String(rejected.presentation!.profile._nativeError)).toContain('持有金');
  await clickId(page, 'native-housing-continue');
  await expect.poll(async () => (await snapshot(page)).nativeView?.kind).not.toBe('housing');
  expect((await snapshot(page)).stats.money).toBe(before.stats.money);
  expect(errors).toEqual([]);
});

test('fortune saves the chosen real answer and explanation timer, then shows a settled result without ticking the next scene', async ({ page }, info) => {
  const original = publicFixture('activity-native-fortune-question');
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-native-fortune-question');
  await clickId(page, 'fortune-intro');
  const before = await snapshot(page), view = await native(page);
  // This saved-result path requires a final type-one question. Validate that
  // actual public fixture rather than assuming a particular record number.
  expect(view.format).toBe(1); expect(view.questionIndex! + 1).toBe(view.questionCount);
  const answers = view.options.filter(option => option.index !== view.exitIndex);
  expect(answers.map(option => option.text)).toEqual(view.currentQuestion!.options);
  const controls = await reachableControls(page, control => /^fortune-option-\d+$/.test(control.id));
  expect(controls).toHaveLength(answers.length);
  for (const option of answers) expect(controls).toContainEqual(expect.objectContaining({ id: `fortune-option-${option.index}`, label: option.text, enabled: true }));
  const answer = answers.find(option => option.index !== 0)!;
  expect(answer, 'The public final-question fixture must have a selectable answer different from the initial cursor').toBeDefined();
  await clickId(page, `fortune-option-${answer.index}`);
  expect(await snapshot(page)).toEqual(before);
  await clock(page, 350);
  const selected = await saveReload(page, info, 'fortune-selected-answer');
  expect(selected.presentation!.profile._nativeFortune).toMatchObject({ phase: 'question', selection: answer.index, elapsed: 350 });
  expect(selected.state).toEqual(original.state);
  await clickId(page, `fortune-option-${answer.index}`); // The original second click confirms.
  await expect.poll(async () => (await native(page)).phase).toBe('explanation');
  expect((await native(page)).selectedAnswer).toBe(answer.index);
  expect((await native(page)).answers).toEqual([...(view.answers ?? []), answer.index]);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 1);
  await clock(page, 20_000);
  expect((await native(page)).phase).toBe('explanation'); // Original comparison is strictly later than 20s.
  const explanation = await saveReload(page, info, 'fortune-explanation-at-deadline');
  expect(explanation.presentation!.profile._nativeFortune).toMatchObject({ phase: 'explanation', elapsed: 20_000 });
  expect(explanation.state.rng).toEqual(original.state.rng);
  await clock(page, 1);
  await expect.poll(async () => (await diagnostic(page)).controls.some(control => control.id === 'fortune-result-continue')).toBe(true);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 2);
  await saveReload(page, info, 'fortune-settled-result');
  const settled = await snapshot(page);
  await clock(page, 9999);
  expect(await snapshot(page)).toEqual(settled);
  expect((await diagnostic(page)).controls.some(control => control.id === 'fortune-result-continue')).toBe(true);
  await clock(page, 1);
  await expect.poll(async () => (await diagnostic(page)).controls.some(control => control.id === 'fortune-result-continue')).toBe(false);
  expect(await snapshot(page)).toEqual(settled);
  expect(errors).toEqual([]);
});

test('school preserves its single-pixel entrance across F6 and submits a displayed answer using the original release key', async ({ page }, info) => {
  const errors = await open(page); await freeze(page); await importScenario(page, publicKind('schoolExam'));
  expect((await native(page)).started).toBe(true);
  for (let frame = 0; frame < 55; frame++) await clock(page, 16);
  const halfway = await saveReload(page, info, 'school-question-slide-halfway');
  expect(halfway.presentation!.profile._nativeSchool).toMatchObject({ phase: 'slide', panelX: 345 });
  for (let frame = 0; frame < 56; frame++) await clock(page, 16);
  const answers = await reachableControls(page, control => /^school-answer-\d+$/.test(control.id));
  expect(answers).toHaveLength(4);
  expect(answers.every(control => control.label.length > 0)).toBe(true);
  await page.locator('#game-canvas').screenshot({ path: info.outputPath('school-readable-question-and-four-answers.png') });
  const before = await snapshot(page), answered = (await native(page)).answered!;
  await page.keyboard.press('d');
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq);
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await native(page)).answered).toBe(answered + 1);
  expect((await snapshot(page)).lastSeq).toBe(before.lastSeq + 1);
  await clock(page, 1);
  const feedback = await saveReload(page, info, 'school-feedback-before-animation');
  expect(feedback.presentation!.profile._nativeSchool).toMatchObject({ phase: 'feedback', answered: answered + 1 });
  expect(errors).toEqual([]);
});

test('growth allocation uses the original preview, nine buttons and default equal split before committing the reward', async ({ page }, info) => {
  const errors = await open(page); await freeze(page); await importScenario(page, publicKind('allocation'));
  const before = await snapshot(page), view = await native(page);
  expect(view.kind).toBe('allocation');
  const pending = before.pending as unknown as { points: number; money: number; defaults: Record<string, number> };
  expect(view.points).toBe(pending.points); expect(view.money).toBe(pending.money);
  await clock(page, 5001);
  expect(await reachableControls(page, control => control.id === 'allocation-auto' || /^allocation-(?:intelligence|morality|charm|stamina)--?1$/.test(control.id))).toHaveLength(9);
  await clickId(page, 'allocation-auto');
  const allocated = await saveReload(page, info, 'growth-equal-split');
  expect(allocated.presentation!.allocations).toEqual(pending.defaults);
  expect((await diagnostic(page)).controls).toContainEqual(expect.objectContaining({ id: 'allocation-confirm', enabled: true }));
  await clickId(page, 'allocation-confirm');
  await expect.poll(async () => (await snapshot(page)).pending?.kind).not.toBe('allocation');
  const after = await snapshot(page);
  expect(after.stats.money).toBe(Math.min(99_999_999, before.stats.money + pending.money));
  for (const field of ['intelligence', 'morality', 'charm', 'stamina']) expect(after.stats[field]).toBe(Math.min(9999, before.stats[field]! + pending.defaults[field]!));
  expect(after.lastSeq).toBe(before.lastSeq + 1);
  expect(errors).toEqual([]);
});

test('hammer starts through its original help control and restores a live game after a current-pointer strike', async ({ page }, info) => {
  publicFixture('activity-minigame-hammer-ready');
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-minigame-hammer-ready');
  expect((await snapshot(page)).pending).toMatchObject({ kind: 'minigame', game: 'hammer', state: { phase: 'ready' } });
  await clickId(page, 'minigame-start');
  await expect.poll(async () => (await snapshot(page)).pending!.state!.phase).toBe('playing');
  await clickLogical(page, 600, 30);
  await expect.poll(async () => (await snapshot(page)).pending!.state!.pointer).toEqual([467, 59]);
  await clock(page, 1001);
  expect((await snapshot(page)).pending!.state!.clockMs).toBeGreaterThanOrEqual(1001);
  expect((await diagnostic(page)).controls.filter(control => /^hammer-hole-\d+$/.test(control.id))).toHaveLength(9);
  await saveReload(page, info, 'hammer-live-strike');
  expect((await snapshot(page)).pending!.state!.phase).toBe('playing');
  expect(errors).toEqual([]);
});

test('tracking reaches the public choose phase through its shuffle and restores before selecting a visible object', async ({ page }, info) => {
  publicFixture('activity-minigame-tracking-memorize');
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-minigame-tracking-memorize');
  expect((await snapshot(page)).pending).toMatchObject({ kind: 'minigame', game: 'tracking', state: { phase: 'memorize' } });
  await clickId(page, 'tracking-next');
  await expect.poll(async () => (await snapshot(page)).pending!.state!.phase).toBe('shuffle');
  await clock(page, 4000);
  expect((await snapshot(page)).pending!.state!.phase).toBe('settle');
  for (let frame = 0; frame < 120 && (await snapshot(page)).pending!.state!.phase !== 'choose'; frame++) await clock(page, 71);
  expect((await snapshot(page)).pending!.state!.phase).toBe('choose');
  expect(await reachableControls(page, control => /^tracking-choose-\d+$/.test(control.id))).toHaveLength(5);
  await saveReload(page, info, 'tracking-before-answer');
  await clickId(page, 'tracking-choose-0');
  await expect.poll(async () => (await snapshot(page)).pending!.state!.phase).toBe('roundResult');
  expect((await snapshot(page)).pending!.state!.selected).toBe(0);
  await page.locator('#game-canvas').screenshot({ path: info.outputPath('tracking-selected-visible-object.png') });
  expect(errors).toEqual([]);
});
