import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { BonusView } from './bonus';
import type { Command, DecodedBitmap, EngineResult, GasState, NativeAnimation, Point } from './types';

type Action = Extract<Command, { type: 'minigameAction' }>;
const resource = (name: string): string => `install/gas/${name}.bmp`;
const shake = [15, -15, 10, -10, 7, -7, 5, -5, 3, -3, 2, -2, 1, -1, 0, 0];
const field = { x: 101, y: 51, width: 433, height: 348 };

/** Native timing-game composition; Rust supplies every animated frame and outcome. */
export class GasView {
  #active = false;
  #generation = 0;
  #state: GasState | undefined;
  #bitmaps = new Map<string, DecodedBitmap>();
  #frames = new Map<string, DecodedBitmap[]>();
  #sprites = new Map<string, { resource: string; frame: number }>();
  #wanted = new Set<string>();
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #acting = false;
  #phase = '';
  #scene = '';
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
      const phase = this.#state?.phase;
      if ((phase === 'ready' || phase === 'help') && [' ', 'Enter', 'Escape'].includes(event.key)) {
        event.preventDefault(); this.#perform({ type: 'minigameAction', action: 'start' });
      } else if (phase === 'playing') {
        if ([' ', 'Enter', 'F1'].includes(event.key)) event.preventDefault();
        if (event.key === 'F1') this.#perform({ type: 'minigameAction', action: 'help' });
        else if (event.key === ' ' || event.key === 'Enter') this.#perform({ type: 'minigameAction', action: 'throw' });
      }
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void {
    this.#soundEnabled = enabled; this.#bonus.setSound(enabled);
    if (!enabled) for (const sound of this.#sounds.values()) sound.pause(); else this.#syncSound();
  }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; this.#phase = this.#scene = '';
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    this.ui.classList.remove('gas-active'); this.#sprites.clear(); this.renderer.clearAnimatedSprites(); this.renderer.frameOffset({ x: 0, y: 0 });
    this.#wantedSounds.clear(); for (const sound of this.#sounds.values()) sound.pause();
    await this.#bonus.hide();
  }

  async show(state: GasState): Promise<void> {
    const entering = !this.#active;
    const previous = this.#state; this.#state = state; this.#active = true;
    if (entering) { this.#generation++; this.#lastTime = 0; }
    const generation = this.#generation;
    if (entering) {
      await Promise.all(['gas02', 'gas03', 'gas04', 'gou', 'fish1', 'x', 'bg01_1'].map(async name => {
        if (!this.#bitmaps.has(name)) this.#bitmaps.set(name, await this.assets.bitmap(resource(name), 1));
      }));
      for (const animation of [...state.targets, ...state.animations]) {
        if (this.#frames.has(animation.resource)) continue;
        const flic = await this.engine.openFlic(await this.assets.bytes(animation.resource), 1);
        try {
          const frames: DecodedBitmap[] = [flic];
          for (let index = 1; index < flic.frameCount; index++) frames.push(await this.engine.nextFlic(flic.decoderId));
          this.#frames.set(animation.resource, frames);
        } finally { await this.engine.closeFlic(flic.decoderId); }
      }
    }
    if (!this.#active || generation !== this.#generation) return;
    const scene = state.phase === 'reward' ? 'reward' : 'game';
    if (scene !== this.#scene) {
      await this.renderer.setScene([
        { resourceId: resource('bg01'), x: 0, y: 0, width: 640, height: 480 },
        ...(scene === 'game' ? [{ resourceId: resource('gas01'), ...field }] : []),
      ], this.assets);
      this.#scene = scene; this.#sprites.clear();
    }
    if (!this.#active || generation !== this.#generation) return;
    if (this.#phase !== state.phase) { this.#phase = state.phase; this.#controls(state); }
    this.#wantedSounds = new Set(['gbsb07']);
    if (state.phase === 'ready') this.#wantedSounds.add('gbsa03');
    if (state.phase === 'playing') {
      this.#wantedSounds.add(state.targetState === 3 ? 'gbsb04' : 'gbsb11');
      if (state.playerState === 2) this.#wantedSounds.add('gbsb09');
    }
    if (state.phase === 'hit' && state.animations[4]!.frame > 9) this.#wantedSounds.add('gbsb02');
    if (state.phase === 'miss' && state.animations[5]!.frame > 4) this.#wantedSounds.add('gbsb10');
    if (state.phase === 'won' || state.phase === 'wonWait') {
      if (previous?.phase !== 'won' && previous?.phase !== 'wonWait') void this.#sound('gbsb06', false);
    } else this.#sounds.get('gbsb06')?.pause();
    if (state.phase === 'lost' && state.animations[7]!.frame === 14 && previous?.animations[7]?.frame !== 14) void this.#sound('gbsb05', false);
    if (state.phase === 'lostFall') this.#sounds.get('gbsb05')?.pause();
    this.#syncSound();
    this.#wanted.clear();
    if (state.phase === 'reward' && state.reward) {
      this.renderer.frameOffset({ x: 0, y: 0 }); await this.#bonus.show('bear', state.reward);
    } else if (state.phase !== 'lostShake') {
      this.renderer.frameOffset({ x: 0, y: 0 });
      const a = state.animations;
      if (state.phase === 'ready' || state.phase === 'help') this.#bitmap('gas-help', 'bg01_1', { x: 131, y: 51 }, 50);
      else {
        if (state.phase === 'playing') {
          if (state.playerState === 0) this.#animation('gas-player', a[0]!, 1);
          else if (state.playerState === 1) this.#animation('gas-player', a[1]!, 1);
          else { this.#animation('gas-player', a[2]!, 1); this.#animation('gas-smoke', a[3]!, 2); }
        } else if (state.phase === 'hit') this.#animation('gas-player', a[4]!, 1);
        else if (state.phase === 'miss') this.#animation('gas-player', a[5]!, 1);
        else if (state.phase === 'won' || state.phase === 'wonWait') {
          this.#animation('gas-player', a[0]!, 1);
          if (state.phase === 'won' && state.clockMs - state.phaseLastTick < 3000) this.#animation('gas-victory', a[6]!, 3);
          else this.#bitmap('gas-victory', 'gas03', { x: 192, y: 137 }, 3);
        } else if (state.phase === 'lost') this.#animation('gas-player', a[7]!, 1);
        else if (state.phase === 'lostFall') this.#animation('gas-player', a[8]!, 1);
        if (['playing', 'miss', 'lost', 'lostFall'].includes(state.phase) || state.phase === 'hit' && a[4]!.frame < 6) {
          this.#animation('gas-target', state.targets[state.targetState]!, 4); this.renderer.animatedSpriteClip('gas-target', field);
        }
        this.#bitmap('gas-front', 'gas02', { x: 349, y: 303 }, 5);
        if (state.phase === 'lostFall') { this.#bitmap('gas-fall', 'gas04', { x: 149, y: Math.trunc(state.fallY) }, 6); this.renderer.animatedSpriteClip('gas-fall', field); }
      }
      this.#bitmap('gas-goal', 'gou', { x: 0, y: 169 }, 60);
      for (let index = 0; index < 3; index++) {
        if (state.results[index]) this.#animation(`gas-fish-${index}`, a[9 + index]!, 61);
        else {
          this.#bitmap(`gas-fish-${index}`, 'fish1', { x: 213 + index * 76, y: 417 }, 61);
          if (index < state.attempts) this.#bitmap(`gas-failed-${index}`, 'x', { x: 219 + index * 76, y: 422 }, 62);
        }
      }
    } else {
      for (const id of this.#sprites.keys()) this.#wanted.add(id);
      this.renderer.frameOffset({ x: 0, y: 15 + (shake[state.shakeFrame] ?? 0) });
    }
    for (const id of this.#sprites.keys()) if (!this.#wanted.has(id)) { this.renderer.removeAnimatedSprite(id); this.#sprites.delete(id); }
    if (this.#status) this.#status.textContent = state.phase === 'reward' && state.reward?.visible ? `体能 ${state.reward.value}，奖励 ${state.reward.increment}。` : `已尝试 ${state.attempts} / 3 次，成功 ${state.successes} 次。`;
    this.#schedule();
  }

  #bitmap(id: string, name: string, position: Point, zIndex: number): void {
    this.#wanted.add(id);
    if (this.#sprites.get(id)?.resource !== name) { this.renderer.setAnimatedSprite(id, this.#bitmaps.get(name)!, position, zIndex); this.#sprites.set(id, { resource: name, frame: 0 }); }
    else this.renderer.animatedSpritePosition(id, position);
  }
  #animation(id: string, animation: NativeAnimation, zIndex: number): void {
    this.#wanted.add(id);
    const previous = this.#sprites.get(id), image = this.#frames.get(animation.resource)![animation.frame]!;
    if (previous?.resource !== animation.resource) this.renderer.setAnimatedSprite(id, image, animation, zIndex);
    else { if (previous.frame !== animation.frame) this.renderer.animatedSpriteFrame(id, image); this.renderer.animatedSpritePosition(id, animation); }
    this.#sprites.set(id, { resource: animation.resource, frame: animation.frame });
  }
  #controls(state: GasState): void {
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('gas-active');
    const status = document.createElement('span'); status.className = 'sr-only gas-status'; this.#status = status; this.ui.append(status);
    if (!['ready', 'help', 'playing'].includes(state.phase)) return;
    const button = document.createElement('button'); button.className = 'game-hitbox gas-action';
    const ready = state.phase === 'ready' || state.phase === 'help';
    button.setAttribute('aria-label', ready ? state.phase === 'ready' ? '开始狗熊捕鱼' : '继续狗熊捕鱼' : '捕鱼');
    button.onclick = () => this.#perform({ type: 'minigameAction', action: ready ? 'start' : 'throw' }); this.ui.append(button);
    this.inform(ready ? '狗熊捕鱼：阅读原版说明后点击画面或按回车开始。' : '狗熊捕鱼：看准时机点击画面或按空格、回车，F1 查看说明。');
  }
  #perform(command: Action): void {
    if (!this.#active || this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step; if (!this.#active) return;
      const result = await this.send(command);
      if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
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
  #syncSound(): void {
    for (const [name, sound] of this.#sounds) if (sound.loop && !this.#wantedSounds.has(name)) sound.pause();
    if (this.#soundEnabled && this.#active && document.visibilityState === 'visible') for (const name of this.#wantedSounds) void this.#sound(name, true);
  }
  async #sound(name: string, loop: boolean): Promise<void> {
    if (!this.#soundEnabled || !this.#active || document.visibilityState === 'hidden') return;
    let loading = this.#soundLoads.get(name);
    if (!loading) {
      loading = this.assets.bytes(`install/sound/gb/${name}.wav`).then(bytes => {
        const sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); sound.loop = loop; this.#sounds.set(name, sound); return sound;
      });
      this.#soundLoads.set(name, loading);
    }
    const sound = await loading;
    if (!this.#active || !this.#soundEnabled || loop && !this.#wantedSounds.has(name)) return;
    if (sound.paused) { sound.currentTime = 0; void sound.play().catch(() => {}); }
  }
}
