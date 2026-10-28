import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { BonusView } from './bonus';
import type { Command, DecodedBitmap, EngineResult, HammerState, OpenedFlic, Point } from './types';

const resource = (name: string): string => `install/hammer/${name}.bmp`;
const lossY = [...Array<number>(20).fill(-100), -50, 0, 50, 120, 200, 200, 120, 80, 120, 200, 200, 120, 200, 200];
type Action = Extract<Command, { type: 'minigameAction' }>;
interface Animation { resourceId: string; flic: OpenedFlic; elapsed: number; frame: number; once: boolean }

/** Native hammer visuals. Rust owns all target timers, collisions, scores and random draws. */
export class HammerView {
  #active = false;
  #generation = 0;
  #state: HammerState | undefined;
  #bitmaps = new Map<string, DecodedBitmap>();
  #sprites = new Map<string, string>();
  #animations = new Map<string, Animation>();
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #userAction = false;
  #uiPhase = '';
  #score: HTMLSpanElement | undefined;
  #status: HTMLSpanElement | undefined;
  #pointer: Point = { x: 102, y: 59 };
  #pressed = false;
  #soundEnabled = true;
  #sounds = new Map<string, HTMLAudioElement>();
  #bonus: BonusView;

  constructor(
    private renderer: GameRenderer,
    private assets: Assets,
    private engine: EngineClient,
    private ui: HTMLElement,
    private send: (command: Command) => Promise<EngineResult>,
    private inform: (message: string) => void,
  ) {
    this.#bonus = new BonusView(renderer, assets, engine);
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') {
        cancelAnimationFrame(this.#raf); this.#raf = 0;
        for (const sound of this.#sounds.values()) sound.pause();
      } else this.#schedule();
    });
    document.addEventListener('keydown', event => {
      if (!this.#active || document.querySelector('dialog[open]') || event.repeat || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      if (this.#state?.phase === 'ready' && (event.key === ' ' || event.key === 'Enter')) {
        event.preventDefault(); this.#perform({ type: 'minigameAction', action: 'start' });
      } else if (this.#state?.phase === 'playing' && /^[1-9]$/.test(event.key)) {
        event.preventDefault(); this.#strikeIndex(Number(event.key) - 1);
      }
    });
    document.addEventListener('keyup', () => { this.#pressed = false; this.#drawPointer(); });
    document.addEventListener('pointerup', () => { this.#pressed = false; this.#drawPointer(); });
  }

  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void {
    this.#soundEnabled = enabled;
    this.#bonus.setSound(enabled);
    if (!enabled) for (const sound of this.#sounds.values()) sound.pause();
  }

  async hide(): Promise<void> {
    this.#active = false; this.#generation++;
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    this.#uiPhase = ''; this.#sprites.clear(); this.#pressed = false;
    this.ui.classList.remove('hammer-active');
    for (const sound of this.#sounds.values()) sound.pause();
    const animations = [...this.#animations.values()]; this.#animations.clear();
    await this.#bonus.hide();
    this.renderer.clearAnimatedSprites();
    await Promise.all(animations.map(animation => this.engine.closeFlic(animation.flic.decoderId)));
  }

  async show(state: HammerState): Promise<void> {
    const entering = !this.#active;
    const previous = this.#state;
    this.#state = state;
    if (entering) { this.#active = true; this.#generation++; this.#lastTime = 0; }
    const generation = this.#generation;
    if (entering) {
      this.#pointer = { x: state.pointer[0], y: state.pointer[1] };
      const names = ['mouse01', 'mouse02', 'mouse03', 'bow', 'score01', 'score02', 'score03', 'ham', 'goal', 'time', 'fig', 'lost', 'help'];
      await Promise.all(names.map(async name => this.#bitmaps.set(name, await this.assets.bitmap(resource(name), 1))));
      if (!this.#active || generation !== this.#generation) return;
      await this.renderer.setScene([
        { resourceId: resource('pattern'), x: 0, y: 0, width: 640, height: 480 },
        { resourceId: resource('bk'), x: 102, y: 59, width: 433, height: 348 },
      ], this.assets);
    }
    if (!this.#active || generation !== this.#generation) return;
    if (this.#uiPhase !== state.phase) {
      this.#uiPhase = state.phase; this.#controls(state);
      if (state.phase === 'won') await this.#openAnimation('hammer-win', 'install/hammer/win.flc', { x: 213, y: 179 }, 50, false);
      else await this.#closeAnimation('hammer-win');
    }
    if (!this.#active || generation !== this.#generation) return;
    if (state.phase === 'ready') {
      this.#sprite('hammer-help', 'help', { x: 133, y: 66 }, 100);
    } else {
      this.#remove('hammer-help');
      for (const [index, target] of state.targets.entries()) {
        const id = `hammer-target-${index}`;
        if (target.phase) this.#sprite(id, `mouse0${target.kind + 1}`, target, index * 2, { x: 0, y: target.spriteY, width: 124, height: 85 });
        else this.#remove(id);
        if (target.impactUntil !== null) this.#sprite(`hammer-impact-${index}`, 'bow', { x: target.impactPosition[0], y: target.impactPosition[1] }, index * 2 + 1);
        else this.#remove(`hammer-impact-${index}`);
        if (target.scoreFrame !== null) this.#sprite(`hammer-score-${index}`, `score0${target.kind + 1}`, { x: target.x + 35, y: target.y - 20 }, 30 + index, { x: 0, y: target.scoreFrame % 2 * 25, width: 43, height: 25 });
        else this.#remove(`hammer-score-${index}`);
        if (previous?.phase === 'playing' && target.hits > (previous.targets[index]?.hits ?? 0)) {
          const wonPoints = target.scoreFrame === 0;
          void this.#sound(wonPoints ? target.kind === 2 ? 'gbsd06' : 'gbsd02' : 'gbsd03');
        }
      }
      this.#drawPointer();
      this.#sprite('hammer-goal', 'goal', { x: 0, y: 172 }, 20);
      if (state.phase !== 'reward') {
        this.#sprite('hammer-time', 'time', { x: 293, y: 72 }, 40);
        this.#sprite('hammer-tens', 'fig', { x: 300, y: 87 }, 41, { x: Math.floor(state.remainingSeconds / 10) * 16, y: 0, width: 16, height: 23 });
        this.#sprite('hammer-ones', 'fig', { x: 316, y: 87 }, 42, { x: state.remainingSeconds % 10 * 16, y: 0, width: 16, height: 23 });
      } else for (const id of ['hammer-time', 'hammer-tens', 'hammer-ones']) this.#remove(id);
      if (state.phase === 'lost') this.#sprite('hammer-lost', 'lost', { x: 156, y: lossY[state.lossFrame] ?? 200 }, 100);
      else this.#remove('hammer-lost');
    }
    if (this.#score) this.#score.textContent = String(state.score);
    if (this.#status) this.#status.textContent = state.phase === 'reward' && state.reward?.visible
      ? `道德 ${state.reward.value}，奖励 ${state.reward.increment}。`
      : `得分 ${state.score}，剩余 ${state.remainingSeconds} 秒。`;
    if (state.phase === 'reward' && state.reward) await this.#bonus.show('ham', state.reward);
    this.#schedule();
  }

  #sprite(id: string, key: string, position: Point, zIndex: number, source?: { x: number; y: number; width: number; height: number }): void {
    const bitmap = this.#bitmaps.get(key);
    if (!bitmap) throw new Error(`打地鼠素材尚未载入：${key}`);
    if (this.#sprites.get(id) !== key) {
      this.renderer.setAnimatedSprite(id, bitmap, position, zIndex); this.#sprites.set(id, key);
    } else this.renderer.animatedSpritePosition(id, position);
    if (source) this.renderer.animatedSpriteSource(id, source);
  }

  #remove(id: string): void { this.renderer.removeAnimatedSprite(id); this.#sprites.delete(id); }

  #drawPointer(): void {
    if (!this.#active || this.#state?.phase === 'ready') return;
    this.#sprite('hammer-pointer', 'ham', this.#pointer, 19, { x: 0, y: this.#pressed ? 68 : 0, width: 68, height: 68 });
  }

  #controls(state: HammerState): void {
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('hammer-active');
    this.#score = undefined; this.#status = undefined;
    if (state.phase === 'ready') {
      const start = document.createElement('button'); start.className = 'game-hitbox hammer-start'; start.setAttribute('aria-label', '开始打地鼠');
      start.onclick = () => this.#perform({ type: 'minigameAction', action: 'start' });
      this.ui.append(start);
      this.inform('打地鼠：阅读原版说明后，点击画面、按空格或回车开始。');
      return;
    }
    const score = document.createElement('span'); score.className = 'hammer-score'; this.#score = score;
    const status = document.createElement('span'); status.className = 'sr-only hammer-status'; this.#status = status;
    this.ui.append(score, status);
    if (state.phase === 'playing') {
      const field = document.createElement('div'); field.className = 'hammer-playfield';
      field.onpointermove = event => this.#move(event);
      field.onpointerdown = event => {
        event.preventDefault(); this.#move(event); this.#pressed = true; this.#drawPointer();
        const index = (event.target as HTMLElement).dataset.target;
        if (event.pointerType !== 'mouse' && index !== undefined) this.#strikeIndex(Number(index));
        else this.#perform({ type: 'minigameAction', action: 'strike', x: this.#pointer.x, y: this.#pointer.y });
      };
      for (const [index, target] of state.targets.entries()) {
        const control = document.createElement('button'); control.className = 'game-hitbox hammer-target'; control.dataset.target = String(index);
        control.setAttribute('aria-label', `击打第 ${index + 1} 个洞`);
        Object.assign(control.style, { left: `${target.x + 25}px`, top: `${target.y + 12}px`, width: '70px', height: '55px' });
        control.onclick = event => { if (event.detail === 0) this.#strikeIndex(index); };
        field.append(control);
      }
      this.ui.append(field);
      this.inform('打地鼠：移动鼠标后点击，或按数字键 1–9；触屏可直接点按洞口。');
    } else this.inform(state.phase === 'won' || state.phase === 'reward' ? `打地鼠成功，得分 ${state.score}。` : `时间到，得分 ${state.score}。`);
  }

  #move(event: PointerEvent): void {
    const rect = this.renderer.canvas.getBoundingClientRect();
    this.#pointer = { x: Math.max(102, Math.min(467, Math.floor((event.clientX - rect.left) * 640 / rect.width))), y: Math.max(59, Math.min(339, Math.floor((event.clientY - rect.top) * 480 / rect.height))) };
    this.#drawPointer();
  }

  #strikeIndex(index: number): void {
    const target = this.#state?.targets[index];
    if (!target) return;
    this.#pointer = { x: target.x + 38, y: target.y - 28 }; this.#pressed = true; this.#drawPointer();
    this.#perform({ type: 'minigameAction', action: 'strikeIndex', index });
  }

  #perform(command: Action): void {
    if (!this.#active || this.#userAction) return;
    this.#userAction = true;
    cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step;
      if (!this.#active) return;
      const result = await this.send(command);
      if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
    })().catch(error => this.inform(String(error))).finally(() => {
      this.#userAction = false; this.#schedule();
    });
  }

  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#userAction || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16;
      this.#lastTime = now;
      const generation = this.#generation;
      this.#step = this.#frame(elapsedMs, generation).catch(async error => {
        if (this.#active && generation === this.#generation) { this.inform(`小游戏暂停：${String(error)}`); await this.hide(); }
      }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }

  async #frame(elapsedMs: number, generation: number): Promise<void> {
    await this.#bonus.tick(elapsedMs);
    for (const [id, animation] of this.#animations) {
      animation.elapsed += elapsedMs;
      if (animation.elapsed <= 100 || animation.once && animation.frame === animation.flic.frameCount - 1) continue;
      animation.elapsed = 0;
      const frame = await this.engine.nextFlic(animation.flic.decoderId);
      if (!this.#active || generation !== this.#generation) return;
      animation.frame = (animation.frame + 1) % animation.flic.frameCount;
      this.renderer.animatedSpriteFrame(id, frame);
    }
    if (!this.#active || generation !== this.#generation) return;
    const result = await this.send({ type: 'minigameTick', elapsedMs });
    if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
  }

  async #openAnimation(id: string, resourceId: string, position: Point, zIndex: number, once: boolean): Promise<void> {
    const generation = this.#generation;
    const flic = await this.engine.openFlic(await this.assets.bytes(resourceId), 1);
    if (!this.#active || generation !== this.#generation) { await this.engine.closeFlic(flic.decoderId); return; }
    this.#animations.set(id, { resourceId, flic, elapsed: 0, frame: 0, once });
    this.renderer.setAnimatedSprite(id, flic, position, zIndex);
  }

  async #closeAnimation(id: string): Promise<void> {
    const animation = this.#animations.get(id);
    if (!animation) return;
    this.#animations.delete(id); this.renderer.removeAnimatedSprite(id);
    await this.engine.closeFlic(animation.flic.decoderId);
  }

  async #sound(name: string): Promise<void> {
    if (!this.#soundEnabled || !this.#active || document.visibilityState === 'hidden') return;
    let sound = this.#sounds.get(name);
    if (!sound) {
      const bytes = await this.assets.bytes(`install/hammer/sound/${name}.wav`);
      sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })));
      this.#sounds.set(name, sound);
    }
    if (!this.#active || !this.#soundEnabled) return;
    sound.currentTime = 0; void sound.play().catch(() => {});
  }
}
