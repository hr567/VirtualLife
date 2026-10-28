import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { BonusView } from './bonus';
import type { Command, DecodedBitmap, EngineResult, FireState, Point } from './types';

type Action = Extract<Command, { type: 'minigameAction' }>;
const path = (name: string): string => `install/fire/${name}`;
// Native 33-frame loss-banner destination table at 0x0046b518.
const lossY = [...Array<number>(20).fill(-100), -50, 0, 50, 120, 200, 200, 120, 80, 120, 200, 200, 120, 200];

/** Original fire game artwork and indexed-palette effects, driven entirely by Rust state. */
export class FireView {
  #active = false;
  #generation = 0;
  #state: FireState | undefined;
  #bitmaps = new Map<string, DecodedBitmap>();
  #frames = new Map<string, DecodedBitmap[]>();
  #sprites = new Map<string, { key: string; frame: number }>();
  #wanted = new Set<string>();
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #acting = false;
  #actions: Action[] = [];
  #phase = '';
  #scene = '';
  #gauge = -1;
  #status: HTMLElement | undefined;
  #bonus: BonusView;
  #soundEnabled = true;
  #sounds = new Map<string, HTMLAudioElement>();
  #soundLoads = new Map<string, Promise<HTMLAudioElement>>();
  #wantedSounds = new Set<string>();

  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#bonus = new BonusView(renderer, assets, engine);
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') { cancelAnimationFrame(this.#raf); this.#raf = 0; for (const sound of this.#sounds.values()) sound.pause(); }
      else { this.#schedule(); this.#syncSound(); }
    });
    document.addEventListener('keydown', event => {
      if (!this.#active || event.repeat || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      if (this.#state?.phase === 'ready' && [' ', 'Enter'].includes(event.key)) { event.preventDefault(); this.#perform('start'); }
      else if (this.#state?.phase === 'playing' && ['ArrowLeft', 'ArrowRight'].includes(event.key)) { event.preventDefault(); this.#perform(event.key === 'ArrowLeft' ? 'left' : 'right'); }
    });
    document.addEventListener('keyup', event => {
      if (!this.#active || this.#state?.phase !== 'playing') return;
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') this.#perform(event.key === 'ArrowLeft' ? 'releaseLeft' : 'releaseRight');
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#soundEnabled = enabled; this.#bonus.setSound(enabled); if (!enabled) for (const sound of this.#sounds.values()) sound.pause(); else this.#syncSound(); }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; this.#phase = this.#scene = ''; this.#actions = [];
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#gauge = -1;
    this.ui.classList.remove('fire-active'); this.#sprites.clear(); this.renderer.clearAnimatedSprites(); this.renderer.paletteEffect();
    this.ui.oncontextmenu = null;
    this.#wantedSounds.clear(); for (const sound of this.#sounds.values()) sound.pause();
    await this.#bonus.hide();
  }

  async show(state: FireState): Promise<void> {
    const entering = !this.#active, previous = this.#state; this.#active = true; this.#state = state;
    if (entering) { this.#generation++; this.#lastTime = 0; }
    const generation = this.#generation;
    if (entering) {
      await Promise.all(['yao3', 'yao5', 'yao6', 'yao8', 'yao9', 'yao10', 'yao13', 'zhuan', 'time', 'help', 'yao7'].map(async name => {
        if (!this.#bitmaps.has(name)) this.#bitmaps.set(name, await this.assets.bitmap(path(`${name}.bmp`), ['yao9', 'yao7'].includes(name) ? undefined : 1));
      }));
      for (const name of ['1', '2', 'yao01', 'yao02', 'yao13', 'yao14']) {
        if (this.#frames.has(name)) continue;
        const flic = await this.engine.openFlic(await this.assets.bytes(path(`${name}.flc`)), 1);
        try {
          const frames: DecodedBitmap[] = [flic];
          for (let frame = 1; frame < flic.frameCount; frame++) frames.push(await this.engine.nextFlic(flic.decoderId));
          this.#frames.set(name, frames);
        } finally { await this.engine.closeFlic(flic.decoderId); }
      }
    }
    if (!this.#active || generation !== this.#generation) return;
    this.ui.dataset.nativePhase = state.lossBanner ? 'lossBanner' : state.lossAnimation ? 'lossAnimation' : state.phase;
    const scene = state.lossAnimation || state.lossBanner ? 'loss' : 'game';
    if (this.#scene !== scene) {
      await this.renderer.setScene(scene === 'game' ? [
        { resourceId: path('yao7.bmp'), x: 0, y: 0, width: 640, height: 480 },
        { resourceId: path('yao4.bmp'), x: 104, y: 69, width: 433, height: 347 },
      ] : [], this.assets);
      this.#scene = scene; this.#sprites.clear(); this.#gauge = -1;
    }
    if (!this.#active || generation !== this.#generation) return;
    if (this.#phase !== state.phase) { this.#phase = state.phase; this.#controls(state); }
    this.#wantedSounds = new Set(state.winStage === 2 ? ['gbse07'] : []);
    this.#syncSound();
    if (!entering && previous) {
      if (state.hand !== previous.hand) void this.#sound('gbse01', false);
      if (state.shakeActive && !previous.shakeActive) void this.#sound('gbse04', false);
      if (state.lightningActive && !previous.lightningActive && state.lightningSound) void this.#sound(`gbse0${state.lightningSound}`, false);
      if (state.lossAnimation && !previous.lossAnimation) void this.#sound('gbse08', false);
    }
    this.#wanted.clear();
    this.renderer.paletteEffect();
    this.renderer.sceneSpritePosition(path('yao7.bmp'), { x: 0, y: state.shakeOffset });
    this.renderer.sceneSpritePosition(path('yao4.bmp'), { x: 104, y: 69 + state.shakeOffset });
    if (state.lossBanner) this.#bitmap('fire-loss', 'yao13', { x: 156, y: lossY[Math.min(32, state.lossFrame)]! }, 100);
    else if (state.lossAnimation) this.#animation('fire-loss', 'yao14', state.tracks[2]!.frame, { x: 214, y: 189 }, 100);
    else {
      const bodyFrame = state.bodyState <= 2 ? [0, 2, 1][state.bodyState]! : state.bodyState === 3 ? 3 + state.bodyFrame % 2 : state.bodyState === 4 ? 5 + state.bodyFrame % 2 : 7 + Math.min(3, state.bodyFrame);
      this.#bitmap('fire-body', 'yao3', { x: 254, y: 284 + state.shakeOffset }, 2, { x: 0, y: bodyFrame * 102, width: 73, height: 102 });
      if (state.phase === 'ready') this.#bitmap('fire-help', 'help', { x: 133, y: 66 }, 100);
      else {
        this.#bitmap('fire-goal', 'zhuan', { x: 0, y: 172 }, 1);
        if (state.winStage) this.#animation('fire-victory', state.winStage === 1 ? '1' : '2', state.tracks[state.winStage === 1 ? 0 : 1]!.frame, { x: 173, y: 186 }, 3);
        const spark = state.sparksState === 1 ? [4, 3][Math.min(1, state.sparksFrame)] : state.sparksState === 2 ? state.sparksFrame % 2 : state.sparksState === 3 ? 2 : undefined;
        if (spark !== undefined) this.#bitmap('fire-sparks', 'yao6', { x: 248, y: 346 }, 4, { x: spark * 34, y: 0, width: 34, height: 20 });
        this.#bitmap('fire-actor', 'yao5', { x: state.actorX, y: state.actorY }, 5, { x: state.actorType * 259, y: 0, width: 259, height: 160 });
        this.#bitmap('fire-meter', 'yao8', { x: 129, y: 269 }, 6);
        this.#drawGauge(state.gauge);
        if (!state.rewardStarted) {
          const remaining = this.#remaining(state);
          this.#bitmap('fire-time', 'time', { x: 298, y: 82 }, 8);
          this.#bitmap('fire-tens', 'yao10', { x: 305, y: 97 }, 8, { x: Math.floor(remaining / 10) * 16, y: 0, width: 16, height: 23 });
          this.#bitmap('fire-ones', 'yao10', { x: 321, y: 97 }, 8, { x: remaining % 10 * 16, y: 0, width: 16, height: 23 });
        }
        this.#bitmap('fire-bottom', 'yao7', { x: 0, y: 416 }, 9, { x: 0, y: 416, width: 640, height: 64 });
        if (state.winBanner && !state.rewardStarted) this.#animation('fire-win', 'yao13', state.tracks[3]!.frame, { x: 214, y: 189 }, 10);
        if (!state.winBanner) {
          if (state.intro) this.#animation('fire-intro', 'yao01', state.tracks[4]!.frame, { x: 429, y: 169 }, 11);
          if (state.randomOverlay) this.#animation('fire-random', 'yao02', state.tracks[5]!.frame, { x: 429, y: 169 }, 12);
        }
        if (!state.failed && state.lightningActive) {
          if (state.lightningFrame === 0 || state.lightningFrame === 2) this.renderer.paletteEffect(2, 2, 2, 63, 254);
        } else if (state.winStage && state.fireFlash) this.renderer.paletteEffect(2.5, 1.2, .5, 63, 254);
        if (state.actorType === 1) { const fade = Math.max(0, 500 - (state.clockMs - state.fadeAt)) / 500; this.renderer.paletteEffect(fade, fade, fade); }
        if (state.helpState) this.#bitmap('fire-help', 'help', { x: state.helpX, y: state.helpY }, 200);
        if (state.rewardStarted && state.reward) await this.#bonus.show('fire', state.reward);
      }
    }
    for (const id of this.#sprites.keys()) if (!this.#wanted.has(id)) { this.renderer.removeAnimatedSprite(id); this.#sprites.delete(id); }
    if (this.#status) this.#status.textContent = state.rewardStarted && state.reward?.visible ? `魅力 ${state.reward.value}，奖励 ${state.reward.increment}。` : `火力 ${state.gauge} / 1000，剩余 ${this.#remaining(state)} 秒。`;
    this.#schedule();
  }
  #remaining(state: FireState): number { return state.failed ? 0 : Math.floor(((state.duration - (state.winStage ? state.wonAtElapsed : state.elapsed)) >>> 0) / 1000) % 100; }
  #bitmap(id: string, key: string, position: Point, zIndex: number, source?: { x: number; y: number; width: number; height: number }): void {
    this.#wanted.add(id);
    if (this.#sprites.get(id)?.key !== key) this.renderer.setAnimatedSprite(id, this.#bitmaps.get(key)!, position, zIndex);
    else this.renderer.animatedSpritePosition(id, position);
    this.#sprites.set(id, { key, frame: 0 }); if (source) this.renderer.animatedSpriteSource(id, source);
  }
  #animation(id: string, key: string, frame: number, position: Point, zIndex: number): void {
    this.#wanted.add(id); const old = this.#sprites.get(id), image = this.#frames.get(key)![frame]!;
    if (old?.key !== key) this.renderer.setAnimatedSprite(id, image, position, zIndex);
    else { if (old.frame !== frame) this.renderer.animatedSpriteFrame(id, image); this.renderer.animatedSpritePosition(id, position); }
    this.#sprites.set(id, { key, frame });
  }
  #drawGauge(value: number): void {
    const lines = Math.trunc(67 * value / 1000); if (lines < 1) return;
    this.#wanted.add('fire-gauge');
    if (this.#gauge === lines && this.#sprites.has('fire-gauge')) return;
    const source = this.#bitmaps.get('yao9')!, pixels = new Uint8Array(4 * (lines + 1) * 4), indices = new Uint8Array(4 * (lines + 1));
    for (let y = 0; y <= lines; y++) { const row = y === 0 ? 0 : 1; pixels.set(source.pixels.subarray(row * 16, row * 16 + 16), y * 16); if (source.paletteIndices) indices.set(source.paletteIndices.subarray(row * 4, row * 4 + 4), y * 4); }
    this.renderer.setAnimatedSprite('fire-gauge', { width: 4, height: lines + 1, pixels, paletteIndices: indices }, { x: 140, y: 341 - lines }, 7);
    this.#sprites.set('fire-gauge', { key: 'gauge', frame: lines }); this.#gauge = lines;
  }
  #controls(state: FireState): void {
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('fire-active');
    const status = document.createElement('span'); status.className = 'sr-only fire-status'; this.#status = status; this.ui.append(status);
    const button = (label: string, className: string): HTMLButtonElement => { const element = document.createElement('button'); element.className = `game-hitbox ${className}`; element.setAttribute('aria-label', label); this.ui.append(element); return element; };
    if (state.phase === 'ready') { button('开始钻木取火', 'fire-start').onclick = () => this.#perform('start'); this.inform('钻木取火：阅读原版说明后，点击画面或按空格、回车开始。'); return; }
    for (const [side, label] of [['left', '左手生火'], ['right', '右手生火']] as const) {
      const element = button(label, `fire-hand fire-${side}`);
      element.onpointerdown = event => { event.preventDefault(); element.setPointerCapture(event.pointerId); this.#perform(side); };
      element.onpointerup = element.onpointercancel = () => this.#perform(side === 'left' ? 'releaseLeft' : 'releaseRight');
      element.onclick = event => { if (event.detail === 0) this.#perform(side); };
    }
    this.ui.oncontextmenu = event => { event.preventDefault(); this.#perform('right'); };
    this.inform('交替按左右方向键，或交替点击画面左右两侧生火。');
  }
  #perform(action: Action['action']): void {
    if (!this.#active) return; this.#actions.push({ type: 'minigameAction', action }); if (this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step;
      while (this.#active && this.#actions.length) { const result = await this.send(this.#actions.shift()!); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }
    })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = (async () => { await this.#bonus.tick(elapsedMs); if (!this.#active || generation !== this.#generation) return; const result = await this.send({ type: 'minigameTick', elapsedMs }); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); })().catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`小游戏暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
  #syncSound(): void {
    for (const [name, sound] of this.#sounds) if (sound.loop && !this.#wantedSounds.has(name)) sound.pause();
    if (this.#soundEnabled && this.#active && document.visibilityState === 'visible') for (const name of this.#wantedSounds) void this.#sound(name, true);
  }
  async #sound(name: string, loop: boolean): Promise<void> {
    if (!this.#soundEnabled || !this.#active || document.visibilityState === 'hidden') return;
    let loading = this.#soundLoads.get(name);
    if (!loading) {
      loading = this.assets.bytes(path(`sound/${name}.wav`)).then(bytes => {
        const sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); sound.loop = loop; this.#sounds.set(name, sound); return sound;
      });
      this.#soundLoads.set(name, loading);
    }
    const sound = await loading;
    if (!this.#active || !this.#soundEnabled || loop && !this.#wantedSounds.has(name)) return;
    if (!loop || sound.paused) { sound.currentTime = 0; void sound.play().catch(() => {}); }
  }
}
