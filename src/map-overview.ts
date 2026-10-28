import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { NativeRendering } from './native-rendering';
import type { GameMap, Point, Snapshot } from './types';

/** 00402800: the current map's independently scaled mosaic and six markers. */
export class MapOverviewView {
  #active = false; #generation = 0; #raf = 0; #step: Promise<void> | undefined;
  #lastTime = 0; #clock = 0; #lastTick = 0; #frame = 0; #color = 0;
  #render: NativeRendering;
  #snapshot: Snapshot | undefined; #map: GameMap | undefined; #camera: Point = { x: 0, y: 0 }; #palette = new Uint8Array(1024);
  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private close: () => Promise<void>, private inform: (text: string) => void) {
    this.#render = new NativeRendering(renderer, assets, engine, 'map-overview');
    document.addEventListener('keydown', event => { if (this.#active && event.key === 'Escape' && !document.querySelector('dialog[open]')) { event.preventDefault(); void this.close(); } });
    ui.addEventListener('contextmenu', event => { if (this.#active) { event.preventDefault(); void this.close(); } });
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); });
  }
  get active(): boolean { return this.#active; }
  hide(): void { this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#render.hide(); this.ui.classList.remove('map-overview-active'); this.renderer.paletteColors(); }
  async show(snapshot: Snapshot, camera: Point): Promise<void> {
    const map = this.assets.maps.maps.find(item => item.id === snapshot.mapId)!;
    const metadata = this.assets.rules.mapOverview!.maps.find(item => item.mapId === map.id && item.special === !!snapshot.life?.traveling && (item.special || item.index === snapshot.life?.stage));
    if (!metadata) throw new Error('原地图缩略参数不存在。');
    this.#active = true; const generation = ++this.#generation; this.#snapshot = snapshot; this.#map = map; this.#camera = { ...camera };
    this.#clock = this.#lastTick = this.#frame = this.#color = this.#lastTime = 0;
    this.ui.replaceChildren(); this.ui.classList.add('map-overview-active'); this.ui.dataset.nativePhase = 'mapOverview'; this.ui.dataset.nativeReady = 'false';
    const back = document.createElement('button'); back.textContent = '返回'; back.className = 'map-overview-back'; back.setAttribute('aria-label', '关闭地图浏览'); back.onclick = () => { void this.close(); }; this.ui.append(back);
    await this.renderer.setScene(metadata.tiles.map(tile => ({ resourceId: tile.resource, x: tile.x, y: tile.y, width: metadata.tileWidth, height: metadata.tileHeight })), this.assets);
    if (!this.#active || generation !== this.#generation) return;
    this.#palette = new Uint8Array(await this.#render.palette(metadata.tiles[0]!.resource)); this.renderer.paletteColors(this.#palette);
    await this.#draw(); this.ui.dataset.nativeReady = 'true'; this.#schedule();
  }
  async #draw(): Promise<void> {
    const snapshot = this.#snapshot!, map = this.#map!, r = this.assets.rules.mapOverview!, metadata = r.maps.find(item => item.mapId === map.id && item.special === !!snapshot.life?.traveling && (item.special || item.index === snapshot.life?.stage))!;
    const x = Math.trunc(this.#camera.x / metadata.columns), y = Math.trunc(this.#camera.y / metadata.rows), width = metadata.tileWidth, height = metadata.tileHeight;
    this.#render.begin();
    const box = (id: string, left: number, top: number, w: number, h: number): void => {
      const color = 242 + this.#color, pixels = new Uint8Array(w * h * 4), paletteIndices = new Uint8Array(w * h).fill(color);
      for (let index = 0; index < w * h; index++) pixels.set([this.#palette[color * 4]!, this.#palette[color * 4 + 1]!, this.#palette[color * 4 + 2]!, 255], index * 4);
      this.#render.image(id, { width: w, height: h, pixels, paletteIndices }, { x: left, y: top }, 1, `${id}:${color}`);
    };
    box('top', x, y, width + 1, 1); box('bottom', x, y + height, width + 1, 1); box('left', x, y, 1, height + 1); box('right', x + width, y, 1, height + 1);
    const player = { x: x + Math.trunc(width / 2) - 24, y: y + Math.trunc(height / 2) - 48 };
    if (!snapshot.life?.traveling && snapshot.life?.stage !== 2) for (let index = 0; index < 5; index++) {
      const cell = snapshot.spirits?.positions[index]; if (!cell) continue;
      const attached = snapshot.spirits?.active === index;
      const world = { x: Math.trunc((map.chipSize.width - 100) / 2) + Math.trunc(cell.column * map.chipSize.width / 2) + map.chipOrigin.x, y: Math.trunc(cell.row * map.chipSize.height / 2) - 50 + map.chipOrigin.y };
      const position = attached ? { x: player.x, y: y + Math.trunc(height / 2) - 72 } : { x: Math.trunc(world.x / metadata.columns), y: Math.trunc(world.y / metadata.rows) };
      await this.#render.animation(`spirit-${index}`, { ...r.markers[index + 1]!, ...position, transparent: 'firstPixel', frame: this.#frame }, index + 2);
    }
    await this.#render.animation('player', { ...r.markers[0]!, ...player, transparent: 'firstPixel', frame: this.#frame }, 7);
    this.#render.end();
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || document.hidden) return;
    this.#raf = requestAnimationFrame(now => { this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      this.#clock += this.#lastTime ? Math.max(0, now - this.#lastTime) : 0; this.#lastTime = now;
      if (this.#clock - this.#lastTick <= 100) { this.#schedule(); return; }
      this.#lastTick = this.#clock; this.#color = (this.#color + 1) % 5; this.#frame++;
      const generation = this.#generation;
      this.#step = this.#draw().catch(error => { if (generation === this.#generation) { this.inform(`地图浏览暂停：${String(error)}`); this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
