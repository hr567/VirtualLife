// Validate the fixed Windows GDI observation using only committed assets.
// Re-observation is optional and lives in export-native-ui-gdi.py.
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
export const referenceSha256 = 'e725c9c7dd9de766cd1bf66dba18260858e447980c65efbd292a9cd3687ef4c0';
const hash = value => createHash('sha256').update(value).digest('hex');
const fnv32 = bytes => { let result=2166136261; for(const v of bytes)result=Math.imul(result^v,16777619)>>>0; return result; };
export async function nativeGdi(manifest) {
  const fixture=JSON.parse(await readFile('tests/fixtures/original-native-ui-gdi.json','utf8'));
  if(fixture.sourceSha256!==referenceSha256)throw new Error('Native GDI reference identity changed');
  const objects=new Map(manifest.objects.map(o=>[o.sha256,o]));
  for(const [kind,context]of Object.entries(fixture.contexts)) {
    const palette=Buffer.from(context.targetPalette);
    if(palette.length!==1024||hash(palette)!==context.targetPaletteSha256)throw new Error(`Invalid ${kind} construction palette`);
    for(const [resource,row]of Object.entries(context.images)) {
      const sha=manifest.resources[resource]?.sha256,object=objects.get(sha);
      if(!object||sha!==row.sourceSha256)throw new Error(`Native GDI resource changed: ${resource}`);
      const raw=await readFile(object.path);
      if(hash(raw)!==sha||raw.toString('ascii',0,2)!=='BM'||raw.readUInt16LE(28)!==8||raw.readUInt32LE(30)!==0)throw new Error(`Invalid GDI source BMP: ${resource}`);
      const width=raw.readInt32LE(18),height=raw.readInt32LE(22),pitch=(width+3)&~3,start=raw.readUInt32LE(10);
      if(width!==row.width||height!==row.height||height<=0||row.destinationColorKey!==1)throw new Error(`Invalid native GDI dimensions: ${resource}`);
      const map=new Map(row.indexMap),indices=Buffer.alloc(width*height),rgba=Buffer.alloc(indices.length*4),black=Buffer.alloc(rgba.length);
      if(map.size!==row.indexMap.length||[...map].some(([a,b])=>a<0||a>255||b<0||b>255))throw new Error(`Invalid native GDI map: ${resource}`);
      for(let y=0;y<height;y++)for(let x=0;x<width;x++) {
        const i=y*width+x,original=raw[start+(height-y-1)*pitch+x],mapped=map.get(original);
        if(mapped===undefined)throw new Error(`Unobserved native BMP index: ${resource}/${original}`);
        indices[i]=mapped;palette.copy(rgba,i*4,mapped*4,mapped*4+4);rgba[i*4+3]=mapped===1?0:255;
        if(mapped!==1)palette.copy(black,i*4,mapped*4,mapped*4+4);black[i*4+3]=255;
      }
      if(hash(indices)!==row.mappedIndicesSha256||hash(rgba)!==row.rgbaSha256||fnv32(black)!==row.colorKeyedBlackRgbaFnv32)throw new Error(`Native GDI full-image observation differs: ${resource}`);
    }
  }
  return fixture;
}
export function indexMaps(fixture,kind) {
  const contexts=kind==='equipmentWorkshop'?[fixture.contexts.equipmentWorkshop,fixture.contexts.equipmentResearch]:[fixture.contexts[kind]];
  return Object.assign({},...contexts.map(context=>Object.fromEntries(Object.entries(context.images).map(([resource,row])=>[resource,row.indexMap]))));
}
export function validateResources(catalog,manifest) {
  if(catalog.sourceSha256!==referenceSha256)throw new Error('Native UI reference identity changed');
  const walk=value=>{
    if(Array.isArray(value))value.forEach(walk);
    else if(value&&typeof value==='object'){
      if(typeof value.resource==='string'&&!manifest.resources[value.resource])throw new Error(`Unknown native UI artwork: ${value.resource}`);
      Object.values(value).forEach(walk);
    }
  };walk(catalog);
}
