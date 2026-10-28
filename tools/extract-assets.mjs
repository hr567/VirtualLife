#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  extractInstallShieldFile, inspectMedia, normalizeLegacyPath, parseCountedList,
  parseInstallShieldCab, parseMap, parseN1Program, parseWordRecords, parseFortuneData, parseHighTestData,
} from './legacy-formats.mjs';

const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const discOption = args.indexOf('--disc');
const disc = path.resolve(discOption < 0 ? 'E:/' : args[discOption + 1]);
const onlyExe = args.includes('--only-exe');
const sha256 = (buffer) => createHash('sha256').update(buffer).digest('hex');
const writeJson = (relative, value) => {
  const destination = path.join(repository, relative);
  mkdirSync(path.dirname(destination), { recursive: true });
  writeFileSync(destination, `${JSON.stringify(value, null, 2)}\n`);
};
function writeIdenticalOrNew(relative, bytes) {
  const destination = path.join(repository, relative);
  mkdirSync(path.dirname(destination), { recursive: true });
  if (existsSync(destination)) {
    const existing = readFileSync(destination);
    if (existing.length !== bytes.length || sha256(existing) !== sha256(bytes)) throw new Error(`Refusing to overwrite different content at ${relative}`);
  } else writeFileSync(destination, bytes);
}

console.log('Reading and validating InstallShield cabinet...');
const cabinetPath = path.join(disc, 'DATA1.CAB');
const cabinet = readFileSync(cabinetPath);
const archive = parseInstallShieldCab(cabinet);
const executableEntry = archive.files.find((entry) => !entry.invalid && entry.normalizedPath === 'n2game.exe');
if (!executableEntry) throw new Error('The expected N2Game.exe is absent');
const executable = extractInstallShieldFile(cabinet, executableEntry);
writeIdenticalOrNew('research/original/N2Game.exe', executable);
writeJson('research/original/provenance.json', {
  formatVersion: 1,
  cabinet: { relativePath: 'DATA1.CAB', sha256: sha256(cabinet), bytes: cabinet.length, formatVersionRaw: archive.version },
  executable: { relativePath: 'N2Game.exe', sha256: sha256(executable), bytes: executable.length, cabinetEntry: executableEntry.index },
  invalidCabinetEntryIndices: archive.files.filter((entry) => entry.invalid).map((entry) => entry.index),
});
console.log(`Original executable ready: ${path.join(repository, 'research/original/N2Game.exe')}`);
if (onlyExe) process.exit(0);

const objects = new Map();
const resources = new Map();
const originalData = new Map();
let totalSourceBytes = 0;

function addResource(id, bytes, provenance) {
  const hash = sha256(bytes);
  const media = inspectMedia(bytes, id);
  const existing = resources.get(id);
  if (existing && existing.sha256 !== hash) throw new Error(`Conflicting legacy resource path: ${id}`);
  const relativePath = `assets/game/objects/${hash.slice(0, 2)}/${hash}.${media.extension}`;
  if (!objects.has(hash)) {
    writeIdenticalOrNew(relativePath, bytes);
    const { extension, ...publicMedia } = media;
    objects.set(hash, { sha256: hash, path: relativePath, bytes: bytes.length, media: publicMedia });
  }
  resources.set(id, { sha256: hash, source: provenance });
  totalSourceBytes += bytes.length;
  if (/\/(event|data1)\//.test(id) || /\/map\/[^/]+\/[^/]+\.(mmi|map|mct)$/.test(id)) originalData.set(id, bytes);
}

let count = 0;
for (const entry of archive.files) {
  if (entry.invalid || entry.normalizedPath === 'n2game.exe') continue;
  const bytes = extractInstallShieldFile(cabinet, entry);
  addResource(`install/${entry.normalizedPath}`, bytes, {
    kind: 'cabinet', container: 'DATA1.CAB', relativePath: entry.sourcePath, cabinetEntry: entry.index,
    compressedBytes: entry.compressedBytes, expandedBytes: entry.expandedBytes,
  });
  if (++count % 500 === 0) console.log(`Extracted ${count} installation resources...`);
}

function collectFiles(directory) {
  const result = [];
  for (const entry of readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name, 'en'))) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) result.push(...collectFiles(absolute));
    else if (entry.isFile()) result.push(absolute);
  }
  return result;
}
const dataFiles = collectFiles(path.join(disc, 'Data'));
count = 0;
for (const absolute of dataFiles) {
  const relative = path.relative(disc, absolute).replaceAll('\\', '/');
  addResource(`cd/${normalizeLegacyPath(relative)}`, readFileSync(absolute), { kind: 'disc-file', relativePath: relative });
  if (++count % 500 === 0) console.log(`Read ${count} optical-disc resources...`);
}

const readResource = (id, optional = false) => {
  if (originalData.has(id)) return originalData.get(id);
  if (optional && !resources.has(id)) return null;
  const resource = resources.get(id);
  if (!resource) throw new Error(`Required original resource is missing: ${id}`);
  return readFileSync(path.join(repository, objects.get(resource.sha256).path));
};

const mapIds = [...resources.keys()].filter((id) => /^cd\/data\/map\/[^/]+\/[^/]+\.mmi$/.test(id)).map((id) => id.split('/')[3]).sort();
const maps = [];
for (const id of mapIds) {
  const prefix = `cd/data/map/${id}/${id}`;
  const tiles = [];
  for (const [resourceId, resource] of resources) {
    if (!resourceId.startsWith(`${prefix}.m`)) continue;
    const match = /\.m(\d)(\d)$/.exec(resourceId);
    if (!match) continue;
    const media = objects.get(resource.sha256).media;
    if (media.kind !== 'bmp' || media.width !== 640 || media.height !== 480) throw new Error(`Unexpected map tile dimensions: ${resourceId}`);
    const row = Number(match[1]), column = Number(match[2]);
    tiles.push({ row, column, x: column * 640, y: row * 480, width: 640, height: 480, resourceId });
  }
  tiles.sort((a, b) => a.row - b.row || a.column - b.column);
  const map = parseMap({ id, mmi: readResource(`${prefix}.mmi`), mct: readResource(`${prefix}.mct`), legacyMap: readResource(`${prefix}.map`, true), resourceId: `${prefix}.mmi`, tiles });
  if (tiles.length * 640 * 480 !== map.dimensions.width * map.dimensions.height || tiles.some((tile) => tile.x + tile.width > map.dimensions.width || tile.y + tile.height > map.dimensions.height)) {
    throw new Error(`Map tile grid does not cover ${id}`);
  }
  maps.push(map);
}
writeJson('assets/catalog/maps.json', { formatVersion: 1, logicalViewport: { width: 640, height: 480 }, maps });

const counted = (name, recordBytes) => ({ sourceResource: `install/event/${name}`, ...parseCountedList(readResource(`install/event/${name}`), recordBytes) });
const wordTable = (name, recordBytes) => {
  const records = parseWordRecords(readResource(`install/event/${name}`), recordBytes);
  return { sourceResource: `install/event/${name}`, recordBytes, count: records.length, semanticsStatus: 'requires-disassembly', records };
};
const abc = readResource('install/event/abc.lst');
const abcCount = abc.readUInt16LE(0);
if (abc.length !== (abcCount + 1) * 10) throw new Error('ABC list size mismatch');
const events = {
  formatVersion: 1,
  status: 'original-data-decoded-native-vm-parity-tested',
  dialogues: counted('dialogue.lst', 256), portraits: counted('portrait.lst', 24), backgrounds: counted('backgrnd.lst', 24), waves: counted('wave.lst', 24),
  additionalLists: {
    backmode: counted('backmode.lst', 256), diwen: counted('diwen.lst', 24), diwenmod: counted('diwenmod.lst', 256), midi: counted('midi.lst', 24), startmod: counted('startmod.lst', 256),
  },
  abc: { sourceResource: 'install/event/abc.lst', count: abcCount, recordBytes: 10, headerWords: parseWordRecords(abc.subarray(0, 10), 10)[0].words, semanticsStatus: 'requires-disassembly', records: parseWordRecords(abc.subarray(10), 10) },
  conditions: wordTable('condition.lst', 58), results: wordTable('result.lst', 118), switches: wordTable('switch.lst', 28),
  program: parseN1Program(readResource('install/event/n1_whole.n1e')),
};
writeJson('assets/catalog/events.json', events);

function indexedData(indexName, dataName, offsetBytes) {
  const indexId = `install/data1/${indexName}`;
  const dataId = `install/data1/${dataName}`;
  const index = readResource(indexId), data = readResource(dataId);
  if (index.length % offsetBytes) throw new Error(`Misaligned index ${indexId}`);
  const offsets = Array.from({ length: index.length / offsetBytes }, (_, item) => offsetBytes === 4 ? index.readUInt32LE(item * 4) : index.readUInt16LE(item * 2));
  if (offsets.some((value, item) => value > data.length || (item > 0 && value < offsets[item - 1]))) throw new Error(`Invalid index offsets ${indexId}`);
  return { indexResource: indexId, dataResource: dataId, offsetBytes, offsets, dataBytes: data.length, semanticsStatus: 'requires-disassembly' };
}
writeJson('assets/catalog/questionnaires.json', {
  formatVersion: 1,
  fortune: { ...indexedData('fortune.idx', 'fortune.dat', 4), ...parseFortuneData(readResource('install/data1/fortune.idx'), readResource('install/data1/fortune.dat')) },
  highTest: { ...indexedData('hightest.idx', 'hightest.dat', 2), ...parseHighTestData(readResource('install/data1/hightest.idx'), readResource('install/data1/hightest.dat')) },
});

const objectList = [...objects.values()].sort((a, b) => a.sha256.localeCompare(b.sha256));
const storedBytes = objectList.reduce((sum, object) => sum + object.bytes, 0);
const manifest = {
  formatVersion: 1,
  title: 'Virtual Life / 虚拟人生',
  originalDisplayMode: { width: 640, height: 480, bitsPerPixel: 16, evidence: 'N2Game.exe SetDisplayMode call at VA 0x4234DB' },
  provenance: {
    cabinet: { relativePath: 'DATA1.CAB', bytes: cabinet.length, sha256: sha256(cabinet), descriptorCount: archive.fileCount, invalidDescriptorCount: archive.files.filter((entry) => entry.invalid).length },
    originalExecutable: { sha256: sha256(executable), bytes: executable.length, includedInPublicAssets: false },
  },
  statistics: { resourceCount: resources.size, uniqueObjectCount: objectList.length, sourceBytes: totalSourceBytes, storedBytes, deduplicatedBytes: totalSourceBytes - storedBytes },
  catalogs: { maps: 'assets/catalog/maps.json', events: 'assets/catalog/events.json', questionnaires: 'assets/catalog/questionnaires.json' },
  objects: objectList,
  resources: Object.fromEntries([...resources.entries()].sort(([a], [b]) => a.localeCompare(b))),
};
writeJson('assets/manifest.json', manifest);
console.log(JSON.stringify({ ...manifest.statistics, maps: maps.length, chips: maps.reduce((sum, map) => sum + map.chips.length, 0), dialogues: events.dialogues.count, eventNodes: events.program.nodeCount }, null, 2));
console.log('Asset extraction and structural validation completed. No installer or original executable was run.');
