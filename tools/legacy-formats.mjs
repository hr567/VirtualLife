import { inflateRawSync, constants as zlibConstants } from 'node:zlib';

function requireBytes(buffer, offset, length, description) {
  if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 0 || offset + length > buffer.length) {
    throw new Error(`${description}: range ${offset}+${length} exceeds ${buffer.length} bytes`);
  }
}

function zeroString(buffer, offset) {
  requireBytes(buffer, offset, 1, 'CAB string');
  const end = buffer.indexOf(0, offset);
  if (end < 0) throw new Error(`Unterminated CAB string at ${offset}`);
  return buffer.toString('latin1', offset, end);
}

export function normalizeLegacyPath(value) {
  const normalized = value.replaceAll('\\', '/').replace(/^\.\//, '');
  if (/^[a-z]:|^\//i.test(normalized) || normalized.split('/').some((part) => part === '..' || part === '.')) {
    throw new Error(`Unsafe legacy path: ${value}`);
  }
  return normalized.toLowerCase();
}

/** FUN_00434ea0 reads two distinct fortune.dat questionnaire record layouts. */
export function parseFortuneData(index, data) {
  if (index.length % 4 || index.length < 8) throw new Error('Invalid fortune index');
  const offsets = Array.from({ length: index.length / 4 }, (_, i) => index.readUInt32LE(i * 4));
  if (offsets[0] !== 0 || offsets.at(-1) !== data.length || offsets.some((v, i) => i && v < offsets[i - 1])) throw new Error('Invalid fortune record boundaries');
  const decoder = new TextDecoder('gbk', { fatal: true });
  const split = (text) => text.startsWith('#') ? text.slice(1).split('#') : [text];
  const integers = (text) => text.split(',').filter((value) => value !== '').map((value) => {
    if (!/^[+-]?\d+$/.test(value)) throw new Error(`Invalid fortune integer ${value}`);
    const number = Number(value);
    if (number < -32768 || number > 32767) throw new Error('Fortune integer exceeds int16');
    return number;
  });
  const records = offsets.slice(0, -1).map((offset, id) => {
    const buffer = data.subarray(offset, offsets[id + 1]);
    let cursor = 0;
    const byte = () => { requireBytes(buffer, cursor, 1, `Fortune ${id} byte`); return buffer[cursor++]; };
    const word = () => { requireBytes(buffer, cursor, 2, `Fortune ${id} word`); const value = buffer.readInt16LE(cursor); cursor += 2; return value; };
    const text = (length) => { requireBytes(buffer, cursor, length, `Fortune ${id} text`); const value = decoder.decode(buffer.subarray(cursor, cursor + length)); cursor += length; return value; };
    const format = byte(), titleLength = word(), count = word();
    if (![1, 2].includes(format) || count < 1 || count > 20) throw new Error(`Invalid fortune ${id} header`);
    const lengths = Array.from({ length: count }, () => Array.from({ length: format === 1 ? 4 : 2 }, word));
    const tailLengths = format === 2 ? [word(), word(), word()] : [];
    const title = text(titleLength), issues = [];
    const questions = lengths.map((lengths, question) => {
      const fields = lengths.map(text);
      const result = { prompt: fields[0], options: split(fields[1]), rawOptions: fields[1] };
      if (result.options.length > (format === 1 ? 12 : 5)) issues.push(`Question ${question} exceeds original option storage`);
      if (format === 1) {
        result.percentages = split(fields[2]).map(integers);
        result.explanations = split(fields[3]);
        result.rawPercentages = fields[2];
        if (result.percentages.length < result.options.length || result.percentages.some((v) => v.length !== 4)) issues.push(`Question ${question} has incomplete original percentage data`);
        // FUN_00435b50 falls back to the last explanation if there are fewer
        // lines than choices. Extra percentage rows are never selected.
      }
      return result;
    });
    const result = { id, format, title, questions, offset, bytes: buffer.length };
    if (format === 2) {
      const [rawScores, rawPercentages, rawExplanations] = tailLengths.map(text);
      const stride = Math.floor(tailLengths[0] / count);
      result.rawScores = rawScores; result.scoreStride = stride;
      result.percentages = split(rawPercentages).map(integers);
      result.explanations = split(rawExplanations);
      result.rawPercentages = rawPercentages;
      result.thresholds = integers(text(byte()));
      if (![4, 6, 8, 10].includes(stride)) {
        issues.push(`Original loader leaves scores uninitialized for stride ${stride}; score table has ${tailLengths[0]} bytes for ${count} questions`);
      } else {
        for (let q = 0; q < count; q++) {
          const encoded = rawScores.slice(q * stride, (q + 1) * stride);
          const scores = integers(encoded.slice(1));
          questions[q].scores = scores;
          if (!encoded.startsWith('#') || scores.length !== stride / 2 || scores.length < questions[q].options.length) issues.push(`Question ${q} score slice does not match original sscanf format`);
        }
      }
      if (result.percentages.some((v) => v.length !== 4) || result.percentages.length < 2 || result.percentages.length > 4) issues.push('Unsupported percentage outcome table');
      if (questions.at(-1).options.length >= 3 && result.percentages.length < 3) issues.push('Original outcome lookup can read an uninitialized third percentage row');
      if (result.thresholds.length < 2) issues.push('Second original score threshold is not initialized');
    }
    if (cursor !== buffer.length) throw new Error(`Fortune ${id} has ${buffer.length - cursor} unconsumed bytes`);
    result.runtimeCompatible = issues.length === 0;
    result.issues = issues;
    return result;
  });
  return { semanticsStatus: 'recovered-from-0x00434ea0', percentageOrder: ['morality', 'intelligence', 'stamina', 'charm'], records };
}

/** FUN_0043eac0: five GBK CRLF lines; original option 0 is the answer. */
export function parseHighTestData(index, data) {
  if (index.length % 2 || index.length < 4) throw new Error('Invalid HighTest index');
  const offsets = Array.from({ length: index.length / 2 }, (_, i) => index.readUInt16LE(i * 2));
  if (offsets[0] !== 0 || offsets.at(-1) !== data.length || offsets.some((v, i) => i && v < offsets[i - 1])) throw new Error('Invalid HighTest record boundaries');
  const decoder = new TextDecoder('gbk', { fatal: true });
  const records = offsets.slice(0, -1).map((offset, id) => {
    const buffer = data.subarray(offset, offsets[id + 1]);
    const fields = [];
    let cursor = 0;
    for (let line = 0; line < 5; line++) {
      const end = buffer.indexOf(Buffer.from('\r\n'), cursor);
      if (end < 0) throw new Error(`HighTest ${id} is missing CRLF line ${line}`);
      if (end - cursor + 2 >= (line === 0 ? 800 : 200)) throw new Error(`HighTest ${id} line exceeds original fgets buffer`);
      fields.push(decoder.decode(buffer.subarray(cursor, end)));
      cursor = end + 2;
    }
    if (cursor !== buffer.length) throw new Error(`HighTest ${id} contains extra lines`);
    return { id, question: fields[0], options: fields.slice(1) };
  });
  return { semanticsStatus: 'recovered-from-0x0043eac0', originalCorrectOption: 0, records };
}

/** The optical disc uses the legacy InstallShield descriptor layout (42-byte records). */
export function parseInstallShieldCab(buffer) {
  requireBytes(buffer, 0, 20, 'CAB header');
  if (buffer.toString('ascii', 0, 4) !== 'ISc(') throw new Error('Not an InstallShield cabinet');
  const version = buffer.readUInt32LE(4);
  if (version !== 0x01005100) throw new Error(`Unsupported InstallShield version 0x${version.toString(16)}`);
  const descriptor = buffer.readUInt32LE(12);
  requireBytes(buffer, descriptor, 48, 'CAB descriptor');
  const table = descriptor + buffer.readUInt32LE(descriptor + 12);
  const directoryCount = buffer.readUInt32LE(descriptor + 28);
  const fileCount = buffer.readUInt32LE(descriptor + 40);
  requireBytes(buffer, table, (directoryCount + fileCount) * 4, 'CAB file table');
  const directories = Array.from({ length: directoryCount }, (_, index) =>
    zeroString(buffer, table + buffer.readUInt32LE(table + index * 4)));
  const files = Array.from({ length: fileCount }, (_, index) => {
    const entry = table + buffer.readUInt32LE(table + (directoryCount + index) * 4);
    requireBytes(buffer, entry, 42, 'CAB file descriptor');
    const directoryIndex = buffer.readUInt16LE(entry + 4);
    if (directoryIndex >= directories.length) throw new Error(`CAB directory ${directoryIndex} is out of range`);
    const directory = directories[directoryIndex];
    const flags = buffer.readUInt16LE(entry + 8);
    const expandedBytes = buffer.readUInt32LE(entry + 10);
    const compressedBytes = buffer.readUInt32LE(entry + 14);
    const dataOffset = buffer.readUInt32LE(entry + 38);
    // Deleted InstallShield entries have flag 8 and an intentionally invalid name pointer.
    if (flags === 8 && expandedBytes === 0 && compressedBytes === 0 && dataOffset === 0) {
      return { index, directory, flags, invalid: true, expandedBytes, compressedBytes, dataOffset };
    }
    const name = zeroString(buffer, table + buffer.readUInt32LE(entry));
    if (flags & ~4) throw new Error(`Unsupported CAB file flags 0x${flags.toString(16)} for ${name}`);
    requireBytes(buffer, dataOffset, compressedBytes, `CAB data ${name}`);
    const sourcePath = [directory, name].filter(Boolean).join('/').replaceAll('\\', '/');
    return { index, name, directory, sourcePath, normalizedPath: normalizeLegacyPath(sourcePath), flags, invalid: false, expandedBytes, compressedBytes, dataOffset };
  });
  return { version, fileCount, directoryCount, files };
}

export function extractInstallShieldFile(cabinet, entry) {
  const input = cabinet.subarray(entry.dataOffset, entry.dataOffset + entry.compressedBytes);
  if (!(entry.flags & 4)) {
    if (input.length !== entry.expandedBytes) throw new Error(`Uncompressed CAB length mismatch: ${entry.sourcePath}`);
    return input;
  }
  const chunks = [];
  let expanded = 0;
  for (let cursor = 0; cursor < input.length;) {
    requireBytes(input, cursor, 2, `Compressed chunk length in ${entry.sourcePath}`);
    const length = input.readUInt16LE(cursor);
    cursor += 2;
    if (!length) throw new Error(`Empty compressed CAB chunk: ${entry.sourcePath}`);
    requireBytes(input, cursor, length, `Compressed chunk in ${entry.sourcePath}`);
    const chunk = inflateRawSync(input.subarray(cursor, cursor + length), {
      finishFlush: zlibConstants.Z_SYNC_FLUSH,
      maxOutputLength: Math.max(1, entry.expandedBytes - expanded),
    });
    chunks.push(chunk);
    expanded += chunk.length;
    cursor += length;
  }
  if (expanded !== entry.expandedBytes) throw new Error(`CAB length mismatch: ${entry.sourcePath}: ${expanded} != ${entry.expandedBytes}`);
  return Buffer.concat(chunks, expanded);
}

export function inspectMedia(buffer, sourcePath) {
  const extension = sourcePath.split('.').pop().toLowerCase();
  // Buffer's ASCII decoder clears bit 7: an FLC size beginning C2 4D would
  // otherwise be mistaken for the BMP signature 42 4D.
  if (buffer.length >= 54 && buffer[0] === 0x42 && buffer[1] === 0x4d) {
    const dibBytes = buffer.readUInt32LE(14);
    if (dibBytes < 40) return { kind: 'bmp', extension: 'bmp', dibBytes };
    return {
      kind: 'bmp', extension: 'bmp', width: buffer.readInt32LE(18), height: Math.abs(buffer.readInt32LE(22)),
      topDown: buffer.readInt32LE(22) < 0, bitsPerPixel: buffer.readUInt16LE(28), compression: buffer.readUInt32LE(30),
      pixelOffset: buffer.readUInt32LE(10), paletteEntries: buffer.readUInt32LE(46) || (buffer.readUInt16LE(28) <= 8 ? 2 ** buffer.readUInt16LE(28) : 0),
      transparency: 'not-established',
    };
  }
  if (buffer.length >= 128 && [0xaf11, 0xaf12].includes(buffer.readUInt16LE(4))) {
    const magic = buffer.readUInt16LE(4);
    return {
      kind: magic === 0xaf11 ? 'fli' : 'flc', extension: 'flc', magic: magic.toString(16),
      declaredBytes: buffer.readUInt32LE(0), frames: buffer.readUInt16LE(6), width: buffer.readUInt16LE(8), height: buffer.readUInt16LE(10),
      bitsPerPixel: buffer.readUInt16LE(12), speedRaw: magic === 0xaf11 ? buffer.readUInt16LE(16) : buffer.readUInt32LE(16),
      transparency: 'not-established',
    };
  }
  if (buffer.length >= 12 && buffer.subarray(0, 4).equals(Buffer.from('RIFF'))) {
    const type = buffer.toString('ascii', 8, 12);
    if (type === 'WAVE') {
      const result = { kind: 'wav', extension: 'wav' };
      for (let offset = 12; offset + 8 <= buffer.length;) {
        const id = buffer.toString('ascii', offset, offset + 4);
        const length = buffer.readUInt32LE(offset + 4);
        if (offset + 8 + length > buffer.length) break;
        if (id === 'fmt ' && length >= 16) Object.assign(result, {
          formatTag: buffer.readUInt16LE(offset + 8), channels: buffer.readUInt16LE(offset + 10), sampleRate: buffer.readUInt32LE(offset + 12), bitsPerSample: buffer.readUInt16LE(offset + 22),
        });
        offset += 8 + length + (length % 2);
      }
      return result;
    }
    if (type === 'AVI ') {
      const stream = buffer.indexOf('vids');
      return { kind: 'avi', extension: 'avi', videoFourCC: stream >= 0 && stream + 8 <= buffer.length ? buffer.toString('ascii', stream + 4, stream + 8) : null };
    }
  }
  if (buffer.length >= 2 && buffer[0] === 0x4d && buffer[1] === 0x5a) return { kind: 'pe', extension: 'exe' };
  return { kind: ['mmi', 'map', 'txt'].includes(extension) ? 'legacy-text' : 'binary', extension: /^[a-z0-9]{1,5}$/.test(extension) ? extension : 'bin' };
}

/** GetPrivateProfileStruct encodes bytes as hex followed by a one-byte sum. */
export function decodeProfileStruct(value) {
  if (!/^(?:[0-9a-f]{2})+$/i.test(value) || value.length < 4) throw new Error(`Invalid profile struct ${value}`);
  const encoded = Buffer.from(value, 'hex');
  const payload = encoded.subarray(0, -1);
  const checksum = [...payload].reduce((sum, byte) => (sum + byte) & 0xff, 0);
  if (checksum !== encoded.at(-1)) throw new Error(`Profile checksum mismatch for ${value}`);
  if (payload.length % 4) return { rawHex: value, bytes: [...payload] };
  return {
    rawHex: value,
    signed: Array.from({ length: payload.length / 4 }, (_, index) => payload.readInt32LE(index * 4)),
    unsigned: Array.from({ length: payload.length / 4 }, (_, index) => payload.readUInt32LE(index * 4)),
  };
}

export function parseLegacyIni(buffer, encoding = 'big5') {
  const sections = {};
  let current;
  for (const raw of new TextDecoder(encoding, { fatal: true }).decode(buffer).split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith(';')) continue;
    if (line.startsWith('[') && line.endsWith(']')) {
      const name = line.slice(1, -1);
      if (Object.hasOwn(sections, name)) throw new Error(`Duplicate INI section ${name}`);
      current = sections[name] = {};
      continue;
    }
    const equals = line.indexOf('=');
    if (!current || equals < 0) throw new Error(`Unexpected legacy INI line: ${line}`);
    const key = line.slice(0, equals).trim();
    if (Object.hasOwn(current, key)) throw new Error(`Duplicate INI key ${key}`);
    current[key] = line.slice(equals + 1).trim();
  }
  return sections;
}

export function parseMap({ id, mmi, mct, legacyMap, resourceId, tiles }) {
  const sections = parseLegacyIni(mmi);
  const decoded = {};
  for (const [section, fields] of Object.entries(sections)) {
    decoded[section] = {};
    for (const [key, value] of Object.entries(fields)) {
      decoded[section][key] = /^(?:[a-f0-9]{2})+$/i.test(value) ? decodeProfileStruct(value) : value;
    }
  }
  const integers = (fields, name, count = 1) => {
    const numbers = fields?.[name]?.signed;
    if (!numbers || numbers.length !== count) throw new Error(`${id}: ${name} is not ${count} signed int32 values`);
    return numbers;
  };
  const general = decoded.General;
  const [width, height] = integers(general, 'MapSize', 2);
  const [originX, originY] = integers(general, 'ChipPos', 2);
  const [chipWidth, chipHeight] = integers(general, 'ChipSize', 2);
  const labels = legacyMap ? parseLegacyIni(legacyMap, 'big5') : {};
  const chips = [];
  for (const [name, fields] of Object.entries(decoded)) {
    const match = /^Chip_(\d+)_(\d+)$/.exec(name);
    if (!match) continue;
    const next = [];
    for (let index = 1; index <= 2; index++) {
      const [row] = integers(fields, `NextChip${index}Row`);
      const [column] = integers(fields, `NextChip${index}Column`);
      if (row === -1 && column === -1) continue;
      if (row < 0 || column < 0) throw new Error(`${id}: partially absent next chip in ${name}`);
      next.push({ row, column });
    }
    const backgroundCells = [];
    for (let index = 1; index <= 4; index++) {
      backgroundCells.push({ row: integers(fields, `Map${index}Row`)[0], column: integers(fields, `Map${index}Column`)[0] });
    }
    chips.push({ row: Number(match[1]), column: Number(match[2]), eventId: integers(fields, 'Event')[0], next, backgroundCells,
      ...(labels[name]?.Name ? { legacyEditorLabel: labels[name].Name } : {}) });
  }
  const keys = new Set(chips.map((chip) => `${chip.row},${chip.column}`));
  const unresolvedEdges = [];
  for (const chip of chips) for (const next of chip.next) {
    if (!keys.has(`${next.row},${next.column}`)) unresolvedEdges.push({ from: { row: chip.row, column: chip.column }, to: next });
  }
  if (mct.length % 8) throw new Error(`${id}: MCT is not an array of int32 coordinate pairs`);
  const coordinateIndex = Array.from({ length: mct.length / 8 }, (_, index) => ({ row: mct.readInt32LE(index * 8), column: mct.readInt32LE(index * 8 + 4) }));
  const start = { row: integers(general, 'StartChipRow')[0], column: integers(general, 'StartChipColumn')[0] };
  return {
    id, sourceResource: resourceId, dimensions: { width, height }, chipOrigin: { x: originX, y: originY }, chipSize: { width: chipWidth, height: chipHeight },
    start, startIsDeclaredMapEntry: true, chips, coordinateIndex, backgroundTiles: tiles,
    validation: { chipCount: chips.length, coordinateCount: coordinateIndex.length, startExists: keys.has(`${start.row},${start.column}`), unresolvedEdges },
    legacySections: decoded,
    notes: ['Map entry and next-chip fields are copied from the original MMI; global new-game map and movement rules are not inferred.', 'Tile suffix Mrc denotes row and column; verified against all map dimensions.'],
  };
}

export function parseCountedList(buffer, recordBytes, encoding = 'gbk') {
  requireBytes(buffer, 0, recordBytes, 'Counted legacy list');
  const count = buffer.readUInt16LE(0);
  if (buffer.length !== (count + 1) * recordBytes) throw new Error(`Counted-list length mismatch: ${count} x ${recordBytes}, actual ${buffer.length}`);
  if (buffer.readUInt16LE(2) !== 0x0400) throw new Error('Unsupported counted-list version');
  const records = [];
  for (let index = 1; index <= count; index++) {
    const offset = index * recordBytes;
    const id = buffer.readUInt16LE(offset);
    const byteLength = buffer.readUInt16LE(offset + 2);
    if (byteLength > recordBytes - 4) {
      // Original blank records use 0xffff rather than a text length.
      records.push({ id, byteLength, rawHex: buffer.subarray(offset, offset + recordBytes).toString('hex') });
      continue;
    }
    const text = new TextDecoder(encoding, { fatal: true }).decode(buffer.subarray(offset + 4, offset + 4 + byteLength));
    records.push({ id, byteLength, text });
  }
  return { count, recordBytes, encoding, records };
}

export function parseWordRecords(buffer, recordBytes) {
  if (recordBytes % 2 || buffer.length % recordBytes) throw new Error(`Invalid word record array: ${buffer.length}/${recordBytes}`);
  return Array.from({ length: buffer.length / recordBytes }, (_, index) => ({
    index,
    words: Array.from({ length: recordBytes / 2 }, (_, word) => buffer.readUInt16LE(index * recordBytes + word * 2)),
  }));
}

export function parseN1Program(buffer) {
  requireBytes(buffer, 0, 30, 'N1 program header');
  if (buffer.toString('ascii', 0, 20).trim() !== 'N1 Group') throw new Error('Not an N1 Group program');
  const count = buffer.readUInt16LE(22);
  const recordBytes = buffer.readUInt16LE(24);
  if (recordBytes !== 12 || buffer.length !== 30 + count * recordBytes) throw new Error('Unsupported N1 program dimensions');
  const nodes = parseWordRecords(buffer.subarray(30), recordBytes);
  return {
    sourceResource: 'install/event/n1_whole.n1e', versionRaw: buffer.readUInt16LE(20), nodeCount: count, recordBytes,
    remainingHeaderWords: [buffer.readUInt16LE(26), buffer.readUInt16LE(28)],
    semanticsStatus: 'requires-disassembly',
    nodes: nodes.map(({ index, words }) => ({ index, words })),
  };
}
