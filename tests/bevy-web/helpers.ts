import { expect, type Page } from '@playwright/test';
import { readdirSync, existsSync } from 'node:fs';
import path from 'node:path';

export interface Control { id: string; label: string; bounds: { x: number; y: number; width: number; height: number }; enabled: boolean }
export interface Diagnostic { screen: string; loading: boolean; error: string | null; notice: string; controls: Control[]; snapshot: Record<string, unknown> }
declare global { interface Window { __virtualLifeBevy?: { readonly diagnostics: Diagnostic; input?: (value: { controlId?: string; frameMs?: number }) => void } } }

export async function diagnostic(page: Page): Promise<Diagnostic> {
  return page.evaluate(() => window.__virtualLifeBevy?.diagnostics ?? { screen: 'Boot', loading: true, error: null, notice: '', controls: [], snapshot: {} });
}
export async function ready(page: Page): Promise<void> {
  await expect.poll(async () => (await diagnostic(page))?.loading, { timeout: 90_000 }).toBe(false);
  expect((await diagnostic(page)).error).toBeNull();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
export async function control(page: Page, label: string): Promise<void> {
  await ready(page);
  const selected = (await diagnostic(page)).controls.find(control => control.label === label && control.enabled);
  expect(selected, `Missing enabled Bevy control: ${label}`).toBeDefined();
  const canvas = await page.locator('#game-canvas').boundingBox();
  expect(canvas).not.toBeNull();
  await page.mouse.move(canvas!.x + (selected!.bounds.x + selected!.bounds.width / 2) * canvas!.width / 640, canvas!.y + (selected!.bounds.y + selected!.bounds.height / 2) * canvas!.height / 480);
  await page.mouse.down();
  // Bevy samples pointer edges in PreUpdate. Keep the real press through a
  // rendered frame before releasing, including with the CI software adapter.
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await page.mouse.up();
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
export async function open(page: Page): Promise<string[]> {
  const errors: string[] = []; page.on('pageerror', error => errors.push(error.message));
  await page.goto('./');
  await expect.poll(async () => (await diagnostic(page))?.screen, { timeout: 90_000 }).toBe('Menu');
  await ready(page);
  expect(await page.title()).toBe('虚拟人生');
  await expect(page.locator('#boot-status')).toBeHidden();
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  return errors;
}
export async function freeze(page: Page): Promise<void> {
  await page.evaluate(() => window.__virtualLifeBevy!.input!({ frameMs: 0 }));
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}
export async function create(page: Page): Promise<void> {
  await control(page, '开始新游戏'); await ready(page);
  await control(page, '姓名');
  await page.getByRole('textbox', { name: '角色姓名' }).fill('原版回归');
  await control(page, '确定');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Game'); await ready(page);
}
export function scenario(name: string): string {
  const directory = 'output/bevy-scenarios';
  if (!existsSync(directory)) throw new Error('Run pnpm test:fixtures first');
  const file = readdirSync(directory).find(file => file === `${name}.json` || file === `browser-${name}.json` || file === `${name}.vlsave.json`);
  if (!file) throw new Error(`Missing fresh Bevy test scenario: ${name}`);
  return path.resolve(directory, file);
}
export async function importScenario(page: Page, name: string): Promise<void> {
  await page.keyboard.press('F6');
  await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
  const picking = page.waitForEvent('filechooser');
  await control(page, '导入存档');
  await (await picking).setFiles(scenario(name));
  await expect.poll(async () => (await diagnostic(page)).screen, { timeout: 30_000 }).toBe('Game');
  await ready(page);
}
