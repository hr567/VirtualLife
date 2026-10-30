// Real canvas/keyboard commands from unmodified public-route saves.
import { test, expect, type Page } from '@playwright/test';
import { diagnostic, freeze, importScenario, open, ready } from './helpers';

interface BattleSnapshot { pending: { kind: string; game?: string; state?: { phase: string; help: boolean; helpPage: number; bonusKind: number; bonus?: { valuePhase: number }; reward?: { valuePhase: number } } } | null }
async function snapshot(page: Page): Promise<BattleSnapshot> { return (await diagnostic(page)).snapshot as unknown as BattleSnapshot; }
async function frame(page: Page, milliseconds=16): Promise<void> {
  await page.evaluate(frameMs => window.__virtualLifeBevy!.input!({ frameMs }),milliseconds);
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await ready(page);
}

test('ordinary battle F1 opens help and Enter closes all nine pages through the core', async ({ page }) => {
  const errors=await open(page); await freeze(page); await importScenario(page,'activity-battle-ordinary-playing');
  expect((await snapshot(page)).pending).toMatchObject({kind:'battle',game:'ordinary',state:{phase:'playing',help:false}});
  await page.keyboard.press('F1');
  await expect.poll(async()=>(await snapshot(page)).pending?.state?.help).toBe(true);
  for(let pageIndex=1;pageIndex<=9;pageIndex++){
    await page.keyboard.press('Enter');
    await expect.poll(async()=>(await snapshot(page)).pending?.state?.helpPage).toBe(pageIndex);
  }
  expect((await snapshot(page)).pending).toMatchObject({kind:'battle',game:'ordinary',state:{phase:'playing',help:false}});
  expect((await diagnostic(page)).notice).not.toContain('未知的战斗操作'); expect(errors).toEqual([]);
});

test('ordinary battle reward canvas skip completes every original reward panel', async ({ page }) => {
  const errors=await open(page); await freeze(page); await importScenario(page,'activity-battle-ordinary-reward');
  expect((await snapshot(page)).pending).toMatchObject({kind:'battle',game:'ordinary',state:{phase:'reward'}});
  let skipped=0;
  for(let guard=0;guard<8;guard++){
    const pending=(await snapshot(page)).pending;
    if(pending?.game!=='ordinary'||pending.state?.phase!=='reward')break;
    // The original skip is effective only after the introductory value phase.
    for(let init=0;init<3&&(await snapshot(page)).pending?.state?.bonus?.valuePhase!==2;init++)await frame(page,1601);
    expect((await snapshot(page)).pending?.state?.bonus?.valuePhase).toBe(2);
    const before=(await diagnostic(page)).snapshot.lastSeq;
    // Exercise the original Escape input rather than only the canvas alias.
    await page.keyboard.press('Escape'); skipped++;
    await expect.poll(async()=>(await diagnostic(page)).snapshot.lastSeq).not.toBe(before);
    await frame(page);
    expect((await snapshot(page)).pending?.state?.bonus?.valuePhase).toBe(3);
    await frame(page,1501);
  }
  expect(skipped).toBeGreaterThan(0);
  const pending=(await snapshot(page)).pending;
  expect(pending?.game!=='ordinary'||pending.state?.phase!=='reward').toBe(true);
  expect((await diagnostic(page)).notice).not.toContain('未知的战斗操作'); expect(errors).toEqual([]);
});

test('original right-click skips the gas minigame reward counting through the core',async({page})=>{
  const errors=await open(page);await freeze(page);await importScenario(page,'activity-minigame-gas-reward');
  expect((await snapshot(page)).pending).toMatchObject({kind:'minigame',game:'gas',state:{phase:'reward'}});
  let skipped=0;
  for(let guard=0;guard<8;guard++){
    const pending=(await snapshot(page)).pending;if(pending?.game!=='gas'||pending.state?.phase!=='reward')break;
    for(let init=0;init<3&&(await snapshot(page)).pending?.state?.reward?.valuePhase!==2;init++)await frame(page,1601);
    expect((await snapshot(page)).pending?.state?.reward?.valuePhase).toBe(2);
    const before=(await diagnostic(page)).snapshot.lastSeq;
    const canvas=await page.locator('#game-canvas').boundingBox();expect(canvas).not.toBeNull();
    await page.mouse.click(canvas!.x+canvas!.width/2,canvas!.y+canvas!.height/2,{button:'right'});
    await expect.poll(async()=>(await diagnostic(page)).snapshot.lastSeq).not.toBe(before);
    await frame(page);expect((await snapshot(page)).pending?.state?.reward?.valuePhase).toBe(3);
    await frame(page,1501);skipped++;
  }
  expect(skipped).toBeGreaterThan(0);const pending=(await snapshot(page)).pending;
  expect(pending?.game!=='gas'||pending.state?.phase!=='reward').toBe(true);expect(errors).toEqual([]);
});
