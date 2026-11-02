import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { control, diagnostic, freeze, importScenario, open, ready, saveSelected, scenario } from './helpers';

interface Record { slot:string; payload:string; checksum:string; savedAt:string; [key:string]:unknown }
async function records(page:Page):Promise<{[slot:string]:Record}> {
  return page.evaluate(()=>new Promise((resolve,reject)=>{const opening=indexedDB.open('virtual-life-bevy',2);opening.onerror=()=>reject(opening.error);opening.onsuccess=()=>{const db=opening.result,tx=db.transaction('saves'),reading=tx.objectStore('saves').getAll();reading.onsuccess=()=>resolve(Object.fromEntries(reading.result.map(record=>[record.slot,record])));reading.onerror=()=>reject(reading.error);tx.oncomplete=()=>db.close();};}));
}
async function upload(page:Page,slot:number,buffer:Buffer):Promise<void> {
  const picking=page.waitForEvent('filechooser');await control(page,'导入槽位 '+slot);
  await(await picking).setFiles({name:'roundtrip.vlsave.json',mimeType:'application/json',buffer});
}

test('all nine positions import, export and reload; imports leave the current game unchanged',async({page},info)=>{
  const errors=await open(page);await freeze(page);await importScenario(page,'map-seed-1');
  const before=(await diagnostic(page)).snapshot;await page.keyboard.press('F6');
  const fixture=readFileSync(scenario('map-seed-4'));
  await expect.poll(async()=>(await diagnostic(page)).controls.filter(c=>c.label.startsWith('导入槽位 ')).length).toBe(9);
  for(let slot=0;slot<9;slot++){
    await upload(page,slot,fixture);
    await expect.poll(async()=>{const state=await diagnostic(page);return state.notice.includes('存档已导入')||state.controls.some(c=>c.label==='确定覆盖'&&c.enabled);}).toBe(true);
    if((await diagnostic(page)).controls.some(c=>c.label==='确定覆盖'&&c.enabled))await control(page,'确定覆盖');
    await expect.poll(async()=>(await records(page))['slot-'+slot]?.payload).toBe(JSON.parse(fixture.toString()).payload);
    expect((await diagnostic(page)).screen).toBe('Saves');expect((await diagnostic(page)).snapshot).toEqual(before);
    const downloading=page.waitForEvent('download');await control(page,'导出槽位 '+slot);const download=await downloading;
    expect(download.suggestedFilename()).toContain('槽位'+slot);
    const stream=await download.createReadStream();const chunks:Buffer[]=[];for await(const chunk of stream!)chunks.push(chunk);
    const exported=JSON.parse(Buffer.concat(chunks).toString());expect(exported).toEqual(JSON.parse(fixture.toString()));expect(exported.slot).toBeUndefined();
  }
  await control(page,'选择槽位 0');expect((await diagnostic(page)).controls.find(c=>c.label==='保存选中槽位')?.enabled).toBe(false);
  await page.screenshot({path:info.outputPath('nine-save-slots-desktop.png')});
  await page.setViewportSize({width:390,height:844});await page.screenshot({path:info.outputPath('nine-save-slots-portrait.png')});
  await control(page,'选择槽位 8');await control(page,'读取选中槽位');await ready(page);
  expect((await diagnostic(page)).snapshot.player).toMatchObject({name:'原版回归4'});
  await page.reload();await ready(page);await control(page,'继续游戏');await control(page,'选择槽位 8');await control(page,'读取选中槽位');await ready(page);
  expect((await diagnostic(page)).snapshot.player).toMatchObject({name:'原版回归4'});expect(errors).toEqual([]);
});

for(const controlled of [false,true])test(`${controlled?'controlled':'normal'} dice save the pre-roll random stream to slot zero only`,async({page})=>{
  await open(page);await freeze(page);await importScenario(page,controlled?'controlled-dice':'map-seed-1');
  const before=await records(page),state=JSON.parse(before['slot-1']!.payload).state;
  await control(page,'掷骰子');
  if(controlled){expect((await records(page))['slot-0']).toBeUndefined();await control(page,'选择 3 点');}
  await expect.poll(async()=>!!(await records(page))['slot-0']).toBe(true);
  const saved=await records(page);expect(JSON.parse(saved['slot-0']!.payload).state).toEqual(state);expect(saved['slot-1']).toEqual(before['slot-1']);
  await expect.poll(async()=>(await diagnostic(page)).snapshot.lastSeq).not.toBe(state.lastSeq);
  await page.keyboard.press('F10');await control(page,'继续游戏');await control(page,'选择槽位 0');await control(page,'读取选中槽位');await ready(page);
  expect((await diagnostic(page)).snapshot.lastSeq).toBe(state.lastSeq);
  const first=saved['slot-0']!;await control(page,'掷骰子');if(controlled)await control(page,'选择 3 点');
  await expect.poll(async()=>(await records(page))['slot-0']!.savedAt).not.toBe(first.savedAt);
  expect((await records(page))['slot-1']).toEqual(before['slot-1']);
});

test('overwrite cancellation and failed imports retain the existing record and live game',async({page},info)=>{
  await open(page);await freeze(page);await importScenario(page,'map-seed-1');const before=(await records(page))['slot-1']!;
  await page.keyboard.press('F6');await upload(page,1,readFileSync(scenario('map-seed-4')));await control(page,'取消');expect((await records(page))['slot-1']).toEqual(before);
  await control(page,'保存选中槽位');await control(page,'取消');expect((await records(page))['slot-1']).toEqual(before);
  const cancel=page.waitForEvent('filechooser');await control(page,'导入槽位 1');await(await cancel).setFiles([]);
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('已取消');expect((await records(page))['slot-1']).toEqual(before);
  const bad={...before,payload:'{}'};await upload(page,1,Buffer.from(JSON.stringify(bad)));await expect.poll(async()=>(await diagnostic(page)).notice).toContain('校验失败');
  const malformed=JSON.stringify({...JSON.parse(before.payload),presentation:{profile:'invalid'}});
  await upload(page,1,Buffer.from(JSON.stringify({...before,payload:malformed,checksum:createHash('sha256').update(malformed).digest('hex')})));
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('场景状态');
  await upload(page,1,Buffer.from(JSON.stringify({...bad,checksum:createHash('sha256').update('{}').digest('hex')})));
  await expect.poll(async()=>(await diagnostic(page)).notice.length).toBeGreaterThan(0);
  await upload(page,1,Buffer.alloc(8*1024*1024+1,32));await expect.poll(async()=>(await diagnostic(page)).notice).toContain('8 MiB');
  expect((await records(page))['slot-1']).toEqual(before);expect((await diagnostic(page)).snapshot.player).toMatchObject({name:'原版回归1'});
  await page.screenshot({path:info.outputPath('save-import-error.png')});
  await saveSelected(page);
});

test('failed pre-roll writes retain slot zero and continue exactly one roll',async({page},info)=>{
  const errors=await open(page);await freeze(page);await importScenario(page,'map-seed-1',0);
  const before=await records(page),seq=(await diagnostic(page)).snapshot.lastSeq;
  await page.evaluate(()=>{const put=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(value:unknown,...rest:unknown[]){if((value as {slot?:string})?.slot==='slot-0')throw new DOMException('full','QuotaExceededError');return Reflect.apply(put,this,[value,...rest]);};});
  await control(page,'掷骰子');
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('自动存档失败');
  await expect.poll(async()=>(await diagnostic(page)).snapshot.lastSeq).toBe(Number(seq)+1);
  expect((await records(page))['slot-0']).toEqual(before['slot-0']);
  await page.screenshot({path:info.outputPath('autosave-failure-continues.png')});expect(errors).toEqual([]);
});

test('another page changing a manual slot prevents a stale overwrite',async({page})=>{
  await open(page);await freeze(page);await importScenario(page,'map-seed-1');await page.keyboard.press('F6');
  await expect.poll(async()=>(await diagnostic(page)).controls.find(c=>c.label==='保存选中槽位')?.enabled).toBe(true);
  const before=(await records(page))['slot-1'];
  await page.evaluate(()=>new Promise<void>((resolve,reject)=>{const opening=indexedDB.open('virtual-life-bevy',2);opening.onsuccess=()=>{const db=opening.result,tx=db.transaction('saves','readwrite'),store=tx.objectStore('saves'),reading=store.get('slot-1');reading.onsuccess=()=>store.put({...reading.result,savedAt:'2040-01-01T00:00:00.000Z'});tx.oncomplete=()=>{db.close();resolve();};tx.onerror=()=>reject(tx.error);};}));
  await control(page,'保存选中槽位');await control(page,'确定覆盖');
  await expect.poll(async()=>(await diagnostic(page)).notice).toContain('其他页面修改');
  expect((await records(page))['slot-1']).toEqual({...before,savedAt:'2040-01-01T00:00:00.000Z'});
});

test('system save and load use the shared slot list and return through options to the game',async({page})=>{
  await open(page);await page.keyboard.press('F6');
  // Wait for F6 to leave the ready menu before checking the overlay's assets.
  await expect.poll(async()=>(await diagnostic(page)).screen).toBe('Saves');
  await ready(page);
  const back=(await diagnostic(page)).controls.find(c=>c.label==='返回游戏')!,canvas=(await page.locator('#game-canvas').boundingBox())!;
  await page.mouse.move(canvas.x+(back.bounds.x+back.bounds.width/2)*canvas.width/640,canvas.y+(back.bounds.y+back.bounds.height/2)*canvas.height/480);
  await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  expect((await diagnostic(page)).loading).toBe(false);
  await control(page,'返回游戏');await expect.poll(async()=>(await diagnostic(page)).screen).toBe('Menu');
  await freeze(page);await importScenario(page,'map-seed-1');
  for(const action of ['保存游戏','读取游戏']){
    await control(page,'系统操作');await control(page,action);
    await expect.poll(async()=>(await diagnostic(page)).screen).toBe('Saves');
    await control(page,'返回游戏');await expect.poll(async()=>(await diagnostic(page)).screen).toBe('Options');
    await control(page,'返回');await expect.poll(async()=>(await diagnostic(page)).screen).toBe('Game');
  }
});

test('version one migration is atomic and retains both original payloads',async({page})=>{
  await page.goto('assets/catalog/maps.json');
  const source=readFileSync('web/platform.js','utf8'),fixture=JSON.parse(readFileSync(scenario('map-seed-1'),'utf8'));
  const result=await page.evaluate(async({source,fixture})=>{
    const {SaveStorage}=await import(URL.createObjectURL(new Blob([source],{type:'application/javascript'})));
    await new Promise<void>((resolve,reject)=>{const opening=indexedDB.open('virtual-life-bevy',1);opening.onupgradeneeded=()=>{opening.result.createObjectStore('saves',{keyPath:'slot'});opening.result.createObjectStore('settings');};opening.onerror=()=>reject(opening.error);opening.onsuccess=()=>{const db=opening.result,tx=db.transaction('saves','readwrite'),store=tx.objectStore('saves');store.put({...fixture,slot:'current'});store.put({...fixture,slot:'previous'});tx.oncomplete=()=>{db.close();resolve();};};});
    const original=IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put=function(value:unknown,...rest:unknown[]){if((value as {slot:string}).slot==='slot-2')throw new DOMException('full','QuotaExceededError');return Reflect.apply(original,this,[value,...rest]);};
    let failed=false;try{await SaveStorage.open();}catch{failed=true;}finally{IDBObjectStore.prototype.put=original;}
    const old=await new Promise<{version:number;keys:IDBValidKey[]}>((resolve,reject)=>{const opening=indexedDB.open('virtual-life-bevy',1);opening.onerror=()=>reject(opening.error);opening.onsuccess=()=>{const db=opening.result,reading=db.transaction('saves').objectStore('saves').getAllKeys();reading.onsuccess=()=>{resolve({version:db.version,keys:reading.result});db.close();};};});
    const storage=await SaveStorage.open();const records=[await storage.load('slot-1'),await storage.load('slot-2'),await storage.load('slot-0')];storage.database.close();return {failed,old,records};
  },{source,fixture});
  expect(result.failed).toBe(true);expect(result.old).toEqual({version:1,keys:['current','previous']});
  expect(result.records[0]).toEqual({...fixture,slot:'slot-1'});expect(result.records[1]).toEqual({...fixture,slot:'slot-2'});expect(result.records[2]).toBeNull();
});
