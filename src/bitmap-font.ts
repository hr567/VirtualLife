import type { DecodedBitmap } from './types';

/** Native ASCFont.16: each byte is one row of an 8×16 ASCII glyph. */
export function asciiText(font: ArrayBuffer, text: string, rgb: [number, number, number] = [255, 255, 255]): DecodedBitmap {
  const glyphs = new Uint8Array(font);
  const width = Math.max(1, text.length * 8);
  const pixels = new Uint8Array(width * 16 * 4);
  for (let index = 0; index < text.length; index++) {
    const glyph = (text.charCodeAt(index) & 255) * 16;
    for (let y = 0; y < 16; y++) for (let x = 0; x < 8; x++) {
      if (!(glyphs[glyph + y]! & (128 >> x))) continue;
      const offset = (y * width + index * 8 + x) * 4;
      pixels.set([...rgb, 255], offset);
    }
  }
  return { width, height: 16, pixels };
}

let gb2312: Map<string, number> | undefined;

/** Native 0040e580: ASCII 8×16 and GB2312 16×16 pixels from the original fonts. */
export function indexedText(ascii: ArrayBuffer, chinese: ArrayBuffer, text: string, color: number, palette: Uint8Array): DecodedBitmap {
  if (!gb2312) {
    gb2312 = new Map(); const decoder = new TextDecoder('gbk');
    for (let high = 0xa1; high <= 0xfe; high++) for (let low = 0xa1; low <= 0xfe; low++) {
      const character = decoder.decode(new Uint8Array([high, low]));
      if (character.length === 1 && character !== '\ufffd' && !gb2312.has(character)) gb2312.set(character, (high * 94 + low - 0x3bbf) * 32);
    }
  }
  const characters = Array.from(text), width = Math.max(1, characters.reduce((length, character) => length + (character.charCodeAt(0) < 0x80 ? 8 : 16), 0));
  const pixels = new Uint8Array(width * 16 * 4), paletteIndices = new Uint8Array(width * 16), fonts = [new Uint8Array(ascii), new Uint8Array(chinese)];
  let column = 0;
  for (const character of characters) {
    const code = character.charCodeAt(0), wide = code >= 0x80, bytes = wide ? fonts[1]! : fonts[0]!, start = wide ? gb2312.get(character) : (code >= 0x20 && code <= 0x7f ? code : 0x20) * 16;
    const glyphWidth = wide ? 16 : 8;
    if (start !== undefined) for (let y = 0; y < 16; y++) for (let x = 0; x < glyphWidth; x++) {
      if (!(bytes[start + y * (wide ? 2 : 1) + Math.trunc(x / 8)]! & (128 >> (x % 8)))) continue;
      const index = y * width + column + x; pixels.set([palette[color * 4]!, palette[color * 4 + 1]!, palette[color * 4 + 2]!, 255], index * 4); paletteIndices[index] = color;
    }
    column += glyphWidth;
  }
  return { width, height: 16, pixels, paletteIndices };
}
