import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { DecodedBitmap, Point } from './types';

export interface NativeImage extends Point { resource: string; width: number; height: number; transparent?: number | 'firstPixel' | null }
export interface NativeFrames extends NativeImage { frameCount: number; interval: number; frame?: number }
export interface SourceRect extends Point { width: number; height: number }

/** Reuses original indexed images while leaving all gameplay clocks in Rust. */
export class NativeRendering {
  #bitmaps = new Map<string, DecodedBitmap>();
  #animations = new Map<string, { bytes: ArrayBuffer; decoderId: number; frame: number; image: DecodedBitmap; count: number; key?: number }>();
  #sprites = new Map<string, { resource: string; frame: number; z: number }>();
  #wanted = new Set<string>();
  #sounds = new Map<string, Promise<HTMLAudioElement>>();
  #palettes = new Map<string, Promise<Uint8Array<ArrayBuffer>>>();
  #loop: string | undefined;
  #audio = new Set<HTMLAudioElement>();
  #soundEnabled = true;
  #active = true;
  constructor(readonly renderer: GameRenderer, readonly assets: Assets, readonly engine: EngineClient, readonly prefix: string) {}
  begin(): void { this.#active = true; this.#wanted.clear(); }
  end(): void { for (const id of this.#sprites.keys()) if (!this.#wanted.has(id)) { this.renderer.removeAnimatedSprite(id); this.#sprites.delete(id); } }
  clear(): void { for (const id of this.#sprites.keys()) this.renderer.removeAnimatedSprite(id); this.#sprites.clear(); }
  hide(): void { this.#active = false; this.#loop = undefined; this.clear(); this.pauseSounds(); const animations = [...this.#animations.values()]; this.#animations.clear(); for (const animation of animations) void this.engine.closeFlic(animation.decoderId); }
  pauseSounds(): void { for (const sound of this.#audio) sound.pause(); }
  setSound(enabled: boolean): void { this.#soundEnabled = enabled; if (!enabled) this.pauseSounds(); }
  palette(resource: string): Promise<Uint8Array<ArrayBuffer>> {
    const cached = this.#palettes.get(resource); if (cached) return cached;
    const pending = this.assets.bytes(resource).then(bytes => {
      const view = new DataView(bytes), palette = new Uint8Array(1024); for (let i = 0; i < 256; i++) palette[i * 4 + 3] = 255;
      if (view.getUint16(0, true) === 0x4d42) {
        const offset = 14 + view.getUint32(14, true), count = view.getUint32(46, true) || 256;
        for (let i = 0; i < count; i++) palette.set([view.getUint8(offset + i * 4 + 2), view.getUint8(offset + i * 4 + 1), view.getUint8(offset + i * 4), 255], i * 4);
      } else {
        let offset = 128; while (view.getUint16(offset + 4, true) !== 0xf1fa) offset += view.getUint32(offset, true);
        const chunks = view.getUint16(offset + 6, true); offset += 16;
        for (let chunk = 0; chunk < chunks; chunk++) {
          const size = view.getUint32(offset, true), kind = view.getUint16(offset + 4, true); let cursor = offset + 6;
          if (kind === 4 || kind === 11) {
            const packets = view.getUint16(cursor, true); cursor += 2; let index = 0;
            for (let packet = 0; packet < packets; packet++) { index += view.getUint8(cursor++); const count = view.getUint8(cursor++) || 256;
              if (index + count > 256) throw new Error('原动画调色板越界');
              for (let color = 0; color < count; color++, index++) for (let channel = 0; channel < 3; channel++) { const value = view.getUint8(cursor++); palette[index * 4 + channel] = kind === 11 ? value << 2 : value & 0xfc; }
            }
          }
          offset += size;
        }
      }
      return palette;
    }); this.#palettes.set(resource, pending); return pending;
  }
  async bitmap(resource: string, key: NativeImage['transparent'] = 1): Promise<DecodedBitmap> {
    const id = `${resource}:${key}`, cached = this.#bitmaps.get(id); if (cached) return cached;
    let image = await this.assets.bitmap(resource, typeof key === 'number' ? key : undefined);
    if (key === 'firstPixel') image = this.#colorKey(image, image.paletteIndices?.[0]);
    this.#bitmaps.set(id, image); return image;
  }
  #colorKey(image: DecodedBitmap, key?: number): DecodedBitmap {
    if (key === undefined || !image.paletteIndices) return image;
    const pixels = new Uint8Array(image.pixels);
    for (let index = 0; index < image.paletteIndices.length; index++) if (image.paletteIndices[index] === key) pixels[index * 4 + 3] = 0;
    return { ...image, pixels };
  }
  async frame(resource: string, target: number, key: NativeImage['transparent'] = 1): Promise<DecodedBitmap> {
    const id = `${resource}:${key}`; let cached = this.#animations.get(id);
    if (!cached) {
      const bytes = await this.assets.bytes(resource), flic = await this.engine.openFlic(bytes.slice(0), typeof key === 'number' ? key : undefined);
      cached = { bytes, decoderId: flic.decoderId, frame: 0, image: flic, count: flic.frameCount, key: typeof key === 'number' ? key : flic.paletteIndices?.[0] };
      if (this.#animations.size >= 12) { const oldest = this.#animations.keys().next().value!, item = this.#animations.get(oldest)!; this.#animations.delete(oldest); await this.engine.closeFlic(item.decoderId); }
    }
    this.#animations.delete(id); this.#animations.set(id, cached); target %= cached.count;
    if (target < cached.frame) { await this.engine.closeFlic(cached.decoderId); const flic = await this.engine.openFlic(cached.bytes.slice(0), cached.key); cached.decoderId = flic.decoderId; cached.image = flic; cached.frame = 0; }
    while (cached.frame < target) { cached.image = await this.engine.nextFlic(cached.decoderId); cached.frame++; }
    if (key === null) { const pixels = new Uint8Array(cached.image.pixels); for (let i = 3; i < pixels.length; i += 4) pixels[i] = 255; return { ...cached.image, pixels }; }
    return cached.image;
  }
  image(id: string, image: DecodedBitmap, position: Point, z: number, signature = id, frame = 0, source?: SourceRect): void {
    id = `${this.prefix}-${id}`; this.#wanted.add(id); const old = this.#sprites.get(id);
    if (old?.resource !== signature || old.z !== z) this.renderer.setAnimatedSprite(id, image, position, z);
    else { if (old.frame !== frame) this.renderer.animatedSpriteFrame(id, image); this.renderer.animatedSpritePosition(id, position); }
    this.#sprites.set(id, { resource: signature, frame, z }); if (source) this.renderer.animatedSpriteSource(id, source);
  }
  async draw(id: string, image: NativeImage, z: number, position: Point = image, source?: SourceRect): Promise<void> {
    this.image(id, await this.bitmap(image.resource, image.transparent), position, z, image.resource, 0, source);
  }
  async animation(id: string, meta: NativeFrames, z: number): Promise<void> {
    const frame = (meta.frame ?? 0) % meta.frameCount;
    this.image(id, await this.frame(meta.resource, frame, meta.transparent), meta, z, meta.resource, frame);
  }
  rectangle(id: string, x: number, y: number, width: number, height: number, z: number, color = [0, 0, 0, 255]): void {
    if (width <= 0 || height <= 0) return;
    const pixels = new Uint8Array(width * height * 4); for (let i = 0; i < pixels.length; i += 4) pixels.set(color, i);
    this.image(id, { width, height, pixels }, { x, y }, z, `${id}:${width}:${height}:${color}`);
  }
  async strip(id: string, meta: NativeImage, x: number, y: number, width: number, z: number): Promise<void> {
    if (width <= 0) return; const source = await this.bitmap(meta.resource, meta.transparent);
    const pixels = new Uint8Array(width * source.height * 4), paletteIndices = source.paletteIndices ? new Uint8Array(width * source.height) : undefined;
    for (let row = 0; row < source.height; row++) for (let column = 0; column < width; column++) {
      const from = (row * source.width + column % source.width), to = row * width + column;
      pixels.set(source.pixels.subarray(from * 4, from * 4 + 4), to * 4); if (paletteIndices) paletteIndices[to] = source.paletteIndices![from]!;
    }
    this.image(id, { width, height: source.height, pixels, paletteIndices }, { x, y }, z, `${meta.resource}:${width}`);
  }
  loop(resource?: string): void { this.#loop = resource; for (const audio of this.#audio) if (audio.loop && audio.dataset.resource !== resource) audio.pause(); if (resource) void this.sound(resource); }
  stopSound(resource?: string): void { for (const sound of this.#audio) if (sound.dataset.resource === resource) { sound.pause(); sound.currentTime = 0; } }
  async sound(resource: string | undefined, restart = false): Promise<void> {
    if (!resource || !this.#active || !this.#soundEnabled || document.hidden || document.querySelector('dialog[open]')) return;
    let pending = this.#sounds.get(resource);
    if (!pending) { pending = this.assets.bytes(resource).then(bytes => { const audio = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); audio.dataset.resource = resource; this.#audio.add(audio); return audio; }); this.#sounds.set(resource, pending); }
    const audio = await pending; if (!this.#active || !this.#soundEnabled || document.hidden) return;
    audio.loop = resource === this.#loop; if (restart || audio.paused) { if (restart || !audio.loop) audio.currentTime = 0; void audio.play().catch(() => {}); }
  }
}
