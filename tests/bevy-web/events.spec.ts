// Uses an unchanged public-route N1 choice save, real keyboard/wheel and mouse.
import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { control, diagnostic, freeze, importScenario, open, ready, scenario } from './helpers';

interface Option { index: number; text: string; nodeId: number }
interface EventSnapshot { pending: { kind: string; options?: Option[] } | null }
async function snapshot(page: Page): Promise<EventSnapshot> { return (await diagnostic(page)).snapshot as unknown as EventSnapshot; }
async function selected(page: Page): Promise<string | undefined> { return (await diagnostic(page)).controls.find(v=>v.id==='event-choice-submit')?.label; }

test('original N1 choice supports cyclic arrows, wheel, Enter and the same real mouse branch',async({page},info)=>{
  const id='activity-event-n1-choice';
  const entries=JSON.parse(readFileSync('output/bevy-scenarios/manifest.json','utf8')).scenarios as {id:string;source:string}[];
  expect(entries.find(v=>v.id===id)?.source).toMatch(/^output\/life-probes\//);
  const envelope=JSON.parse(readFileSync(scenario(id),'utf8')) as {payload:string};
  const original=JSON.parse(envelope.payload).state.pending as {kind:string;options:Option[]};
  expect(original.kind).toBe('choice');expect(original.options.length).toBeGreaterThanOrEqual(2);expect(original.options.every(v=>Number.isInteger(v.nodeId))).toBe(true);
  const errors=await open(page);await freeze(page);await importScenario(page,id);await page.mouse.move(5,5);await ready(page);
  expect((await snapshot(page)).pending?.options).toEqual(original.options);
  await expect.poll(()=>selected(page)).toBe(`确定选择：${original.options[0]!.text}`);
  await page.keyboard.press('ArrowUp');await expect.poll(()=>selected(page)).toBe(`确定选择：${original.options.at(-1)!.text}`);
  await page.keyboard.press('ArrowDown');await expect.poll(()=>selected(page)).toBe(`确定选择：${original.options[0]!.text}`);
  await page.keyboard.press('ArrowDown');await expect.poll(()=>selected(page)).toBe(`确定选择：${original.options[1]!.text}`);
  await page.mouse.wheel(0,-100);await expect.poll(()=>selected(page)).toBe(`确定选择：${original.options[0]!.text}`);
  await page.mouse.wheel(0,100);await expect.poll(()=>selected(page)).toBe(`确定选择：${original.options[1]!.text}`);
  await page.screenshot({path:info.outputPath('n1-choice-selected.png')});
  const sequence=(await diagnostic(page)).snapshot.lastSeq;
  await page.keyboard.press('Enter');await expect.poll(async()=>(await diagnostic(page)).snapshot.lastSeq).not.toBe(sequence);await ready(page);
  const keyboard=(await diagnostic(page)).snapshot;
  await importScenario(page,id);await control(page,original.options[1]!.text);await ready(page);
  expect((await diagnostic(page)).snapshot).toEqual(keyboard);
  expect((await diagnostic(page)).notice).not.toContain('选择无效');expect(errors).toEqual([]);
});
