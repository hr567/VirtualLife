import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { DecodedBitmap, GameMap, Point, Snapshot } from './types';

interface Decoration {
  id: string; position: Point; frames: DecodedBitmap[]; foreground: boolean;
  frame: number; displayedFrame: number; interval: number; repeat: boolean; delay: number; lastTick: number; initial: boolean; playing: boolean;
}
interface PaletteCycle { first: number; last: number; type: number; interval: number; lastTick: number; position: number; direction: number }
type Field = string | { signed?: number[]; bytes?: number[] };
const number = (value: Field | undefined, index = 0): number => typeof value === 'object' ? (value.signed ?? value.bytes)?.[index] ?? 0 : 0;

/** 0x4170b0 / 0x417550 / 0x410030: decorative animations precede actors; bitmap floaters follow them. */
export class MapDecorationsView {
  #map = ''; #loadSerial = -1; #active = false; #paused = false; #generation = 0; #loaded = false;
  #items: Decoration[] = []; #cycles: PaletteCycle[] = []; #palette = new Uint8Array(1024); #initialPalette = new Uint8Array(1024);
  #raf = 0; #clock = 0;
  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient, private inform: (message: string) => void) {
    document.addEventListener('visibilitychange', () => { if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); });
  }
  hide(): void {
    if (!this.#active) return;
    this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0;
    for (const item of this.#items) this.renderer.removeAnimatedSprite(item.id);
    this.renderer.paletteColors();
  }
  pause(paused: boolean): void { this.#paused = paused; if (paused) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); }
  async show(map: GameMap, state?: Snapshot['mapDecorations']): Promise<void> {
    const entering = !this.#active, loadSerial = state?.mapId === map.id ? state.loadSerial : -1;
    this.#active = true;
    if (!this.#loaded || this.#map !== map.id || this.#loadSerial !== loadSerial) {
      const generation = ++this.#generation;
      for (const item of this.#items) this.renderer.removeAnimatedSprite(item.id);
      this.#items = []; this.#cycles = []; this.#map = map.id; this.#loadSerial = loadSerial; this.#clock = 0; this.#loaded = false;
      this.#palette = new Uint8Array(1024); this.#initialPalette = new Uint8Array(1024);
      const animations = new Map<string, DecodedBitmap[]>(), bitmaps = new Map<string, DecodedBitmap>();
      const directory = map.sourceResource.slice(0, map.sourceResource.lastIndexOf('/') + 1);
      for (const [name, fields] of Object.entries(map.legacySections)) {
        const match = /^(Spirit|Floater)_(\d+)$/.exec(name); if (!match) continue;
        const foreground = match[1] === 'Floater', index = Number(match[2]);
        const resource = directory + String(fields.File).replaceAll('\\', '/').toLowerCase();
        let frames: DecodedBitmap[];
        if (foreground) {
          let bitmap = bitmaps.get(resource);
          if (!bitmap) {
            bitmap = await this.assets.bitmap(resource);
            const key = bitmap.paletteIndices?.[0];
            if (key !== undefined) for (let pixel = 0; pixel < bitmap.paletteIndices!.length; pixel++) if (bitmap.paletteIndices![pixel] === key) bitmap.pixels[pixel * 4 + 3] = 0;
            bitmaps.set(resource, bitmap);
          }
          frames = [bitmap];
        } else {
          let decoded = animations.get(resource);
          if (!decoded) {
            const flic = await this.engine.openFlic(await this.assets.bytes(resource));
            try { decoded = [flic]; for (let frame = 1; frame < flic.frameCount; frame++) decoded.push(await this.engine.nextFlic(flic.decoderId)); }
            finally { await this.engine.closeFlic(flic.decoderId); }
            animations.set(resource, decoded);
          }
          frames = decoded;
        }
        if (!this.#active || generation !== this.#generation) return;
        this.#items.push({ id: `map-decoration-${name}`, position: { x: number(fields.Pos), y: number(fields.Pos, 1) }, frames, foreground,
          frame: 0, displayedFrame: 0, interval: foreground ? Infinity : Math.trunc(1000 / number(fields.Speed)), repeat: number(fields.Repeat) === 1,
          delay: state?.delaysMs[index] ?? Infinity, lastTick: -Infinity, initial: true, playing: true });
      }
      for (const [name, fields] of Object.entries(map.legacySections)) if (/^PalCycling_\d+$/.test(name)) this.#cycles.push({ first: number(fields.FirstColor), last: number(fields.LastColor), type: number(fields.Type), interval: Math.trunc(1000 / number(fields.Speed)), lastTick: -Infinity, position: number(fields.FirstColor), direction: 1 });
      if (this.#cycles.length) {
        const bytes = await this.assets.bytes(map.backgroundTiles[0]!.resourceId), header = new DataView(bytes), offset = 14 + header.getUint32(14, true);
        for (const cycle of this.#cycles) for (let index = cycle.first; index <= cycle.last; index++) this.#palette.set([header.getUint8(offset + index * 4 + 2), header.getUint8(offset + index * 4 + 1), header.getUint8(offset + index * 4), 255], index * 4);
        this.#initialPalette.set(this.#palette);
      }
      if (!this.#active || generation !== this.#generation) return;
      this.#loaded = true;
    } else if (!entering) { this.#schedule(); return; }
    for (const item of this.#items) this.renderer.setMapDecoration(item.id, item.frames[item.displayedFrame]!, item.position, item.foreground);
    this.renderer.paletteColors(this.#palette); this.#schedule();
  }
  #schedule(): void {
    if (!this.#active || this.#paused || this.#raf || document.hidden) return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (this.renderer.map?.id !== this.#map) { this.#active = false; return; }
      if (document.querySelector('dialog[open]')) { this.#schedule(); return; }
      this.#clock = now;
      try {
        for (const item of this.#items) {
          if (item.foreground) continue;
          const image = item.frames[item.frame]!, x = item.position.x - this.renderer.camera.x, y = item.position.y - this.renderer.camera.y;
          if (x <= -image.width || y <= -image.height || x >= 640 || y >= 480) continue;
          let changed = false;
          if (item.repeat) {
            if (this.#clock - item.lastTick > item.interval) {
              item.lastTick = this.#clock;
              if (item.initial) item.initial = false;
              else { item.frame = (item.frame + 1) % item.frames.length; changed = true; }
            }
          } else if (item.playing) {
            if (this.#clock - item.lastTick > item.interval) {
              item.lastTick = this.#clock;
              if (item.frame + 1 === item.frames.length) { item.playing = false; item.frame = 0; }
              else { item.frame++; changed = true; }
            }
          } else if (this.#clock - item.lastTick > item.delay) { item.lastTick = this.#clock; item.playing = true; }
          if (changed) { item.displayedFrame = item.frame; this.renderer.animatedSpriteFrame(item.id, item.frames[item.displayedFrame]!); }
        }
        let colorsChanged = false;
        for (const cycle of this.#cycles) if (this.#clock - cycle.lastTick > cycle.interval) {
          cycle.lastTick = this.#clock; colorsChanged = true;
          if (cycle.type === 0) {
            const last = this.#palette.slice(cycle.last * 4, cycle.last * 4 + 4);
            this.#palette.copyWithin((cycle.first + 1) * 4, cycle.first * 4, cycle.last * 4); this.#palette.set(last, cycle.first * 4);
          } else if (cycle.type === 1) {
            cycle.position += cycle.direction;
            if (cycle.position === cycle.last || cycle.position === cycle.first) cycle.direction *= -1;
            else this.#palette.set(this.#initialPalette.subarray((cycle.direction === 1 ? cycle.first : cycle.position) * 4, (cycle.direction === 1 ? cycle.first : cycle.position) * 4 + 4), cycle.position * 4);
          }
        }
        if (colorsChanged) this.renderer.paletteColors(this.#palette);
      } catch (error) { this.inform(`地图动画暂停：${String(error)}`); this.pause(true); }
      this.#schedule();
    });
  }
}
