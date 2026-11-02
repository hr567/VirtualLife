import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { control, diagnostic, freeze, importScenario, open, ready, saveSelected } from './helpers';
import { bmp, png } from './images';
test.use({hasTouch:true});

const manifest=JSON.parse(readFileSync('assets/manifest.json','utf8'));
function resource(id:string):Buffer {const sha=manifest.resources[id].sha256;return readFileSync(manifest.objects.find((o:{sha256:string})=>o.sha256===sha).path);}
function consoleErrors(page:Page):string[]{
  const errors:string[]=[];page.on('console',message=>{if(message.type()==='error')errors.push(message.text());});return errors;
}
async function tick(page:Page,ms:number):Promise<void>{
  await ready(page);await page.evaluate(ms=>window.__virtualLifeBevy!.input!({frameMs:ms}),ms);
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));await ready(page);
}
async function finishTransition(page:Page):Promise<void>{
  for(let i=0;i<180;i++){
    if((await diagnostic(page)).controls.some(c=>c.id==='event-continue'))return;
    await tick(page,16);
  }
  throw new Error('Original event transition did not finish');
}
async function originalPortrait(page:Page,id:string,x:number):Promise<void>{
  const bytes=resource(id),source=bmp(bytes),actual=png(await page.locator('#game-canvas').screenshot());
  const offset=bytes.readUInt32LE(10),stride=Math.ceil(source.width/4)*4;
  let checked=0;
  for(let y=0;y<source.height;y++)for(let col=0;col<source.width;col++){
    if(bytes[offset+(source.height-1-y)*stride+col]===1)continue;
    const from=(y*source.width+col)*4,to=((y+304)*640+x+col)*4;
    expect(actual.rgba.subarray(to,to+3)).toEqual(source.rgba.subarray(from,from+3));checked++;
  }
  expect(checked).toBeGreaterThan(100);
}

test('restored N1 portraits use player expressions and dialogue clicks cover the entire original screen',async({page},info)=>{
  const logs=consoleErrors(page);
  await page.setViewportSize({width:640,height:480});const errors=await open(page);await freeze(page);
  await importScenario(page,'n1-dynamic');
  const before=(await diagnostic(page)).snapshot.lastSeq;
  // This point is outside the 166,304,308,112 dialogue rectangle.
  await page.mouse.click(20,20);await ready(page);
  await expect.poll(async()=>(await diagnostic(page)).snapshot.lastSeq).not.toBe(before);
  await finishTransition(page);
  expect(((await diagnostic(page)).snapshot.pending as {dialogueId:number}).dialogueId).toBe(13069);
  await originalPortrait(page,'install/bmp/umf02d.bmp',70);
  await page.screenshot({path:info.outputPath('restored-player-expression.png')});
  await importScenario(page,'n1-expression');
  await originalPortrait(page,'install/bmp/pm208c.bmp',482);
  await page.screenshot({path:info.outputPath('restored-dialogue-expression.png')});
  expect(errors).toEqual([]);
  expect(logs).toEqual([]);
});

for(const id of ['birthday-before','life-notice-10','life-notice-3'])test(`restored ${id} waits for its visible sequence and resumes from save`,async({page},info)=>{
  const logs=consoleErrors(page);
  const errors=await open(page);await freeze(page);
  await importScenario(page,id);await control(page,'继续对话');await ready(page);
  await expect.poll(async()=>(await diagnostic(page)).controls.some(c=>c.label==='继续人生')).toBe(true);
  const seq=(await diagnostic(page)).snapshot.lastSeq;
  await page.screenshot({path:info.outputPath(`restored-${id}.png`)});
  await page.keyboard.press('F6');await ready(page);await saveSelected(page);
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('进度已保存');
  await control(page,'读取选中槽位');await ready(page);
  expect((await diagnostic(page)).controls.some(c=>c.label==='继续人生')).toBe(true);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(seq);
  await control(page,'继续人生');await ready(page);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(seq);
  expect(errors).toEqual([]);
  expect(logs).toEqual([]);
});

// Each original payroll animation advances one visible frame per tick. Give
// each independent save/restore scenario its own budget on CI's software GPU.
for(const id of ['payday-promotion','payday-demotion','payday-mastery'])test(`restored ${id} completes its visible sequence and resumes from save`,async({page},info)=>{
  const logs=consoleErrors(page);
  test.setTimeout(300_000);
  const errors=await open(page);await freeze(page);
  await importScenario(page,id);await control(page,'继续对话');await ready(page);
  const seq=(await diagnostic(page)).snapshot.lastSeq;
  expect((await diagnostic(page)).controls.some(c=>c.label==='掷骰子')).toBe(false);
  await tick(page,16);
  for(let i=0;i<10;i++)await tick(page,25);
  await page.screenshot({path:info.outputPath(`restored-${id}.png`)});
  await page.keyboard.press('F6');await ready(page);await saveSelected(page);
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('进度已保存');
  await control(page,'读取选中槽位');await ready(page);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(seq);
  for(let i=0;i<500;i++){
    if((await diagnostic(page)).controls.some(c=>c.label==='掷骰子'))break;
    await tick(page,1000);
  }
  expect((await diagnostic(page)).controls.some(c=>c.label==='掷骰子')).toBe(true);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(seq);
  expect(errors).toEqual([]);
  expect(logs).toEqual([]);
});

test('restored startup and new-life movies decode, keep scanlines and skip through original Escape input',async({page},info)=>{
  const logs=consoleErrors(page);
  const requests:string[]=[];page.on('request',request=>requests.push(request.url()));
  test.setTimeout(180_000);const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));
  await page.setViewportSize({width:640,height:480});await page.goto('./');
  await expect.poll(async()=>(await diagnostic(page)).controls.some(c=>c.label==='跳过开场影片'),{timeout:90_000}).toBe(true);
  await ready(page,true);await freeze(page);
  await page.screenshot({path:info.outputPath('restored-startup-movie.png')});
  await page.keyboard.press('Escape');await ready(page);
  await control(page,'开始新游戏');await control(page,'确定');await ready(page);await freeze(page);
  expect((await diagnostic(page)).controls.some(c=>c.label==='跳过开场影片')).toBe(true);
  const seq=(await diagnostic(page)).snapshot.lastSeq;
  // Each cue shows its first frame before consuming elapsed time. Prime that
  // frame explicitly because the startup test freezes the presentation clock.
  await tick(page,16);await tick(page,6000);await tick(page,250);await tick(page,501);await tick(page,1);
  const caption=png(await page.locator('#game-canvas').screenshot());let captionPixels=0;
  for(let y=430;y<455;y++)for(let x=90;x<550;x++){
    const at=(y*640+x)*4;if(caption.rgba[at]||caption.rgba[at+1]||caption.rgba[at+2])captionPixels++;
  }
  expect(captionPixels,'Original prologue caption must render in its fixed band').toBeGreaterThan(20);
  await page.screenshot({path:info.outputPath('restored-prologue.png')});
  await page.keyboard.press('F6');await ready(page);await saveSelected(page);
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('进度已保存');
  await control(page,'读取选中槽位');await ready(page);
  expect((await diagnostic(page)).controls.some(c=>c.label==='跳过开场影片')).toBe(true);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(seq);
  for(let i=0;i<22;i++){
    if((await diagnostic(page)).controls.some(c=>c.label==='掷骰子'))break;
    await tick(page,100000);
  }
  expect((await diagnostic(page)).controls.some(c=>c.label==='掷骰子')).toBe(true);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(seq);
  const outroHash=manifest.resources['derived/proepi/aopen03.webm'].sha256;
  const outroPath=manifest.objects.find((object:{sha256:string})=>object.sha256===outroHash).path;
  expect(requests.some(url=>url.endsWith(outroPath)),'Prologue must never load a defeat outro').toBe(false);
  await page.keyboard.press('F10');await ready(page);await control(page,'开始新游戏');await control(page,'确定');await ready(page);
  await page.setViewportSize({width:844,height:390});
  const canvas=await page.locator('#game-canvas').boundingBox();
  await page.touchscreen.tap(canvas!.x+20,canvas!.y+20);await ready(page);
  expect((await diagnostic(page)).controls.some(c=>c.label==='掷骰子')).toBe(true);
  await page.screenshot({path:info.outputPath('restored-map-mobile-landscape.png')});
  expect(errors).toEqual([]);
  expect(logs).toEqual([]);
});
