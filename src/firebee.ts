import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { BonusView } from './bonus';
import { asciiText } from './bitmap-font';
import type { Command, DecodedBitmap, EngineResult, FirebeeState, Point } from './types';

type Action = Extract<Command, { type: 'minigameAction' }>;
const path = (name: string): string => `install/firebee/${name}`;
const field = { x: 103, y: 66, width: 433, height: 348 };

/** The native render lists preserve the original projectile-removal frame order. */
export class FirebeeView {
  #active = false; #generation = 0; #state: FirebeeState | undefined;
  #bitmaps = new Map<string, DecodedBitmap>(); #frames = new Map<string, DecodedBitmap[]>();
  #sprites = new Map<string, { key: string; frame: number }>(); #wanted = new Set<string>();
  #raf = 0; #lastTime = 0; #step: Promise<void> | undefined; #acting = false; #actions: Action[] = [];
  #phase = ''; #scene = ''; #font: ArrayBuffer | undefined; #score = NaN;
  #left = false; #right = false; #fireEdge = false;
  #status: HTMLElement | undefined; #bonus: BonusView;
  #soundEnabled = true; #sounds = new Map<string, HTMLAudioElement>(); #soundLoads = new Map<string, Promise<HTMLAudioElement>>();

  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#bonus = new BonusView(renderer, assets, engine);
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') { this.#release(); cancelAnimationFrame(this.#raf); this.#raf = 0; for (const sound of this.#sounds.values()) sound.pause(); }
      else { this.#schedule(); this.#syncSound(); }
    });
    window.addEventListener('blur', () => this.#release());
    document.addEventListener('keydown', event => {
      if (!this.#active || event.repeat || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      if (['ready', 'help'].includes(this.#state?.phase ?? '') && [' ', 'Enter', 'Escape'].includes(event.key)) { event.preventDefault(); this.#perform({ type: 'minigameAction', action: 'start' }); }
      else if (this.#state?.phase === 'playing') {
        if (['ArrowLeft', 'ArrowRight', ' ', 'F1'].includes(event.key)) event.preventDefault();
        if (event.key === 'ArrowLeft') { this.#left = true; this.#input(); }
        if (event.key === 'ArrowRight') { this.#right = true; this.#input(); }
        if (event.key === ' ') this.#input(true);
        if (event.key === 'F1') this.#perform({ type: 'minigameAction', action: 'help' });
      }
    });
    document.addEventListener('keyup', event => {
      if (!this.#active) return;
      if (event.key === 'ArrowLeft') { this.#left = false; this.#input(); }
      if (event.key === 'ArrowRight') { this.#right = false; this.#input(); }
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#soundEnabled = enabled; this.#bonus.setSound(enabled); if (!enabled) for (const sound of this.#sounds.values()) sound.pause(); else this.#syncSound(); }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; this.#phase = this.#scene = ''; this.#actions = []; this.#left = this.#right = this.#fireEdge = false;
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#score = NaN;
    this.ui.classList.remove('firebee-active'); this.#sprites.clear(); this.renderer.clearAnimatedSprites();
    for (const sound of this.#sounds.values()) sound.pause(); await this.#bonus.hide();
  }

  async show(state: FirebeeState): Promise<void> {
    const entering = !this.#active, previous = this.#state; this.#active = true; this.#state = state;
    if (entering) { this.#generation++; this.#lastTime = 0; this.#left = state.left; this.#right = state.right; this.#fireEdge = state.fire; }
    const generation = this.#generation;
    if (entering) {
      this.#font ??= await this.assets.bytes('install/font/ascfont.16');
      await Promise.all(['plane', 'plane1', 'plane2', 'shut01', 'shut02', 'bg06_1'].map(async name => {
        if (!this.#bitmaps.has(name)) this.#bitmaps.set(name, await this.assets.bitmap(path(`${name}.bmp`), 1));
      }));
      for (const name of [...Array.from({ length: 15 }, (_, index) => `mos${String(index + 1).padStart(2, '0')}a`), 'xma01', 'bomb', 'jb']) {
        if (this.#frames.has(name)) continue;
        const flic = await this.engine.openFlic(await this.assets.bytes(path(`${name}.flc`)), 1);
        try { const frames: DecodedBitmap[] = [flic]; for (let frame = 1; frame < flic.frameCount; frame++) frames.push(await this.engine.nextFlic(flic.decoderId)); this.#frames.set(name, frames); }
        finally { await this.engine.closeFlic(flic.decoderId); }
      }
    }
    if (!this.#active || generation !== this.#generation) return;
    const scene = state.phase === 'reward' ? 'reward' : 'game';
    if (scene !== this.#scene) {
      await this.renderer.setScene([{ resourceId: path('bg06.bmp'), x: 0, y: 0, width: 640, height: 480 }, ...(scene === 'game' ? [{ resourceId: path('back.bmp'), ...field, transparentPaletteIndex: 1 }] : [])], this.assets);
      this.#scene = scene; this.#sprites.clear(); this.#score = NaN;
    }
    if (!this.#active || generation !== this.#generation) return;
    if (state.phase !== this.#phase) { this.#phase = state.phase; this.#controls(state); }
    this.#syncSound();
    if (!entering && previous?.clockMs !== state.clockMs) for (const sound of state.sounds) void this.#sound(`gbsg0${sound}`, false);
    this.#wanted.clear();
    if (scene === 'game') {
      state.drawnEnemies.forEach((enemy, index) => {
        const id = `firebee-enemy-${index}`; this.#animation(id, enemy.row === 15 ? 'xma01' : `mos${String(enemy.row + 1).padStart(2, '0')}a`, enemy.frame, enemy, 1);
        this.renderer.animatedSpriteClip(id, field);
      });
      state.drawnEnemyShots.forEach(([x, y], index) => this.#bitmap(`firebee-enemy-shot-${index}`, 'shut01', { x, y }, 2));
      state.drawnPlayerShots.forEach(([x, y], index) => this.#bitmap(`firebee-player-shot-${index}`, 'shut02', { x, y }, 3));
      state.drawnExplosions.forEach((explosion, index) => this.#animation(`firebee-explosion-${index}`, 'bomb', explosion.frame, explosion, 4));
      this.#bitmap('firebee-player', ['plane', 'plane1', 'plane2'][state.playerState]!, { x: state.playerX, y: 350 }, 5);
      if (state.failed) this.#animation('firebee-death', 'jb', state.death.frame, state.death, 6);
      this.#wanted.add('firebee-score');
      if (this.#score !== state.score) { this.renderer.setAnimatedSprite('firebee-score', asciiText(this.#font!, String(state.score)), { x: 480, y: 432 }, 7); this.#sprites.set('firebee-score', { key: 'score', frame: state.score }); this.#score = state.score; }
      if (state.phase === 'ready' || state.phase === 'help') this.#bitmap('firebee-help', 'bg06_1', { x: 131, y: 51 }, 8);
    } else if (state.reward) await this.#bonus.show('fly', state.reward);
    for (const id of this.#sprites.keys()) if (!this.#wanted.has(id)) { this.renderer.removeAnimatedSprite(id); this.#sprites.delete(id); }
    if (this.#status) this.#status.textContent = state.phase === 'reward' && state.reward?.visible ? `金钱 ${state.reward.value}，奖励 ${state.reward.increment}。` : `得分 ${state.score}。`;
    this.#schedule();
  }
  #bitmap(id: string, key: string, position: Point, zIndex: number): void {
    this.#wanted.add(id);
    if (this.#sprites.get(id)?.key !== key) this.renderer.setAnimatedSprite(id, this.#bitmaps.get(key)!, position, zIndex);
    else this.renderer.animatedSpritePosition(id, position);
    this.#sprites.set(id, { key, frame: 0 });
  }
  #animation(id: string, key: string, frame: number, position: Point, zIndex: number): void {
    this.#wanted.add(id); const previous = this.#sprites.get(id), image = this.#frames.get(key)![frame]!;
    if (previous?.key !== key) this.renderer.setAnimatedSprite(id, image, position, zIndex);
    else { if (previous.frame !== frame) this.renderer.animatedSpriteFrame(id, image); this.renderer.animatedSpritePosition(id, position); }
    this.#sprites.set(id, { key, frame });
  }
  #controls(state: FirebeeState): void {
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('firebee-active');
    const status = document.createElement('span'); status.className = 'sr-only firebee-status'; this.#status = status; this.ui.append(status);
    const button = (label: string, className: string, click: () => void): HTMLButtonElement => { const element = document.createElement('button'); element.className = `game-hitbox ${className}`; element.setAttribute('aria-label', label); element.onclick = click; this.ui.append(element); return element; };
    if (state.phase === 'ready' || state.phase === 'help') { button(state.phase === 'ready' ? '开始射击' : '继续射击', 'firebee-start', () => this.#perform({ type: 'minigameAction', action: 'start' })); this.inform('阅读原版说明后，点击画面或按回车开始。'); return; }
    if (state.phase !== 'playing') return;
    for (const [direction, label] of [['left', '向左飞行'], ['right', '向右飞行']] as const) {
      const element = button(label, `firebee-move firebee-${direction}`, () => {});
      const set = (value: boolean): void => { if (direction === 'left') this.#left = value; else this.#right = value; this.#input(); };
      element.onpointerdown = event => { event.preventDefault(); element.setPointerCapture(event.pointerId); set(true); };
      element.onpointerup = element.onpointercancel = element.onlostpointercapture = () => set(false);
    }
    button('发射', 'firebee-fire', () => this.#input(true)); button('射击说明', 'firebee-help-button', () => this.#perform({ type: 'minigameAction', action: 'help' }));
    this.inform('左右方向键移动，空格发射，F1 查看说明。触屏可按住场地两侧移动、点击中央发射。');
  }
  #input(fire = false): void { if (this.#active && this.#state?.phase === 'playing') { this.#fireEdge ||= fire; this.#perform({ type: 'minigameAction', action: 'input', left: this.#left, right: this.#right, fire: this.#fireEdge }); } }
  #release(): void { if (this.#left || this.#right) { this.#left = this.#right = false; this.#input(); } }
  #perform(command: Action): void {
    if (!this.#active) return; this.#actions.push(command); if (this.#acting) return; this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => { await this.#step; while (this.#active && this.#actions.length) { const result = await this.send(this.#actions.shift()!); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); } })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = (async () => { await this.#bonus.tick(elapsedMs); if (!this.#active || generation !== this.#generation) return; this.#fireEdge = false; const result = await this.send({ type: 'minigameTick', elapsedMs }); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); })().catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`小游戏暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
  #syncSound(): void {
    const loops = this.#active && this.#state?.phase !== 'reward' ? ['gbsg01', 'gbsg09'] : [];
    for (const [name, sound] of this.#sounds) if (sound.loop && !loops.includes(name)) sound.pause();
    if (this.#soundEnabled && document.visibilityState === 'visible') for (const name of loops) void this.#sound(name, true);
  }
  async #sound(name: string, loop: boolean): Promise<void> {
    if (!this.#soundEnabled || !this.#active || document.visibilityState === 'hidden') return;
    let loading = this.#soundLoads.get(name);
    if (!loading) { loading = this.assets.bytes(`install/sound/gb/${name}.wav`).then(bytes => { const sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); sound.loop = loop; this.#sounds.set(name, sound); return sound; }); this.#soundLoads.set(name, loading); }
    const sound = await loading; if (!this.#active || !this.#soundEnabled || loop && this.#state?.phase === 'reward') return;
    if (!loop || sound.paused) { sound.currentTime = 0; void sound.play().catch(() => {}); }
  }
}
