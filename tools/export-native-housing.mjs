// Recover housing presentation metadata from the fixed, unmodified executable.
// Runtime builds consume the checked-in JSON and never require the original.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const bytes = await readFile('research/original/N2Game.exe');
const manifest = JSON.parse(await readFile('assets/manifest.json', 'utf8'));
const sourceSha256 = createHash('sha256').update(bytes).digest('hex');
if (sourceSha256 !== 'e725c9c7dd9de766cd1bf66dba18260858e447980c65efbd292a9cd3687ef4c0') throw new Error('Wrong original executable');
const pe = bytes.readUInt32LE(60), sections = bytes.readUInt16LE(pe + 6), base = bytes.readUInt32LE(pe + 52);
const table = pe + 24 + bytes.readUInt16LE(pe + 20);
function offset(va) {
  const rva = va - base;
  for (let i = 0; i < sections; i++) {
    const at = table + i * 40, start = bytes.readUInt32LE(at + 12);
    if (rva >= start && rva < start + Math.max(bytes.readUInt32LE(at + 8), bytes.readUInt32LE(at + 16))) return bytes.readUInt32LE(at + 20) + rva - start;
  }
  throw new Error(`Unmapped original address ${va.toString(16)}`);
}
const i32 = va => bytes.readInt32LE(offset(va));
const i16 = va => bytes.readInt16LE(offset(va));
const address = va => `0x${va.toString(16).padStart(8, '0')}`;
function text(va) { const at = offset(va); return new TextDecoder('gbk').decode(bytes.subarray(at, bytes.indexOf(0, at))); }
const pointer = va => text(bytes.readUInt32LE(offset(va)));
function instruction(va, hex) {
  if (bytes.subarray(offset(va), offset(va) + hex.length / 2).toString('hex') !== hex) throw new Error(`Housing instruction changed at ${address(va)}`);
}
// Ghidra omitted the third argument of both palette methods. Verify all pushes
// from x86 rather than inferring end indices from its incomplete signatures.
instruction(0x42a64e, '68a1000000688e00000068d8ae4700');
instruction(0x42a6ac, '68c800000068a200000052');
instruction(0x42a6d7, '688d0000006a0250');
instruction(0x42a725, '688d0000006a0251');
function resolve(raw) {
  raw = raw.replaceAll('\\', '/').toLowerCase();
  const found = [raw, `install/${raw}`, `cd/data/${raw}`, `cd/${raw}`].find(value => manifest.resources[value]);
  if (!found) throw new Error(`Missing original housing artwork ${raw}`);
  return found;
}
function media(raw, x = 0, y = 0, transparent = 1) {
  const resource = resolve(raw), hash = manifest.resources[resource].sha256;
  const obj = manifest.objects.find(value => value.sha256 === hash), meta = obj.media;
  const result = { resource, x, y, width: meta.width, height: meta.height, transparent };
  if (meta.frames) {
    if (meta.kind === 'fli') throw new Error(`Unverified FLI timing ${resource}`);
    result.frameCount = meta.frames; result.interval = meta.speedRaw;
  }
  return result;
}
const row = (va, transparent = 1) => ({ ...media(text(va + 8), i32(va), i32(va + 4), transparent), source: address(va) });
function strip(raw, x, y) { const value = media(raw, x, y); return { ...value, frameCount: 3, frameHeight: Math.trunc(value.height / 3) }; }
const data = {
  formatVersion: 1, sourceSha256,
  evidence: ['0x004296b0', '0x0042a0e0', '0x0042a550', '0x0042a120', '0x0042b2e0'],
  layers: [row(0x47aea8, null), row(0x47aed0)], blink: row(0x47aef8),
  music: resolve(pointer(0x47b3ac)), buySound: resolve(pointer(0x47b3a4)), houseSound: resolve(pointer(0x47b3a8)),
  paletteSections: [{ resource: resolve('boss/be201a.bmp'), first: 142, last: 161, source: '0x0042a550' }, { resource: resolve('public/button/yes_a.flc'), first: 162, last: 200, source: '0x0042a6b7' }],
  previewPalette: { first: 2, last: 141, purchaseCall: '0x0042a6df', investmentCall: '0x0042a733', untouchedBackgroundRange: [201, 255] },
  houses: Array.from({ length: 4 }, (_, id) => ({ id, ...media(text(0x47af20 + id * 60), i32(0x47af54 + id * 60), i32(0x47af58 + id * 60)), source: address(0x47af20 + id * 60), description: pointer(0x47add0 + id * 4) })),
  investments: Array.from({ length: 12 }, (_, id) => ({ id, ...media(text(0x47b010 + id * 70), 357, 51, 'firstPixel'), source: address(0x47b010 + id * 70) })),
  previous: strip(text(0x47c134), 364, 278), next: strip(text(0x47c124), 430, 278),
  yes: strip('public/button/y01.bmp', 393, 423), no: strip('public/button/n01.bmp', 458, 423),
  invest: [media(text(0x47c114), 393, 423), media(text(0x47c100), 393, 423)],
  income: [media(text(0x47c0ec), 458, 423), media(text(0x47c0d8), 458, 423)],
  back: [media('public/button/exita.flc', 519, 272), media('public/button/exitb.flc', 519, 272)],
  text: { x: 128, y: 371, width: 400, height: 100, size: 16, weight: 700, lineHeight: 16, color: [255,255,255,255] },
  clocks: { introMs: 20000, insufficientMs: 10000, blinkStartMs: 2000, blinkPeriodMs: 2100 },
  messages: { greeting: pointer(0x47adc8), select: pointer(0x47adcc), insufficient: pointer(0x47ade0), completed: pointer(0x47adec), investOnly: text(0x47c1a4), investAndIncome: text(0x47c1cc), incomeOnly: text(0x47c1b8), specialGreeting: pointer(0x49b344) },
  stageDescriptions: Array.from({ length: 13 }, (_, i) => pointer(0x47ae70 + i * 4)),
  hud: { money: { x: 77, y: 46 }, intelligence: { x: 77, y: 17 }, morality: { x: 229, y: 17 }, charm: { x: 379, y: 17 }, stamina: { x: 530, y: 17 } },
  gains: { evidence: '0x0042b2e0', effectY: 160, digitY: 230, digitSpacing: 30, stepX: 6, stepY: 16, digits: Array.from({ length: 10 }, (_, i) => media(pointer(0x47cf08 + i * 4))), order: [
    { attribute: 'charm', x: 356, y: 17, effect: media(pointer(0x47ced8), 356, 160) },
    { attribute: 'morality', x: 206, y: 17, effect: media(pointer(0x47cee0), 206, 160) },
    { attribute: 'intelligence', x: 54, y: 17, effect: media(pointer(0x47cee4), 54, 160) },
    { attribute: 'stamina', x: 507, y: 17, effect: media(pointer(0x47cedc), 507, 160) },
  ] },
  originalDifferences: ['The Japanese house description says 60000, while the executable price table charges 40000. Preserve both.', 'Business investment amounts and income are provided by the core; prose promises are not rewritten to match those tables.'],
};
const serialized = `${JSON.stringify(data, null, 2)}\n`;
const output = 'assets/catalog/native-housing.json';
if (process.argv.includes('--check')) {
  if (await readFile(output, 'utf8') !== serialized) throw new Error('Housing presentation metadata differs from original');
} else await writeFile(output, serialized);
console.log(`Verified original housing UI: ${data.houses.length} houses, ${data.investments.length} investments, ${sourceSha256}`);
