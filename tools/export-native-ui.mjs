// Rebuild presentation-only metadata from the fixed local reference executable.
// No domain rule catalog or content identity is changed by this exporter.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { nativeGdi, indexMaps, validateResources } from './native-ui-gdi.mjs';
const rules = JSON.parse(await readFile('assets/catalog/rules.json', 'utf8'));
const manifest = JSON.parse(await readFile('assets/manifest.json', 'utf8'));
const gdi = await nativeGdi(manifest);
if (process.argv.includes('--check')) {
  const catalog = JSON.parse(await readFile('assets/catalog/native-ui.json', 'utf8'));
  validateResources(catalog,manifest);
  if(catalog.sourceSha256!==rules.sourceSha256)throw new Error('Native UI and domain source identities differ');
  for(const kind of ['statShop','itemShop','equipmentWorkshop','career'])
    if(JSON.stringify(catalog[kind].gdiIndexMaps)!==JSON.stringify(indexMaps(gdi,kind)))throw new Error(`Native GDI index map differs: ${kind}`);
  console.log(`Verified native UI metadata and fixed GDI observations: ${catalog.sourceSha256}`);
  process.exit(0);
}
const bytes = await readFile('research/original/N2Game.exe');
const sourceSha256 = createHash('sha256').update(bytes).digest('hex');
if (sourceSha256 !== rules.sourceSha256) throw new Error('Reference executable differs from the recovered rules');
const pe = bytes.readUInt32LE(60), sections = bytes.readUInt16LE(pe + 6), base = bytes.readUInt32LE(pe + 52);
const table = pe + 24 + bytes.readUInt16LE(pe + 20);
function offset(va) {
  const rva = va - base;
  for (let index = 0; index < sections; index++) {
    const at = table + index * 40, start = bytes.readUInt32LE(at + 12);
    if (rva >= start && rva < start + Math.max(bytes.readUInt32LE(at + 8), bytes.readUInt32LE(at + 16))) return bytes.readUInt32LE(at + 20) + rva - start;
  }
  throw new Error(`Unmapped reference address: ${va.toString(16)}`);
}
const i32 = va => bytes.readInt32LE(offset(va));
const i16 = va => bytes.readInt16LE(offset(va));
function text(va) { const start = offset(va); return new TextDecoder('gbk').decode(bytes.subarray(start, bytes.indexOf(0, start))); }
const pointer = va => text(bytes.readUInt32LE(offset(va)));
const address = va => `0x${va.toString(16).padStart(8, '0')}`;
function resolve(raw) {
  raw = raw.replaceAll('\\', '/').toLowerCase();
  const resource = [raw, `install/${raw}`, `cd/data/${raw}`].find(value => manifest.resources[value]);
  if (!resource) throw new Error(`Original UI resource is missing: ${raw}`);
  return resource;
}
function media(raw, x = 0, y = 0, transparent = 1) {
  const resource = resolve(raw), sha = manifest.resources[resource].sha256;
  const meta = manifest.objects.find(object => object.sha256 === sha).media;
  // The four recovered panels currently use FLC only. Do not silently inherit
  // a generic FLI conversion without a native clock instruction observation.
  if (meta.kind === 'fli') throw new Error(`Native FLI timing needs instruction evidence: ${resource}`);
  return { resource, x, y, width: meta.width, height: meta.height, transparent,
    ...(meta.frames ? { frameCount: meta.frames, interval: meta.speedRaw } : {}) };
}
function row(va, transparent = 1) { return { ...media(text(va + 8), i32(va), i32(va + 4), transparent), source: address(va) }; }
function button(raw, x, y) { const result = media(raw, x, y); return { ...result, frameCount: 3, frameHeight: Math.trunc(result.height / 3) }; }
const common = {
  evidence: ['0x00459fc0', '0x0045a090', '0x0045b960'],
  yes: button('public/button/y01.bmp', 393, 423), no: button('public/button/n01.bmp', 458, 423),
  back: [media('public/button/exita.flc', 519, 272), media('public/button/exitb.flc', 519, 272)],
  text: { x: 128, y: 371, width: 400, height: 100, size: 16, weight: 700, color: [255, 255, 255, 255] },
  blink: { startMs: 2000, periodMs: 2100 },
};
const statShop = {
  evidence: ['0x0042c970', '0x0042b930', '0x0042bf70', '0x0042c230', '0x0042c270', '0x0042c620'],
  layers: [row(0x47cf30, null), row(0x47cf58)], blink: row(0x47cf80),
  slots: Array.from({ length: 8 }, (_, i) => ({ x: i32(0x47cfa8 + i * 40), y: i32(0x47cfac + i * 40), width: 51, height: 51, source: address(0x47cfa8 + i * 40) })),
  paletteSections: [{ resource: resolve('boss/bp201a.bmp'), first: 142, last: 161, source: '0x0042c9f2' }, { resource: resolve('q/aa01.bmp'), first: 226, last: 255, source: '0x0042ca0c' }, { resource: resolve('public/button/yes_a.flc'), first: 163, last: 200, source: '0x0042ca41' }],
  music: resolve(pointer(0x47d0ec)), buySound: resolve(pointer(0x47d0e8)),
  text: { ...common.text, bytes: 140 },
  descriptions: {
    young: Object.fromEntries(rules.statShops.young.flat().map((item, i) => [resolve(item.resource), pointer(0x47cd28 + i * 4)])),
    adult: Object.fromEntries(rules.statShops.adult.flat().map((item, i) => [resolve(item.resource), [pointer(0x47cd64 + i * 8), pointer(0x47cd68 + i * 8)]])),
  },
  messages: { greeting: pointer(0x47c200), insufficient: pointer(0x47c204), completed: pointer(0x47c208), select: text(0x47edd0), confirm: text(0x47eddc), separator: text(0x47edcc), price: text(0x47edc0), currency: text(0x47edbc) },
};
const itemShop = {
  evidence: ['0x0043b540', '0x004399b0', '0x0043a4b0', '0x0043a4e0', '0x0043a530', '0x0043a7b0'],
  layers: [row(0x490590, null), row(0x4905b8)], blink: row(0x4905e0),
  tabs: Array.from({ length: 4 }, (_, i) => ({ ...button(pointer(0x490ff0 + i * 4), i16(0x491000 + i * 4), i16(0x491002 + i * 4)), category: i })),
  slots: Array.from({ length: 3 }, (_, i) => ({ x: i16(0x490608 + i * 4), y: i16(0x49060a + i * 4), width: 177, height: 194, source: address(0x490608 + i * 4) })),
  equipmentAnimations: Array.from({ length: 18 }, (_, i) => media(text(0x48e3f0 + i * 70))),
  paletteSections: [{ resource: resolve('boss/bs201a.bmp'), first: 142, last: 161 }, { resource: resolve('public/button/yes_a.flc'), first: 163, last: 200, source: '0x0043b87a' }],
  music: resolve(pointer(0x491014)), confirmText: text(0x493c2c), retrieveText: text(0x493c44), purchasedText: pointer(0x490260), greeting: pointer(0x49025c),
};
const equipmentWorkshop = {
  evidence: ['0x0043d130', '0x0043cb10', '0x0043cf70', '0x0043d020'],
  layers: [row(0x493dc0, null), row(0x493de8)], blink: row(0x493e10), researchBackground: row(0x493e38, null),
  modes: [button(pointer(0x493db8), 250, 100), button(pointer(0x493dbc), 350, 100)],
  slots: Array.from({ length: 8 }, (_, i) => ({ x: i32(0x493d20 + i * 8), y: i32(0x493d24 + i * 8), width: 51, height: 51, source: address(0x493d20 + i * 8) })),
  animations: Array.from({ length: 18 }, (_, i) => media(text(0x493e98 + i * 70), 357, 51, 'firstPixel')),
  research: media(pointer(0x494388), 357, 51, 'firstPixel'), resultAnimation: media('pm0p/pm0p06.flc', 357, 51, 'firstPixel'),
  music: resolve(pointer(0x493e60)),
  paletteSections: [{ resource: resolve('boss/br201a.bmp'), first: 212, last: 231, source: '0x0043d293' }, { resource: resolve('public/button/yes_a.flc'), first: 163, last: 200, source: '0x0043d2c6' }],
  messages: { greeting: pointer(0x493c80), purchase: pointer(0x493c74), purchased: pointer(0x493c78), upgraded: pointer(0x493db4), maximumLevel: pointer(0x493cd0), insufficientMoney: text(0x4954c4), capacityReached: text(0x4954d8), research: pointer(0x493dac), confirmResearch: pointer(0x493db0), cancelled: pointer(0x493c7c), researchItem: pointer(0x493e64), researchPrice: pointer(0x493e68), itemSeparator: text(0x49549c) },
};
const career = {
  evidence: ['0x00419590', '0x0042e400', '0x0042d4b0', '0x0042e2d0'],
  background: media(pointer(0x47f2e8), 0, 0, null), defaultDetail: media(pointer(0x47f2ec), 300, 103),
  intro: media(pointer(0x47f2f0), 249, 175, 'firstPixel'), marker: media(pointer(0x47f294)),
  exit: common.back.map(value => ({ ...value, x: 545, y: 384 })),
  transitionIn: media(pointer(0x47f2fc), 250, 50, 'firstPixel'), transitionOut: media(pointer(0x47f300), 0, 200, 'firstPixel'),
  rows: Array.from({ length: 10 }, (_, i) => ({ x: i32(0x47f298 + i * 8), y: i32(0x47f29c + i * 8), width: 80, height: 26, source: address(0x47f298 + i * 8) })),
  entries: Array.from({ length: 24 }, (_, i) => ({ id: i, label: media(pointer(0x47eea8 + i * 4)), detail: media(pointer(0x47ef08 + i * 4), 300, 103), description: pointer(0x47f230 + i * 4), actors: [0, 1].map(sex => media(pointer(0x47ef68 + (sex * 24 + i) * 4), 300, 90, 'firstPixel')) })),
  scroll: { x: 150, y: 76, parts: [0x47ff48, 0x47ff34, 0x47ff20, 0x47ff0c, 0x47fefc, 0x47feec].map(va => media(text(va))) },
  music: resolve(pointer(0x47f308)), selectedSound: resolve(pointer(0x47f304)),
  messages: { greeting: pointer(0x47f028), confirm: pointer(0x47f030), insufficient: pointer(0x47f034), selected: pointer(0x47f038), current: pointer(0x47f058), degree: pointer(0x47f054) },
};
const catalog = { formatVersion: 1, sourceSha256, common, statShop, itemShop, equipmentWorkshop, career };
for(const kind of ['statShop','itemShop','equipmentWorkshop','career'])catalog[kind].gdiIndexMaps=indexMaps(gdi,kind);
catalog.gdiObservation={fixture:'tests/fixtures/original-native-ui-gdi.json',method:gdi.method,platform:gdi.platform};
const output = `${JSON.stringify(catalog, null, 2)}\n`;
await writeFile('assets/catalog/native-ui.json', output);
console.log(`Verified native UI metadata: ${sourceSha256}`);
