// Original 00420250 static initializer and 00420840/4205f0/4206a0 draw calls.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const binary=await readFile('research/original/N2Game.exe');
const manifest=JSON.parse(await readFile('assets/manifest.json','utf8'));
const rules=JSON.parse(await readFile('assets/catalog/rules.json','utf8'));
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
if(sha(binary)!==rules.sourceSha256)throw new Error('Original executable hash differs');
const pe=binary.readUInt32LE(60),table=pe+24+binary.readUInt16LE(pe+20),base=binary.readUInt32LE(pe+52);
function offset(va){const rva=va-base;for(let i=0;i<binary.readUInt16LE(pe+6);i++){const p=table+i*40,s=binary.readUInt32LE(p+12);if(rva>=s&&rva<s+binary.readUInt32LE(p+16))return binary.readUInt32LE(p+20)+rva-s;}throw new Error(`Unmapped address ${va.toString(16)}`);}
const i32=va=>binary.readInt32LE(offset(va));
function string(va){const p=offset(va);return new TextDecoder('gbk').decode(binary.subarray(p,binary.indexOf(0,p)));}
function media(raw,x,y){const id=`install/${raw.replaceAll('\\','/').toLowerCase()}`,hash=manifest.resources[id]?.sha256;const m=manifest.objects.find(o=>o.sha256===hash)?.media;if(!m)throw new Error(`Missing original resource ${id}`);return{resource:id,x,y,width:m.width,height:m.height,transparent:1,...(m.frames?{frameCount:m.frames,interval:m.speedRaw}:{})};}
const preview=media(string(0x47a064),i32(0x47a050),i32(0x47a054));
const allocation=media(string(0x47a074),i32(0x47a058),i32(0x47a05c));
function ok(x,y){return{...media(string(0x47a0a8),x,y),hover:media(string(0x47a0b8),x,y).resource};}
preview.ok=ok(preview.x+130,preview.y+228);allocation.ok=ok(allocation.x+159,allocation.y+226);
const button=(address,x,y)=>({...media(`bmp/${string(address)}`,x,y),frameCount:3});
const buttons=[button(0x47a090,allocation.x+45,allocation.y+226)];
for(const [address,x]of[[0x47a09c,71],[0x47a084,191]])for(let row=0;row<4;row++)buttons.push(button(address,allocation.x+x,allocation.y+62+row*38));
const result={formatVersion:1,sourceSha256:sha(binary),evidence:{functions:['0x00420250','0x00420840','0x004205f0','0x004206a0','0x00420730','0x0040e840','0x0040e880'],initializerSha256:sha(binary.subarray(offset(0x420250),offset(0x420333))),previewTimeoutMs:5001,font:'original ASCII bitmap, 8x16, palette index 0; centered in ten characters'},preview,allocation,buttons};
const target='assets/catalog/native-allocation.json',output=JSON.stringify(result,null,2)+'\n';
if(process.argv.includes('--check')){if(await readFile(target,'utf8')!==output)throw new Error('Native allocation metadata differs');}else await writeFile(target,output);
console.log(`Original allocation metadata ${process.argv.includes('--check')?'verified':'exported'}.`);
