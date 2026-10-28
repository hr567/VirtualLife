import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { DecodedBitmap, EndingSummaryState } from './types';

/** 0044e2d0: indexed text/portrait edge remapping followed by the native palette. */
export class EndingSummaryView {
  #scene = ''; #frames: DecodedBitmap[] = []; #portrait: DecodedBitmap | undefined;
  #remaps: Uint8Array[] = []; #text: DecodedBitmap | undefined;
  #scroll = NaN; #actorFrame = -1; #actorY = NaN; #visibleActor = false; #visibleText = false;
  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient) {}
  hide(): void { this.#scene = ''; this.#scroll = this.#actorY = NaN; this.#actorFrame = -1; this.#visibleActor = this.#visibleText = false; }
  async show(state: EndingSummaryState): Promise<void> {
    const rules = this.assets.rules.endingSummary!, [left, top, right, bottom] = rules.clip;
    if (this.#scene !== state.background) {
      await this.renderer.setScene([{ resourceId: state.background, x: 0, y: 0, width: 640, height: 480 }], this.assets);
      const flic = await this.engine.openFlic(await this.assets.bytes(state.actor.resource));
      try { this.#frames = [flic]; for (let frame = 1; frame < flic.frameCount; frame++) this.#frames.push(await this.engine.nextFlic(flic.decoderId)); }
      finally { await this.engine.closeFlic(flic.decoderId); }
      this.#portrait = await this.assets.bitmap(state.portrait, 1);
      this.#remaps = this.#buildRemaps(state.originalPalette);
      this.#scene = state.background; this.#scroll = this.#actorY = NaN; this.#actorFrame = -1;
      this.#visibleText = this.#visibleActor = false;
    }
    const palette = new Uint8Array(1024);
    for (let index = 0; index < 256; index++) palette.set([...state.palette[index]!.slice(0, 3), 255], index * 4);
    this.renderer.paletteColors(palette);
    if (state.showText) {
      if (this.#scroll !== state.scrollY || !this.#visibleText) {
        this.#text = this.#drawText(state, left, top, right - left, bottom - top);
        if (!this.#visibleText) this.renderer.setAnimatedSprite('ending-summary-text', this.#text, { x: left, y: top }, 0);
        else this.renderer.animatedSpriteFrame('ending-summary-text', this.#text);
        this.#scroll = state.scrollY;
      }
    } else this.renderer.removeAnimatedSprite('ending-summary-text');
    this.#visibleText = state.showText;
    if (state.showActor) {
      if (this.#actorFrame !== state.actor.frame || this.#actorY !== state.actorY || !this.#visibleActor) {
        const frame = this.#frames[state.actor.frame % this.#frames.length]!;
        const actor = this.#composite(frame, state.actorY, top);
        const position = { x: 110, y: state.actorY }, clip = { x: left, y: top, width: right - left, height: bottom - top };
        if (!this.#visibleActor) {
          this.renderer.setAnimatedSprite('ending-summary-actor', actor, position, 1); this.renderer.animatedSpriteClip('ending-summary-actor', clip);
        } else {
          this.renderer.animatedSpriteFrame('ending-summary-actor', actor); this.renderer.animatedSpritePosition('ending-summary-actor', position);
        }
        this.#actorFrame = state.actor.frame; this.#actorY = state.actorY;
      }
    } else this.renderer.removeAnimatedSprite('ending-summary-actor');
    this.#visibleActor = state.showActor;
  }
  #buildRemaps(palette: number[][]): Uint8Array[] {
    const rows: Uint8Array[] = [];
    for (let row = 0; row < 70; row++) {
      const remap = new Uint8Array(256);
      for (let color = 1; color < 256; color++) {
        const target = palette[color]!.slice(0, 3).map(value => Math.floor(value * row / 70));
        let distance = Infinity;
        for (let index = 0; index < 256; index++) {
          const candidate = palette[index]!, square = (candidate[0]! - target[0]!) ** 2 + (candidate[1]! - target[1]!) ** 2 + (candidate[2]! - target[2]!) ** 2;
          if (square < distance) { distance = square; remap[color] = index; }
        }
      }
      rows.push(remap);
    }
    return rows;
  }
  #composite(image: DecodedBitmap, imageY: number, top: number): DecodedBitmap {
    const portrait = this.#portrait!;
    if (!image.paletteIndices || !portrait.paletteIndices) throw new Error('结局人物缺少原版调色板索引');
    const pixels = new Uint8Array(image.width * image.height * 4), indices = new Uint8Array(image.width * image.height);
    for (let y = 0; y < image.height; y++) {
      const row = imageY + y - top, source = row < 70 ? row : row >= 354 ? 424 - row : 70;
      const remap = source >= 0 && source < 70 ? this.#remaps[source] : undefined;
      for (let x = 0; x < image.width; x++) {
        const index = y * image.width + x, portraitIndex = y * portrait.width + x;
        // The native FLC uses an opaque BltFast; only the portrait applies color key 1.
        let color = image.paletteIndices[index]!;
        if (x < portrait.width && y < portrait.height && portrait.pixels[portraitIndex * 4 + 3]) color = portrait.paletteIndices[portraitIndex]!;
        indices[index] = color && remap ? remap[color]! : color;
        pixels[index * 4 + 3] = 255;
      }
    }
    return { width: image.width, height: image.height, pixels, paletteIndices: indices };
  }
  #drawText(state: EndingSummaryState, left: number, top: number, width: number, height: number): DecodedBitmap {
    const rules = this.assets.rules.endingSummary!, canvas = document.createElement('canvas'); canvas.width = width; canvas.height = height;
    const context = canvas.getContext('2d')!;
    context.font = `${rules.fontSize}px SimSun, "Songti SC", serif`; context.textBaseline = 'top'; context.fillStyle = '#fff';
    context.letterSpacing = `${rules.characterSpacing}px`;
    for (const paragraph of state.paragraphs) {
      const y = paragraph.y + state.scrollY - top;
      if (y >= height || y + paragraph.height <= 0) continue;
      const text = new TextDecoder('gbk').decode(new Uint8Array(paragraph.bytes));
      for (const [line, value] of text.split('\n').entries()) context.fillText(value, paragraph.x - left, y + line * rules.lineHeight);
    }
    const pixels = new Uint8Array(context.getImageData(0, 0, width, height).data), indices = new Uint8Array(width * height);
    for (let y = 0; y < height; y++) {
      const color = y < 70 ? 246 + Math.floor(y * 9 / 69) : y >= 354 ? 246 + Math.floor((424 - y) * 9 / 69) : 255;
      for (let x = 0; x < width; x++) { const index = y * width + x; pixels[index * 4 + 3] = pixels[index * 4 + 3]! >= 128 ? 255 : 0; indices[index] = color; }
    }
    return { width, height, pixels, paletteIndices: indices };
  }
}
