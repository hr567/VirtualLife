import { inflateSync } from 'node:zlib';

export interface Pixels { width: number; height: number; rgba: Uint8Array }

/** Read the lossless 8-bit RGB/RGBA screenshots produced by Chromium. */
export function png(bytes: Buffer): Pixels {
  if (bytes.subarray(1, 4).toString() !== 'PNG') throw new Error('Screenshot is not PNG');
  let width = 0, height = 0, channels = 0;
  const blocks: Buffer[] = [];
  for (let offset = 8; offset < bytes.length;) {
    const length = bytes.readUInt32BE(offset), type = bytes.toString('ascii', offset + 4, offset + 8);
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0); height = data.readUInt32BE(4);
      if (data[8] !== 8 || ![2, 6].includes(data[9]!) || data[12] !== 0) throw new Error('Unsupported screenshot format');
      channels = data[9] === 6 ? 4 : 3;
    }
    if (type === 'IDAT') blocks.push(data);
    offset += length + 12;
  }
  const source = inflateSync(Buffer.concat(blocks)), stride = width * channels;
  const decoded = new Uint8Array(stride * height);
  const paeth = (a: number, b: number, c: number): number => {
    const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
    return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
  };
  for (let y = 0; y < height; y++) {
    const filter = source[y * (stride + 1)]!;
    for (let x = 0; x < stride; x++) {
      const index = y * stride + x, a = x >= channels ? decoded[index - channels]! : 0;
      const b = y ? decoded[index - stride]! : 0, c = y && x >= channels ? decoded[index - stride - channels]! : 0;
      const predictor = [0, a, b, Math.floor((a + b) / 2), paeth(a, b, c)][filter];
      if (predictor === undefined) throw new Error('Invalid PNG filter');
      decoded[index] = (source[y * (stride + 1) + 1 + x]! + predictor) & 255;
    }
  }
  const rgba = new Uint8Array(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    rgba.set(decoded.subarray(i * channels, i * channels + 3), i * 4); rgba[i * 4 + 3] = 255;
  }
  return { width, height, rgba };
}

/** Independent source-art oracle for the uncompressed indexed original UI BMPs. */
export function bmp(bytes: Buffer): Pixels {
  if (bytes.toString('ascii', 0, 2) !== 'BM' || bytes.readUInt16LE(28) !== 8 || bytes.readUInt32LE(30) !== 0) throw new Error('Expected original indexed UI BMP');
  const width = bytes.readInt32LE(18), signedHeight = bytes.readInt32LE(22), height = Math.abs(signedHeight);
  const start = bytes.readUInt32LE(10), palette = 14 + bytes.readUInt32LE(14), stride = Math.ceil(width / 4) * 4;
  const count = bytes.readUInt32LE(46) || 256, rgba = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const index = bytes[start + (signedHeight > 0 ? height - 1 - y : y) * stride + x]!;
    const to = (y * width + x) * 4, p = palette + index * 4;
    rgba.set(index < count ? [bytes[p + 2]!, bytes[p + 1]!, bytes[p]!, 255] : [0, 0, 0, 255], to);
  }
  return { width, height, rgba };
}

export function blit(target: Pixels, image: Pixels, x: number, y: number, sourceY = 0, height = image.height): void {
  for (let row = 0; row < height; row++) for (let col = 0; col < image.width; col++) {
    const from = ((row + sourceY) * image.width + col) * 4, to = ((row + y) * target.width + col + x) * 4;
    target.rgba.set(image.rgba.subarray(from, from + 4), to);
  }
}

export function differingPixels(actual: Pixels, expected: Pixels): number {
  if (actual.width !== expected.width || actual.height !== expected.height) throw new Error('Pixel dimensions differ');
  let count = 0;
  for (let i = 0; i < actual.rgba.length; i += 4) if (actual.rgba[i] !== expected.rgba[i] || actual.rgba[i + 1] !== expected.rgba[i + 1] || actual.rgba[i + 2] !== expected.rgba[i + 2]) count++;
  return count;
}
