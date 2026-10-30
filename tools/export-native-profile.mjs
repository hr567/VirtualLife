// Presentation-only export of the original 00421490 character creator.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const bytes=await readFile('research/original/N2Game.exe');
const sourceSha256=createHash('sha256').update(bytes).digest('hex');
if(sourceSha256!=='e725c9c7dd9de766cd1bf66dba18260858e447980c65efbd292a9cd3687ef4c0')throw new Error('Wrong original executable');
const manifest=JSON.parse(await readFile('assets/manifest.json','utf8'));
const pe=bytes.readUInt32LE(60),base=bytes.readUInt32LE(pe+52),sections=bytes.readUInt16LE(pe+6),table=pe+24+bytes.readUInt16LE(pe+20);
function offset(va){const rva=va-base;for(let i=0;i<sections;i++){const p=table+i*40,start=bytes.readUInt32LE(p+12);if(rva>=start&&rva<start+Math.max(bytes.readUInt32LE(p+8),bytes.readUInt32LE(p+16)))return bytes.readUInt32LE(p+20)+rva-start;}throw new Error(`Unmapped ${va.toString(16)}`);}
const i32=va=>bytes.readInt32LE(offset(va));
function string(va){const p=offset(va);return new TextDecoder('gbk').decode(bytes.subarray(p,bytes.indexOf(0,p)));}
function resource(raw){raw=raw.replaceAll('\\','/').toLowerCase();const id=[raw,`install/${raw}`,`cd/data/${raw}`,`cd/${raw}`].find(id=>manifest.resources[id]);if(!id)throw new Error(`Missing original profile resource ${raw}`);return id;}
function media(raw,x=0,y=0,transparent=1){const id=resource(raw);const obj=manifest.objects.find(v=>v.sha256===manifest.resources[id].sha256),m=obj.media;return {resource:id,x,y,width:m.width,height:m.height,transparent,...(m.frames?{frameCount:m.frames,interval:m.speedRaw}:{})};}
function strip(raw,x,y){const v=media(raw,x,y);return {...v,frameCount:3,frameHeight:v.height/3};}
const c=await readFile('research/decompiled/functions/00421490.c','utf8');
// The compiler builds its 12 zodiac records on the stack. Reconstruct the
// literal stores from the fixed decompile, retaining their original date
// ranges; no browser/calendar approximation or guessed month-to-image map.
const stack=Buffer.alloc(0x400),sizes=new Map();
for(const m of c.matchAll(/\b(undefined[124]|int|uint|DWORD|short|char)\s+(local_([0-9a-f]+))(?:\s*\[(\d+)\])?;/g))sizes.set(m[2],({undefined1:1,undefined2:2,undefined4:4,int:4,uint:4,DWORD:4,short:2,char:1}[m[1]])*(m[4]?Number(m[4]):1));
for(const m of c.matchAll(/^\s*(local_([0-9a-f]+))\s*=\s*(?:\([^;\n]*?\))?\s*(?:DAT_([0-9a-f]{8})|(0x[0-9a-f]+|\d+));/gm)){
 const size=sizes.get(m[1]);if(!size||size>8)continue;
 const p=0x400-parseInt(m[2],16);if(p<0||p+size>stack.length)continue;
 if(m[3])bytes.copy(stack,p,offset(parseInt(m[3],16)),offset(parseInt(m[3],16))+size);
 else {const value=Number(m[4]);if(size===4)stack.writeUInt32LE(value>>>0,p);else if(size===2)stack.writeUInt16LE(value&65535,p);else if(size===1)stack[p]=value&255;}
}
const zodiac=Array.from({length:12},(_,index)=>{const p=0x400-0x2ee+index*48;const raw=new TextDecoder('gbk').decode(stack.subarray(p,stack.indexOf(0,p)));const ranges=[22,26,30,34].map(n=>stack.readInt32LE(p+n));if(ranges.some((v,i)=>v<1||v>(i%2?31:12)))throw new Error(`Invalid original zodiac row ${index}`);return {index,...media(raw,370,96,null),startMonth:ranges[0],startDay:ranges[1],endMonth:ranges[2],endDay:ranges[3]};});
const rows=[['name','姓名','bmp/pm0301c.bmp',139,91],['male','男性','bmp/pm0301a.bmp',120,348],['female','女性','bmp/pm0301b.bmp',120,391],['month-next','下一月份','bmp/pm0402.bmp',344,125],['month-previous','上一月份','bmp/pm0403.bmp',344,143],['day-next','下一日期','bmp/pm0402.bmp',344,175],['day-previous','上一日期','bmp/pm0403.bmp',344,193],['blood-previous','上一血型','bmp/pm0404.bmp',287,303],['blood-next','下一血型','bmp/pm0405.bmp',467,303]];
const output={formatVersion:1,sourceSha256,evidence:['0x00421490','0x00422413','0x00422433','0x00422453','0x0040e580','0x00420d90'],background:media(string(0x47a410),0,0,null),buttons:rows.map(([id,label,file,x,y])=>({id,label,...strip(file,x,y)})),actors:[media(string(0x47a210),15,151),media(string(0x47a200),15,151)].map(v=>({...v,interval:100,source:'0x0042206c: FUN_00458750(actor,100)'})),confirm:[media(string(0x47a1f0),480,416),media(string(0x47a1e0),480,416)],cancel:[media(string(0x47a1cc),545,416),media(string(0x47a1b8),545,416)],blood:[0x47a2a0,0x47a290,0x47a280,0x47a270].map((va,index)=>({index,...media(string(va),353,282)})),zodiac,monthDays:Array.from({length:12},(_,i)=>i32(0x47a0c8+i*4)),text:{colorIndex:31,font:'original ascfont.16 / stdfont.16',month:{x:296,y:135},day:{x:296,y:184},name:{x:92,y:128,capacityBytes:11}},defaultName:string(0x476804),gdiObservation:null};
const observed=JSON.parse(await readFile('assets/catalog/native-profile-gdi.json','utf8'));
if(observed.sourceSha256!==sourceSha256||createHash('sha256').update(Buffer.from(observed.targetPalette)).digest('hex')!==observed.targetPaletteSha256)throw new Error('Wrong profile GDI observation');
for(const [id,row] of Object.entries(observed.images))if(manifest.resources[id]?.sha256!==row.sourceSha256)throw new Error(`Profile GDI source changed: ${id}`);
output.blood.forEach((row,i)=>{row.bloodType=['A','AB','B','O'][i];});
output.actors.forEach(row=>{row.source='0x00422086: FUN_00458750(actor,100)';});
output.defaults={name:output.defaultName,sex:'male',birthMonth:1,birthDay:1,bloodType:'A',evidence:['0x00421607','0x00422133','0x00422144','0x00422148','0x0042214c','0x00422150']};
output.hoverSound=resource(string(0x47a10c));
output.clickSound=resource(string(0x47a0fc));
output.gdiIndexMaps=Object.fromEntries(Object.entries(observed.images).map(([id,row])=>[id,row.indexMap]));
output.gdiObservation=observed;
const serialized=`${JSON.stringify(output,null,2)}\n`;
if(process.argv.includes('--check')){if(await readFile('assets/catalog/native-profile.json','utf8')!==serialized)throw new Error('Original profile metadata differs');}
else await writeFile('assets/catalog/native-profile.json',serialized);
console.log(JSON.stringify({buttons:output.buttons.length,zodiac:output.zodiac.map(v=>[v.startMonth,v.startDay,v.endMonth,v.endDay]),monthDays:output.monthDays,defaultName:output.defaultName,bloodResources:output.blood.map(v=>({resource:v.resource,path:manifest.objects.find(o=>o.sha256===manifest.resources[v.resource].sha256).path}))},null,2));
