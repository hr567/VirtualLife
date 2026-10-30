// Presentation-only school-exam contract, recovered from 0043e030/0043f100.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { nativeGdi, indexMaps, validateResources } from './native-ui-gdi.mjs';
const rules=JSON.parse(await readFile('assets/catalog/rules.json','utf8'));
const manifest=JSON.parse(await readFile('assets/manifest.json','utf8'));
const gdi=await nativeGdi(manifest);
if(process.argv.includes('--check')){
  const catalog=JSON.parse(await readFile('assets/catalog/native-school.json','utf8'));
  validateResources(catalog,manifest);
  if(catalog.sourceSha256!==rules.sourceSha256||JSON.stringify(catalog.gdiIndexMaps)!==JSON.stringify(indexMaps(gdi,'school')))throw new Error('School reference/GDI metadata differs');
  console.log(`Verified school metadata and fixed GDI observations: ${catalog.sourceSha256}`);
  process.exit(0);
}
const bytes=await readFile('research/original/N2Game.exe');
const sourceSha256=createHash('sha256').update(bytes).digest('hex');
if(sourceSha256!==rules.sourceSha256)throw new Error('Reference executable mismatch');
const pe=bytes.readUInt32LE(60),count=bytes.readUInt16LE(pe+6),base=bytes.readUInt32LE(pe+52),section=pe+24+bytes.readUInt16LE(pe+20);
function offset(va){const rva=va-base;for(let i=0;i<count;i++){const p=section+i*40,start=bytes.readUInt32LE(p+12);if(rva>=start&&rva<start+Math.max(bytes.readUInt32LE(p+8),bytes.readUInt32LE(p+16)))return bytes.readUInt32LE(p+20)+rva-start;}throw new Error('Unmapped executable address');}
function text(va){const p=offset(va);return new TextDecoder('gbk').decode(bytes.subarray(p,bytes.indexOf(0,p)));}
const ptr=va=>text(bytes.readUInt32LE(offset(va)));
function media(raw,x=0,y=0,transparent='firstPixel'){
  const normalized=raw.replaceAll('\\','/').toLowerCase();const resource=[normalized,`install/${normalized}`,`cd/data/${normalized}`].find(path=>manifest.resources[path]);
  if(!resource)throw new Error(`Missing original school resource ${raw}`);
  const object=manifest.objects.find(o=>o.sha256===manifest.resources[resource].sha256),m=object.media;
  if(m.kind==='fli')throw new Error('School FLI clock requires explicit instruction evidence');
  return {resource,x,y,width:m.width,height:m.height,transparent,...(m.frames?{frameCount:m.frames,interval:m.speedRaw}: {})};
}
function button(raw,x,y){const m=media(raw,x,y,1);return {...m,frameCount:3,frameHeight:m.height/3};}
const catalog={formatVersion:1,sourceSha256,evidence:['0x0043e030','0x0043f100','0x0043ed90','0x0043eac0'],
  background:media(ptr(0x495574),0,0,null),questionPanel:media(ptr(0x495578),289,59,1),scorePanel:media(ptr(0x49557c),289,59,1),
  presenter:media(ptr(0x495580),70,38),presenterEntrance:{x:0,y:3,stepX:8,stepY:4,targetX:70,targetY:38},
  questionEntrance:{x:400,y:59,targetX:289,stepX:-1},
  introText:{x:128,y:351,width:400,height:120,size:16,weight:700,color:[255,255,255,255]},
  questionText:{x:300,y:146,width:240,height:232,size:16,weight:700,color:[0,255,255,255],lineHeight:16},
  answerPrefixes:[0x48dd90,0x48dd88,0x48dd80,0x4959f0].map(text),questionSeparator:text(0x495a14),
  yes:button('public/button/y01.bmp',393,436),no:button('public/button/n01.bmp',458,436),
  selector:media(text(0x48bb88)),correct:media(text(0x4959bc),364,217),wrong:media(text(0x4959b0),364,217),
  win:media(ptr(0x495584),160,120),lost:media(ptr(0x495588),160,120),
  resultAnimations:Array.from({length:4},(_,i)=>{const m=media(ptr(0x49558c+i*4));m.interval=i<2?m.interval*2:Math.trunc(m.interval/2);return m;}),
  music:media(ptr(0x495550)).resource,
  sounds:{correct:media(ptr(0x4955dc)).resource,wrong:media(ptr(0x4955e0)).resource,select:media(ptr(0x4955e4)).resource,confirm:media(ptr(0x4955e8)).resource,step:media(ptr(0x4955ec)).resource,intro:media(ptr(0x4955f4)).resource,question:media(ptr(0x4955f8)).resource,win:media(text(0x4959dc)).resource,lost:media(text(0x4959c8)).resource},
  messages:{intro:ptr(0x495558),invite:ptr(0x49555c),repeat:ptr(0x495560)},
};
// 0043e030 doubles both final win/lost FLC intervals and all four result
// animations, then divides the two terminal result animations by four.
catalog.win.interval*=2;catalog.lost.interval*=2;
// FUN_0043f100 installs the presenter's 163..254 after BMP construction,
// then replaces 163..200 with the yes animation's shared UI colors.
catalog.paletteSections=[{resource:catalog.presenter.resource,first:163,last:254,source:'0x0043f1a0'},
  {resource:media('public/button/yes_a.flc').resource,first:163,last:200,source:'0x0043f1bc'}];
catalog.gdiIndexMaps=indexMaps(gdi,'school');
catalog.gdiObservation={fixture:'tests/fixtures/original-native-ui-gdi.json',method:gdi.method,platform:gdi.platform};
const output=JSON.stringify(catalog,null,2)+'\n';
await writeFile('assets/catalog/native-school.json',output);
console.log(`Verified original school UI: ${sourceSha256}`);
