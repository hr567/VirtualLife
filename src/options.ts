import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { NativeRendering, type NativeImage } from './native-rendering';
import { masterVolume, setMasterVolume } from './audio';

type Page = 'menu' | 'volume' | 'exit';
type Control = { element: HTMLButtonElement; image: NativeImage; frame: number };

/** Original system and volume screens; browser save files use the platform dialog. */
export class OptionsView {
  #active = false; #page: Page = 'menu'; #generation = 0; #render: NativeRendering;
  #controls: Control[] = []; #hoverBack = false; #raf = 0; #step: Promise<void> | undefined;
  #clock = 0; #lastTime = 0; #lastTick = 0; #frames = [0, 0]; #first = true;
  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private close: () => Promise<void>, private save: () => void, private exit: () => Promise<void>, private volumeChanged: (value: number) => void, private inform: (text: string) => void) {
    this.#render = new NativeRendering(renderer, assets, engine, 'options');
    document.addEventListener('keydown', event => { if (!this.#active || this.#page !== 'exit' || event.repeat || document.querySelector('dialog[open]')) return; if (event.code === 'KeyY') void this.exit(); else if (event.code === 'KeyN') void this.show('menu'); });
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); });
  }
  get active(): boolean { return this.#active; }
  hide(): void { this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#render.hide(); this.renderer.paletteColors(); this.ui.classList.remove('options-active'); }
  async show(page: Page = 'menu'): Promise<void> {
    cancelAnimationFrame(this.#raf); this.#raf = 0; await this.#step; cancelAnimationFrame(this.#raf); this.#raf = 0;
    this.#active = true; this.#page = page; const generation = ++this.#generation; this.#lastTime = this.#clock = this.#lastTick = 0; this.#frames = [0, 0]; this.#first = true; this.#hoverBack = false;
    const rules = this.assets.rules.optionsRendering!, selected = page === 'menu' ? rules : rules[page];
    this.ui.hidden = false; this.ui.replaceChildren(); this.ui.classList.add('options-active'); this.ui.dataset.nativePhase = `options-${page}`; this.ui.dataset.nativeReady = 'false'; this.#controls = [];
    await this.renderer.setScene([{ resourceId: selected.background.resource, x: 0, y: 0, width: 640, height: 480 }], this.assets); this.#render.clear();
    if (!this.#active || generation !== this.#generation) return;
    const palette = new Uint8Array(await this.#render.palette(selected.background.resource)), backPalette = await this.#render.palette(selected.back[0]!.resource);
    palette.set(backPalette.subarray(162 * 4, 191 * 4), 162 * 4); this.renderer.paletteColors(palette);
    if (page === 'menu') for (const entry of rules.buttons) this.#button({ volume: '音量设置', save: '保存游戏', load: '读取游戏', exit: '离开游戏' }[entry.action], entry, () => { if (entry.action === 'save' || entry.action === 'load') this.save(); else void this.show(entry.action); });
    else if (page === 'exit') { this.#button('确定离开游戏', rules.exit.yes, () => { void this.exit(); }); this.#button('取消离开游戏', rules.exit.no, () => { void this.show('menu'); }); }
    else {
      const { track, thumb } = rules.volume, width = track.width - thumb.width;
      const control = document.createElement('input'); control.type = 'range'; control.min = '0'; control.max = '100'; control.step = '1'; control.value = String(masterVolume()); control.setAttribute('aria-label', '总音量'); control.className = 'native-volume-range';
      Object.assign(control.style, { left: `${track.x}px`, top: `${track.y - Math.trunc((thumb.height - track.height) / 2)}px`, width: `${track.width}px`, height: `${thumb.height}px` });
      control.oninput = () => { setMasterVolume(Number(control.value)); this.volumeChanged(masterVolume()); this.ui.dataset.nativeVolume = String(masterVolume()); };
      control.onpointerdown = event => { event.preventDefault(); control.setPointerCapture(event.pointerId); change(event); };
      const change = (event: PointerEvent): void => { const rect = this.ui.getBoundingClientRect(), local = (event.clientX - rect.left) * 640 / rect.width; control.value = String(Math.max(0, Math.min(100, Math.trunc((local - track.x - thumb.width / 2) * 100 / width)))); control.dispatchEvent(new Event('input', { bubbles: true })); };
      control.onpointermove = event => { if (control.hasPointerCapture(event.pointerId)) change(event); };
      this.ui.append(control); this.ui.dataset.nativeVolume = String(masterVolume());
    }
    const back = selected.back[0]!, control = document.createElement('button'); control.className = 'game-hitbox'; control.setAttribute('aria-label', page === 'menu' ? '返回游戏' : '返回系统操作'); Object.assign(control.style, { left: `${back.x}px`, top: `${back.y}px`, width: `${back.width}px`, height: `${back.height}px` });
    control.onpointerenter = control.onfocus = () => { this.#hoverBack = true; }; control.onpointerleave = control.onblur = () => { this.#hoverBack = false; }; control.onclick = () => { if (page === 'menu') void this.close(); else void this.show('menu'); }; this.ui.append(control);
    await this.#draw(); this.ui.dataset.nativeReady = 'true'; this.#schedule();
  }
  #button(label: string, image: NativeImage, action: () => void): void {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label); Object.assign(button.style, { left: `${image.x}px`, top: `${image.y}px`, width: `${image.width}px`, height: `${image.height / 3}px` }); button.onclick = action;
    const control: Control = { element: button, image, frame: 0 }; button.onpointerenter = () => { control.frame = 1; }; button.onpointerleave = () => { control.frame = 0; }; button.onpointerdown = () => { control.frame = 2; }; button.onpointerup = () => { control.frame = 1; }; this.#controls.push(control); this.ui.append(button);
  }
  async #draw(): Promise<void> {
    const rules = this.assets.rules.optionsRendering!, page = this.#page === 'menu' ? rules : rules[this.#page]; this.#render.begin();
    for (const [index, control] of this.#controls.entries()) { const image = control.image, height = image.height / 3; await this.#render.draw(`button-${index}`, image, 1, image, { x: 0, y: control.frame * height, width: image.width, height }); }
    if (this.#page === 'volume') { const { track, thumb } = rules.volume; await this.#render.draw('track', track, 1); await this.#render.draw('thumb', thumb, 2, { x: track.x + Math.trunc((track.width - thumb.width) * masterVolume() / 100), y: track.y - Math.trunc((thumb.height - track.height) / 2) }); }
    const back = page.back[this.#hoverBack ? 1 : 0]!; await this.#render.animation('back', { ...back, frame: this.#frames[this.#hoverBack ? 1 : 0] }, 3); this.#render.end();
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || document.hidden) return;
    this.#raf = requestAnimationFrame(now => { this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      this.#clock += this.#lastTime ? now - this.#lastTime : 0; this.#lastTime = now;
      if (this.#clock - this.#lastTick > 114) { this.#lastTick = this.#clock; if (this.#first) this.#first = false; else this.#frames = this.#frames.map((frame, index) => (frame + 1) % [6, 5][index]!); }
      this.#step = this.#draw().catch(error => { this.inform(`系统操作暂停：${String(error)}`); this.hide(); }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
