// Presentation metadata from the fixed executable and its native GDI glyph metrics.
import { readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const bytes = await readFile('research/original/N2Game.exe');
const rules = JSON.parse(await readFile('assets/catalog/rules.json', 'utf8'));
const manifest = JSON.parse(await readFile('assets/manifest.json', 'utf8'));
const questionnaires = JSON.parse(await readFile('assets/catalog/questionnaires.json', 'utf8'));
const glyphs = JSON.parse(await readFile('assets/catalog/glyphs.json', 'utf8'));
const sourceSha256 = createHash('sha256').update(bytes).digest('hex');
if (sourceSha256 !== rules.sourceSha256) throw new Error('Reference executable differs from recovered rules');
const pe = bytes.readUInt32LE(60), sections = bytes.readUInt16LE(pe + 6), base = bytes.readUInt32LE(pe + 52);
const table = pe + 24 + bytes.readUInt16LE(pe + 20);
function offset(va) {
  const rva = va - base;
  for (let index = 0; index < sections; index++) {
    const at = table + index * 40, start = bytes.readUInt32LE(at + 12);
    if (rva >= start && rva < start + Math.max(bytes.readUInt32LE(at + 8), bytes.readUInt32LE(at + 16))) return bytes.readUInt32LE(at + 20) + rva - start;
  }
  throw new Error(`Unmapped reference address ${va.toString(16)}`);
}
const i32 = va => bytes.readInt32LE(offset(va));
function text(va) { const at = offset(va); return new TextDecoder('gbk').decode(bytes.subarray(at, bytes.indexOf(0, at))); }
const pointer = va => text(bytes.readUInt32LE(offset(va)));
function resolve(raw) {
  raw = raw.replaceAll('\\', '/').toLowerCase();
  const id = [raw, `install/${raw}`, `cd/data/${raw}`, `cd/${raw}`].find(id => manifest.resources[id]);
  if (!id) throw new Error(`Missing original resource ${raw}`);
  return id;
}
function media(raw, x = 0, y = 0, transparent = 1) {
  const resource = resolve(raw), sha = manifest.resources[resource].sha256;
  const m = manifest.objects.find(object => object.sha256 === sha).media;
  if (m.kind === 'fli') throw new Error(`Fortune FLI clock needs instruction evidence: ${resource}`);
  return { resource, x, y, width: m.width, height: m.height, transparent, ...(m.frames ? { frameCount: m.frames, interval: m.speedRaw } : {}) };
}
const halfWidth = new Set(glyphs.halfWidthCharacters);
const narrow = new Set(glyphs.narrowCharacters);
const characters = new Set(glyphs.characters);
const style = { height: 16 };
if (glyphs.formatVersion !== 2) throw new Error('Unsupported bitmap font layout metadata');
const advance = ch => {
  if (!characters.has(ch)) throw new Error(`Missing font metric ${ch}`);
  return (halfWidth.has(ch) ? 8 : narrow.has(ch) ? 7 : 16) + 1;
};
// FUN_0045bae0 appends each string on a new line; ASCII words are measured as
// units, while Chinese glyphs and spaces are measured individually by GDI.
function appendLayout(input, target) {
  let x = 0;
  target.lines.push('');
  const tokens = input.match(/\n| |[\x21-\x7f]+|[^\x00-\x7f]/gu) ?? [];
  for (const token of tokens) {
    if (token === '\n') { target.lines.push(''); x = 0; continue; }
    const width = [...token].reduce((n, ch) => n + advance(ch), 0);
    if (x + width > 260) { target.lines.push(''); x = 0; }
    target.lines[target.lines.length - 1] += token;
    x += width;
  }
}
const separator = text(0x48bc0c);
const records = Object.fromEntries(questionnaires.fortune.records.map(record => [record.id, {
  questions: record.questions.map(question => {
    const layout = { lines: [] };
    appendLayout(record.title, layout); appendLayout(separator, layout); appendLayout(question.prompt, layout);
    const options = question.options.map((option, index) => {
      const y = layout.lines.length * style.height;
      appendLayout(option, layout);
      return { index, y, text: option };
    });
    return { text: layout.lines.join('\n'), height: layout.lines.length * style.height, options };
  }),
}]));
const catalog = {
  formatVersion: 1, sourceSha256,
  evidence: ['0x00436b80', '0x004358a0', '0x00435b10', '0x00435c00', '0x00436450', '0x00435b50', '0x0045bae0'],
  layers: [0x48b2f0, 0x48b318].map((va, i) => ({ ...media(text(va + 8), i32(va), i32(va + 4), i === 0 ? null : 1), source: `0x${va.toString(16)}` })),
  paletteSections: [{ resource: resolve('boss/bd201a.bmp'), first: 142, last: 161 }, { resource: resolve('public/button/yes_a.flc'), first: 163, last: 200 }],
  music: resolve(pointer(0x48b278)),
  selector: media(text(0x48bb88)), cursor: media(text(0x48bc38)),
  scroll: { x: 381, y: 110, step: 10, parts: [0x48bb9c, 0x48bbb0, 0x48bbc4, 0x48bbd8, 0x48bbec, 0x48bbfc].map(va => media(text(va))) },
  body: { x: 117, y: 71, width: 260, height: 238, size: 16, weight: 700, lineHeight: style.height, color: [255, 255, 255, 255] },
  bottom: { x: 128, y: 371, width: 400, height: 100, size: 16, weight: 700, lineHeight: style.height, color: [255, 255, 255, 255] },
  hud: { y: 17, entries: [['intelligence', 77], ['morality', 229], ['charm', 379], ['stamina', 530]].map(([field, x]) => ({ field, x })) },
  delays: { intro: 10000, explanation: 20000, result: 10000 },
  answerKeys: ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'Minus', 'Equal'],
  messages: { introductions: Array.from({ length: 12 }, (_, i) => pointer(0x48b27c + i * 4)), mission: pointer(0x49b348), result: pointer(0x48b2ac) },
  layoutProvenance: { font: 'Preserved SimSun cell advances', size: 16, weight: 700, metrics: 'assets/catalog/glyphs.json halfWidthCharacters/narrowCharacters', append: 'FUN_0045bae0', separator },
  records,
};
const output = `${JSON.stringify(catalog, null, 2)}\n`;
if (process.argv.includes('--check')) {
  if (await readFile('assets/catalog/native-fortune.json', 'utf8') !== output) throw new Error('Native fortune metadata differs from fixed reference');
} else await writeFile('assets/catalog/native-fortune.json', output);
console.log(`Verified native fortune metadata: ${Object.keys(records).length} questionnaires, ${sourceSha256}`);
