import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { BonusView } from './bonus';
import { asciiText } from './bitmap-font';
import type { Command, DecodedBitmap, EngineResult, FistState, Point } from './types';

type Action = Extract<Command, { type: 'minigameAction' }>;
const path = (name: string): string => `install/fistgame/${name}`;
const scoreValue = (value: number): number => value << 16 >> 16;
const lossShake = Array.from({ length: 8 }, (_, bounce) => {
  const values = [0]; let y = 12 - Math.floor(bounce * 12 / 8), velocity = y - 2;
  while (y > 0) { values.push(-y); y += velocity; velocity -= 2; }
  return values;
}).flat();

/** Original fist game frames are selected by the Rust render-loop state. */
export class FistView {
  #active = false;
  #generation = 0;
  #state: FistState | undefined;
  #bitmaps = new Map<string, DecodedBitmap>();
  #frames = new Map<string, DecodedBitmap[]>();
  #sprites = new Map<string, string>();
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #actions: Action[] = [];
  #acting = false;
  #phase = '';
  #left = false;
  #right = false;
  #font: ArrayBuffer | undefined;
  #scoreValue: number | undefined;
  #status: HTMLSpanElement | undefined;
  #bonus: BonusView;
  #soundEnabled = true;
  #sounds = new Map<string, HTMLAudioElement>();

  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#bonus = new BonusView(renderer, assets, engine);
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') {
        cancelAnimationFrame(this.#raf); this.#raf = 0; this.#release();
        for (const sound of this.#sounds.values()) sound.pause();
      } else { this.#schedule(); if (this.#soundEnabled) this.setSound(true); }
    });
    window.addEventListener('blur', () => this.#release());
    document.addEventListener('keydown', event => {
      if (!this.#active || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      if (['ready', 'help'].includes(this.#state?.phase ?? '') && [' ', 'Enter', 'Escape'].includes(event.key)) {
        event.preventDefault(); if (!event.repeat) this.#perform({ type: 'minigameAction', action: 'start' });
      } else if (this.#state?.phase === 'playing') {
        if (['ArrowLeft', 'ArrowRight', ' ', 'F1'].includes(event.key)) event.preventDefault();
        if (event.repeat) return;
        if (event.key === 'ArrowLeft') { this.#left = true; this.#input(); }
        else if (event.key === 'ArrowRight') { this.#right = true; this.#input(); }
        else if (event.key === ' ') this.#input(true);
        else if (event.key === 'F1') { this.#left = this.#right = false; this.#perform({ type: 'minigameAction', action: 'help' }); }
      }
    });
    document.addEventListener('keyup', event => {
      if (event.key === 'ArrowLeft') { this.#left = false; this.#input(); }
      if (event.key === 'ArrowRight') { this.#right = false; this.#input(); }
    });
  }

  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void {
    this.#soundEnabled = enabled; this.#bonus.setSound(enabled);
    if (!enabled) for (const sound of this.#sounds.values()) sound.pause();
    else if (this.#active && ['ready', 'help', 'playing', 'resultPause'].includes(this.#state?.phase ?? '')) void this.#sound('gbsf11', true);
  }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; this.#phase = ''; this.#actions = [];
    this.#left = this.#right = false; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    for (const sound of this.#sounds.values()) sound.pause();
    this.ui.classList.remove('fist-active'); this.#sprites.clear(); this.renderer.clearAnimatedSprites(); this.renderer.frameOffset({ x: 0, y: 0 }); this.#scoreValue = undefined;
    await this.#bonus.hide();
  }

  async show(state: FistState): Promise<void> {
    const entering = !this.#active;
    const previous = this.#state;
    this.#active = true; this.#state = state;
    if (entering) { this.#generation++; this.#lastTime = 0; }
    const generation = this.#generation;
    if (entering) {
      this.#font ??= await this.assets.bytes('install/font/ascfont.16');
      await Promise.all(['back1', 'zhong', 'gun0', 'ping', 'time', 'win', 'lost', 'fisthelp', ...Array.from({ length: 10 }, (_, index) => String(index))].map(async name => {
        if (!this.#bitmaps.has(name)) this.#bitmaps.set(name, await this.assets.bitmap(path(`${name}.bmp`), 1));
      }));
      for (const name of ['boss01', 'boss02', 'boss03', 'boss04', 'shut']) {
        if (this.#frames.has(name)) continue;
        const flic = await this.engine.openFlic(await this.assets.bytes(path(`${name}.flc`)), 1);
        try {
          const frames: DecodedBitmap[] = [flic];
          for (let frame = 1; frame < flic.frameCount; frame++) frames.push(await this.engine.nextFlic(flic.decoderId));
          this.#frames.set(name, frames);
        } finally { await this.engine.closeFlic(flic.decoderId); }
      }
      if (!this.#active || generation !== this.#generation) return;
      await this.renderer.setScene([
        { resourceId: path('fistbg.bmp'), x: 0, y: 0, width: 640, height: 480 },
        { resourceId: path('back.bmp'), x: 101, y: 51, width: 433, height: 348 },
      ], this.assets);
      if (['ready', 'help', 'playing', 'resultPause'].includes(state.phase)) void this.#sound('gbsf11', true);
    }
    if (!this.#active || generation !== this.#generation) return;
    if (this.#phase !== state.phase) { this.#phase = state.phase; this.#controls(state); }
    if (previous?.phase === 'resultPause' && state.phase !== 'resultPause') this.#sounds.get('gbsf11')?.pause();
    if (previous?.phase === 'playing' && state.phase === 'playing') {
      if (previous.strikeFrame === -1 && state.strikeFrame === 0) void this.#sound('gbsf02');
      if (previous.strikeFrame === 5 && state.strikeFrame === 6) { this.#sounds.get('gbsf02')?.pause(); void this.#sound('gbsf03'); }
      if (previous.strikeFrame === 6 && state.playerState === 0) {
        if (state.targetState === 6) { void this.#sound('gbsf04'); void this.#sound('gbsf05'); }
        else void this.#sound('gbsf06');
      }
    }
    if (state.phase !== 'ready') {
      const target = `boss0${Math.floor(state.targetState / 2) + 1}`;
      this.#animation('fist-target', target, Math.max(0, state.targetFrame), { x: state.targetX, y: 145 }, 1);
      this.renderer.animatedSpriteClip('fist-target', { x: 101, y: 51, width: 433, height: 348 });
      if (state.targetState === 6 && state.targetFrame === 0) this.#sprite('fist-hit', 'ping', { x: state.targetX + 50, y: 199 }, 2);
      else this.#remove('fist-hit');
      this.#sprite('fist-front', 'back1', { x: 101, y: 51 }, 3);
      if (state.playerState === 3) {
        this.#remove('fist-gun'); this.#animation('fist-punch', 'shut', Math.max(0, state.strikeFrame), { x: state.playerX, y: 219 }, 5);
      } else { this.#remove('fist-punch'); this.#sprite('fist-gun', 'gun0', { x: state.playerX, y: 309 }, 5); }
      this.#sprite('fist-time', 'time', { x: 290, y: 56 }, 6);
      this.#sprite('fist-tens', String(Math.floor(state.remainingSeconds / 10)), { x: 290, y: 83 }, 7);
      this.#sprite('fist-ones', String(state.remainingSeconds % 10), { x: 316, y: 83 }, 8);
    }
    this.#sprite('fist-goal', 'zhong', { x: 0, y: 172 }, 4);
    if (state.phase === 'ready' || state.phase === 'help') this.#sprite('fist-help', 'fisthelp', { x: 133, y: 46 }, 200);
    else this.#remove('fist-help');
    if (state.phase === 'won' && state.winFrame % 2 === 0) this.#sprite('fist-win', 'win', { x: 101, y: 197 }, 90);
    else this.#remove('fist-win');
    if (['lost', 'lostShake', 'lostWait'].includes(state.phase)) this.#sprite('fist-loss', 'lost', { x: 154, y: state.lossY }, 90);
    else this.#remove('fist-loss');
    const score = scoreValue(state.score);
    if (state.phase !== 'ready' && this.#scoreValue !== score) {
      const bitmap = asciiText(this.#font!, String(score));
      this.renderer.setAnimatedSprite('fist-score', bitmap, { x: 527 - bitmap.width, y: 437 }, 9); this.#scoreValue = score;
    }
    this.renderer.frameOffset({ x: 0, y: state.phase === 'lostShake' ? lossShake[state.lossShakeFrame] ?? -2 : state.phase === 'lostWait' ? -2 : 0 });
    if (this.#status) this.#status.textContent = state.phase === 'reward' && state.reward?.visible ? `道德 ${state.reward.value}，奖励 ${state.reward.increment}。` : `得分 ${score}，剩余 ${state.remainingSeconds} 秒。`;
    if (state.phase === 'reward' && state.reward) await this.#bonus.show('targ', state.reward);
    this.#schedule();
  }

  #sprite(id: string, name: string, point: Point, zIndex: number, source?: { x: number; y: number; width: number; height: number }): void {
    if (this.#sprites.get(id) !== name) { this.renderer.setAnimatedSprite(id, this.#bitmaps.get(name)!, point, zIndex); this.#sprites.set(id, name); }
    else this.renderer.animatedSpritePosition(id, point);
    if (source) this.renderer.animatedSpriteSource(id, source);
  }
  #animation(id: string, name: string, frame: number, point: Point, zIndex: number): void {
    const image = this.#frames.get(name)![frame]!;
    const key = `${name}:${frame}`;
    if (!this.#sprites.has(id)) this.renderer.setAnimatedSprite(id, image, point, zIndex);
    else { if (this.#sprites.get(id) !== key) this.renderer.animatedSpriteFrame(id, image); this.renderer.animatedSpritePosition(id, point); }
    this.#sprites.set(id, key);
  }
  #remove(id: string): void { this.#sprites.delete(id); this.renderer.removeAnimatedSprite(id); }

  #controls(state: FistState): void {
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('fist-active');
    const status = document.createElement('span'); status.className = 'sr-only fist-status'; this.#status = status; this.ui.append(status);
    const button = (label: string, className: string, click: () => void): HTMLButtonElement => {
      const element = document.createElement('button'); element.className = `game-hitbox ${className}`; element.setAttribute('aria-label', label); element.onclick = click; this.ui.append(element); return element;
    };
    if (state.phase === 'ready' || state.phase === 'help') {
      button(state.phase === 'ready' ? '开始拳击' : '继续拳击', 'fist-start', () => this.#perform({ type: 'minigameAction', action: 'start' }));
      this.inform('拳击：左右方向键移动，空格出拳；阅读原版说明后点击画面或按回车开始。');
      return;
    }
    if (state.phase !== 'playing') return;
    for (const [direction, label] of [['left', '向左移动'], ['right', '向右移动']] as const) {
      const element = button(label, `fist-move fist-${direction}`, () => {});
      const set = (value: boolean): void => { if (direction === 'left') this.#left = value; else this.#right = value; this.#input(); };
      element.onpointerdown = event => { event.preventDefault(); element.setPointerCapture(event.pointerId); set(true); };
      element.onpointerup = () => set(false); element.onpointercancel = () => set(false);
      element.onlostpointercapture = () => set(false);
    }
    button('出拳', 'fist-fire', () => this.#input(true));
    button('拳击说明', 'fist-help-button', () => this.#perform({ type: 'minigameAction', action: 'help' }));
    this.inform('拳击：左右方向键移动，空格出拳，F1 查看说明。触屏可按住场地左右侧移动、点击中央出拳。');
  }

  #input(fire = false): void { if (this.#active && this.#state?.phase === 'playing') this.#perform({ type: 'minigameAction', action: 'input', left: this.#left, right: this.#right, fire }); }
  #release(): void { if (this.#left || this.#right) { this.#left = this.#right = false; this.#input(); } }
  #perform(command: Action): void {
    if (!this.#active) return;
    this.#actions.push(command); if (this.#acting) return; this.#acting = true;
    cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step;
      while (this.#active && this.#actions.length) {
        const result = await this.send(this.#actions.shift()!);
        if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
      }
    })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now;
      const generation = this.#generation;
      this.#step = (async () => {
        await this.#bonus.tick(elapsedMs);
        if (!this.#active || generation !== this.#generation) return;
        const result = await this.send({ type: 'minigameTick', elapsedMs });
        if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
      })().catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`小游戏暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
  async #sound(name: string, loop = false): Promise<void> {
    if (!this.#soundEnabled || !this.#active || document.visibilityState === 'hidden') return;
    let sound = this.#sounds.get(name);
    if (!sound) {
      const bytes = await this.assets.bytes(path(`${name}.wav`));
      sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); sound.loop = loop; this.#sounds.set(name, sound);
    }
    if (!this.#active || !this.#soundEnabled) return;
    if (!loop) sound.currentTime = 0;
    void sound.play().catch(() => {});
  }
}
