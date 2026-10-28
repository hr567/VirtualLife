import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { DecodedBitmap, GameMap, OpenedFlic, Snapshot } from './types';

interface SpiritAnimation { resource: string; flic: OpenedFlic; image: DecodedBitmap }

/** FUN_0041e540/0041e880/0041e8f0: original age, facing, anchor and layer rules. */
export class SpiritsView {
  #active = false;
  #generation = 0;
  #animations = new Map<number, SpiritAnimation>();
  #raf = 0;
  #lastTime = 0;
  #elapsed = 0;
  #paused = false;

  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient, private inform: (message: string) => void) {
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') { cancelAnimationFrame(this.#raf); this.#raf = 0; }
      else this.#schedule();
    });
  }

  hide(): void {
    this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    for (let index = 0; index < 5; index++) this.renderer.removeAnimatedSprite(`map-spirit-${index}`);
  }
  pause(paused: boolean): void { this.#paused = paused; this.#lastTime = 0; if (paused) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); }

  async show(snapshot: Snapshot, map: GameMap, facing = 0): Promise<void> {
    const state = snapshot.spirits, stage = snapshot.life?.stage;
    if (!state || stage === undefined || stage === 2 || snapshot.life?.traveling) { this.hide(); return; }
    this.#active = true; const generation = ++this.#generation;
    const age = [1, 1, 1, 2, 2, 4, 3][stage];
    if (age === undefined) { this.hide(); return; }
    for (let index = 0; index < 5; index++) {
      const attached = state.active === index;
      const resource = this.assets.resolveLegacy(`spirit/${age}${['xr', 'dl', 'sa', 'fd', 'yb'][index]}${['3a', '3b', '3c', '3d'][attached ? facing % 4 : 0]}.flc`);
      let animation = this.#animations.get(index);
      if (animation?.resource !== resource) {
        if (animation) await this.engine.closeFlic(animation.flic.decoderId);
        this.#animations.delete(index);
        const flic = await this.engine.openFlic(await this.assets.bytes(resource));
        if (!this.#active || generation !== this.#generation) { await this.engine.closeFlic(flic.decoderId); return; }
        animation = { resource, flic, image: flic }; this.#animations.set(index, animation);
      }
      if (!this.#active || generation !== this.#generation) return;
      const cell = state.positions[index];
      if (!cell) continue;
      const position = attached ? { x: 270, y: 90 } : {
        x: Math.trunc((map.chipSize.width - animation.flic.width) / 2) + Math.trunc(cell.column * map.chipSize.width / 2) + map.chipOrigin.x,
        y: Math.trunc(cell.row * map.chipSize.height / 2) - 50 + map.chipOrigin.y,
      };
      this.renderer.setMapSprite(`map-spirit-${index}`, animation.image, position, attached);
    }
    this.pause(!!snapshot.pending);
  }

  #schedule(): void {
    if (!this.#active || this.#paused || this.#raf || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      this.#elapsed += this.#lastTime ? now - this.#lastTime : 0; this.#lastTime = now;
      if (this.#elapsed <= 100) { this.#schedule(); return; }
      this.#elapsed = 0;
      const generation = this.#generation;
      void (async () => {
        for (const [index, animation] of this.#animations) {
          const frame = await this.engine.nextFlic(animation.flic.decoderId);
          if (!this.#active || generation !== this.#generation) return;
          animation.image = frame; this.renderer.animatedSpriteFrame(`map-spirit-${index}`, frame);
        }
      })().catch(error => { this.inform(`神灵动画暂停：${String(error)}`); this.pause(true); }).finally(() => this.#schedule());
    });
  }
}
