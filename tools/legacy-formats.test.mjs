import assert from 'node:assert/strict';
import { deflateRawSync } from 'node:zlib';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  decodeProfileStruct, extractInstallShieldFile, normalizeLegacyPath,
  parseCountedList, parseInstallShieldCab, parseN1Program, parseHighTestData, parseFortuneData, inspectMedia,
} from './legacy-formats.mjs';

test('profile structs preserve little-endian fields and reject corrupt checksums', () => {
  assert.deepEqual(decodeProfileStruct('80070000A00500002C').signed, [1920, 1440]);
  assert.deepEqual(decodeProfileStruct('FFFFFFFFFC').signed, [-1]);
  assert.deepEqual(decodeProfileStruct('FFFFFFFFFC').unsigned, [0xffffffff]);
  assert.throws(() => decodeProfileStruct('80070000A00500002D'), /checksum/);
  assert.throws(() => decodeProfileStruct('800x'), /Invalid/);
});

function sampleCabinet() {
  const expected = Buffer.from('original resource bytes');
  const compressed = deflateRawSync(expected);
  const cab = Buffer.alloc(162 + compressed.length);
  cab.write('ISc(');
  cab.writeUInt32LE(0x01005100, 4);
  cab.writeUInt32LE(32, 12);
  cab.writeUInt32LE(48, 44); // descriptor-relative file table
  cab.writeUInt32LE(1, 60); // directory count
  cab.writeUInt32LE(1, 72); // file count
  cab.writeUInt32LE(8, 80); // empty directory string at 88
  cab.writeUInt32LE(16, 84); // file descriptor at 96
  cab.writeUInt32LE(64, 96); // name at 144
  cab.writeUInt16LE(4, 104);
  cab.writeUInt32LE(expected.length, 106);
  cab.writeUInt32LE(compressed.length + 2, 110);
  cab.writeUInt32LE(160, 134);
  cab.write('asset.bin', 144);
  cab.writeUInt16LE(compressed.length, 160);
  compressed.copy(cab, 162);
  return { cab, expected };
}

test('InstallShield extraction checks table pointers, blocks, and expanded lengths', () => {
  const { cab, expected } = sampleCabinet();
  const archive = parseInstallShieldCab(cab);
  assert.equal(archive.files[0].sourcePath, 'asset.bin');
  assert.deepEqual(extractInstallShieldFile(cab, archive.files[0]), expected);
  const wrongLength = { ...archive.files[0], expandedBytes: expected.length + 1 };
  assert.throws(() => extractInstallShieldFile(cab, wrongLength), /length mismatch/);
  const broken = Buffer.from(cab);
  broken.writeUInt32LE(0xffffffff, 96);
  assert.throws(() => parseInstallShieldCab(broken), /range/);
  cab.writeUInt16LE(0, 160);
  assert.throws(() => extractInstallShieldFile(cab, archive.files[0]), /Empty compressed/);
});

test('deleted InstallShield descriptors never dereference invalid filename pointers', () => {
  const { cab } = sampleCabinet();
  cab.writeUInt32LE(0xfcf59ec8, 96);
  cab.writeUInt16LE(8, 104);
  cab.writeUInt32LE(0, 106);
  cab.writeUInt32LE(0, 110);
  cab.writeUInt32LE(0, 134);
  assert.equal(parseInstallShieldCab(cab).files[0].invalid, true);
});

test('resource paths normalize Windows spelling without allowing traversal', () => {
  assert.equal(normalizeLegacyPath('Map\\Park\\PARK.M00'), 'map/park/park.m00');
  for (const value of ['../escape', 'Map/../escape', 'C:\\game', '/root/file']) assert.throws(() => normalizeLegacyPath(value), /Unsafe/);
});

test('FLIC size bytes with high bits set cannot alias an ASCII bitmap signature', () => {
  const bytes = Buffer.alloc(128);
  bytes.writeUInt32LE(0x14dc2, 0);
  bytes.writeUInt16LE(0xaf12, 4);
  bytes.writeUInt16LE(24, 6);
  bytes.writeUInt16LE(200, 8);
  bytes.writeUInt16LE(200, 10);
  bytes.writeUInt16LE(8, 12);
  assert.equal(inspectMedia(bytes, 'charani/m24/m24w1c.flc').kind, 'flc');
});

test('counted strings preserve explicit byte lengths and reject truncated tables', () => {
  const data = Buffer.alloc(48, 0x20);
  data.writeUInt16LE(1, 0);
  data.writeUInt16LE(0x400, 2);
  data.writeUInt16LE(7, 24);
  data.writeUInt16LE(9, 26);
  data.write('bg201.bmp', 28);
  assert.deepEqual(parseCountedList(data, 24).records[0], { id: 7, byteLength: 9, text: 'bg201.bmp' });
  assert.throws(() => parseCountedList(data.subarray(0, 47), 24), /length mismatch/);
});

test('N1 program retains unknown words and verifies declared instruction count', () => {
  const data = Buffer.alloc(42);
  data.write('N1 Group            ');
  data.writeUInt16LE(0x400, 20);
  data.writeUInt16LE(1, 22);
  data.writeUInt16LE(12, 24);
  [31, 11, 20861, 2, 0, 0].forEach((word, index) => data.writeUInt16LE(word, 30 + index * 2));
  assert.deepEqual(parseN1Program(data).nodes[0].words, [31, 11, 20861, 2, 0, 0]);
  data.writeUInt16LE(2, 22);
  assert.throws(() => parseN1Program(data), /dimensions/);
});

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const manifestPath = path.join(root, 'assets/manifest.json');

test('malformed animation exceptions remain exact original bytes and unreferenced by maps', () => {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const maps = JSON.parse(readFileSync(path.join(root, 'assets/catalog/maps.json'), 'utf8')).maps;
  const exceptions = JSON.parse(readFileSync(path.join(root, 'tests/fixtures/unused-malformed-media.json'), 'utf8')).cases;
  for (const exception of exceptions) {
    const object = manifest.objects.find(item => item.sha256 === exception.sha256);
    assert.equal(manifest.resources[exception.resource].sha256, exception.sha256);
    assert.equal(createHash('sha256').update(readFileSync(path.join(root, object.path))).digest('hex'), exception.sha256);
    const map = maps.find(map => map.id === exception.resource.split('/')[3]);
    assert.ok(map);
    const sourceName = exception.resource.split('/').at(-1);
    const fileNames = Object.values(map.legacySections).flatMap(section => Object.entries(section)
      .filter(([key, value]) => key.toLowerCase().includes('file') && typeof value === 'string').map(([,value]) => value.toLowerCase()));
    assert.ok(!fileNames.includes(sourceName), `${exception.resource} became a runtime map dependency`);
  }
});

test('HighTest preserves original answer order and rejects malformed fgets records', () => {
  const data = Buffer.from('Question?\r\nRight\r\nWrong1\r\nWrong2\r\nWrong3\r\n');
  const index = Buffer.alloc(4); index.writeUInt16LE(data.length, 2);
  const result = parseHighTestData(index, data);
  assert.deepEqual(result.records, [{ id: 0, question: 'Question?', options: ['Right', 'Wrong1', 'Wrong2', 'Wrong3'] }]);
  assert.equal(result.originalCorrectOption, 0);
  const malformed = Buffer.from(data); malformed[malformed.length - 2] = 32;
  assert.throws(() => parseHighTestData(index, malformed), /missing CRLF/);
});

test('questionnaire catalog parses every original record without repairing source data', { skip: !existsSync(manifestPath) }, () => {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const objects = new Map(manifest.objects.map((object) => [object.sha256, object]));
  const resource = (id) => readFileSync(path.join(root, objects.get(manifest.resources[id].sha256).path));
  const tests = parseHighTestData(resource('install/data1/hightest.idx'), resource('install/data1/hightest.dat'));
  assert.equal(tests.records.length, 247);
  assert.deepEqual(tests.records[1], { id: 1, question: '电话是谁发明的', options: ['贝尔', '瓦特', '牛顿', '莱特'] });
  const fortunes = parseFortuneData(resource('install/data1/fortune.idx'), resource('install/data1/fortune.dat'));
  assert.equal(fortunes.records.length, 50);
  assert.equal(fortunes.records[0].title, '你了解你的内在性格吗？');
  assert.equal(fortunes.records[33].scoreStride, 23);
  assert.equal(fortunes.records[33].runtimeCompatible, false);
  assert.equal(fortunes.records[37].scoreStride, 11);
  assert.equal(fortunes.records[37].runtimeCompatible, false);
});

test('extracted corpus has complete map topology, catalog references, and matching sampled hashes', { skip: !existsSync(manifestPath) }, () => {
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const maps = JSON.parse(readFileSync(path.join(root, manifest.catalogs.maps), 'utf8')).maps;
  const events = JSON.parse(readFileSync(path.join(root, manifest.catalogs.events), 'utf8'));
  assert.equal(maps.length, 8);
  assert.equal(maps.reduce((sum, map) => sum + map.chips.length, 0), 683);
  for (const map of maps) {
    assert.equal(map.validation.startExists, true, map.id);
    assert.deepEqual(map.validation.unresolvedEdges, [], map.id);
    assert.deepEqual(new Set(map.coordinateIndex.map((chip) => `${chip.row},${chip.column}`)), new Set(map.chips.map((chip) => `${chip.row},${chip.column}`)), map.id);
    for (const tile of map.backgroundTiles) assert.ok(manifest.resources[tile.resourceId], tile.resourceId);
  }
  assert.equal(events.dialogues.count, 4549);
  assert.equal(events.dialogues.records[0].text, '春天到郊外尽情游玩');
  assert.equal(events.program.nodeCount, 13913);
  assert.equal(events.conditions.count, 79);
  assert.equal(events.results.count, 1002);
  const objects = new Map(manifest.objects.map((object) => [object.sha256, object]));
  for (const [id, resource] of Object.entries(manifest.resources)) assert.ok(objects.has(resource.sha256), id);
  for (const object of manifest.objects) assert.equal(statSync(path.join(root, object.path)).size, object.bytes, object.path);
  const sampleIds = ['install/bmp/pm0201.bmp', 'install/event/dialogue.lst', 'cd/data/map/park/park.m00', 'cd/data/video/ffopening.avi'];
  for (const id of sampleIds) {
    const object = objects.get(manifest.resources[id].sha256);
    assert.equal(createHash('sha256').update(readFileSync(path.join(root, object.path))).digest('hex'), object.sha256);
  }
  assert.ok(manifest.statistics.storedBytes < 1_000_000_000);
  assert.equal(manifest.resources['install/n2game.exe'], undefined);
});
