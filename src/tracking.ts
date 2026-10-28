import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer, SceneImage } from './renderer';
import type { Command, EngineResult, OpenedFlic, Point, TrackingState } from './types';
import { BonusView } from './bonus';

const sprites = ['gbu01', 'gbu02', 'gbu09', 'gbu03', 'gbu10', 'gbu07', 'gbu08', 'gbu04', 'gbu05', 'gbu06'];
const resource = (name: string): string => `install/gbs/${name}.flc`;
type Action = Extract<Command, { type: 'minigameAction' }>;
interface Animation {
  resource: string;
  flic: OpenedFlic;
  frame: number;
  elapsedMs: number;
  finished: boolean;
  once: boolean;
}

/** The native five-object game: Rust owns motion and answers; this view plays the disc art. */
export class TrackingView {
  #active = false;
  #generation = 0;
  #state: TrackingState | undefined;
  #animations = new Map<number, Animation>();
  #bytes = new Map<string, Promise<ArrayBuffer>>();
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #userAction = false;
  #idleFinished = false;
  #transformed = false;
  #uiKey = '';
  #sound = gameAudio();
  #soundEnabled = true;
  #soundKey = '';
  #soundUrls = new Map<string, string>();
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
        cancelAnimationFrame(this.#raf); this.#raf = 0; this.#sound.pause();
      } else if (this.#active) {
        this.#schedule();
        if (this.#soundEnabled) void this.#sound.play().catch(() => {});
      }
    });
  }

  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void {
    this.#soundEnabled = enabled;
    this.#bonus.setSound(enabled);
    if (enabled && this.#active) void this.#sound.play().catch(() => {});
    else this.#sound.pause();
  }

  async hide(): Promise<void> {
    this.#active = false; this.#generation++;
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    this.#sound.pause(); this.#soundKey = ''; this.#uiKey = '';
    const animations = [...this.#animations.values()]; this.#animations.clear();
    await this.#bonus.hide();
    this.renderer.clearAnimatedSprites();
    await Promise.all(animations.map(animation => this.engine.closeFlic(animation.flic.decoderId)));
  }

  async show(state: TrackingState): Promise<void> {
    const entering = !this.#active;
    const newRound = entering || this.#state?.round !== state.round;
    this.#state = state;
    if (newRound) this.#transformed = false;
    if (!state.idleAnimation) this.#idleFinished = false;
    if (entering) {
      this.#active = true; this.#generation++;
      this.#lastTime = 0;
      await this.renderer.setScene([
        { resourceId: 'install/gbs/bg02.bmp', x: 0, y: 0, width: 640, height: 480 },
        { resourceId: 'install/gbs/gbs01.bmp', x: 101, y: 51, width: 433, height: 348 },
      ], this.assets);
    }
    const generation = this.#generation;
    if (!this.#active) return;
    if (state.phase === 'reward' && state.reward) {
      if (this.#uiKey !== 'reward') {
        this.#uiKey = 'reward';
        const animations = [...this.#animations.values()]; this.#animations.clear();
        await Promise.all(animations.map(animation => this.engine.closeFlic(animation.flic.decoderId)));
        await this.renderer.setScene([{ resourceId: 'install/gbs/bg02.bmp', x: 0, y: 0, width: 640, height: 480 }], this.assets);
        this.ui.replaceChildren();
        const status = document.createElement('span'); status.className = 'tracking-status sr-only'; status.textContent = `已找到 ${state.correct} 次异形，正在结算奖励。`;
        this.ui.append(status); this.inform(status.textContent);
      }
      await this.#bonus.show('alie', state.reward);
      if (this.#active && generation === this.#generation) this.#schedule();
      return;
    }
    const key = `${state.round}:${state.phase}:${state.selected}:${state.correct}`;
    if (key !== this.#uiKey) {
      this.#uiKey = key;
      await this.#overlays(state);
      if (!this.#active || generation !== this.#generation) return;
      this.#controls(state);
      void this.#playSound(state).catch(error => this.inform(`音效读取失败：${String(error)}`));
    }
    await this.#syncSprites(state, generation);
    if (!this.#active || generation !== this.#generation) return;
    this.#schedule();
  }

  async #overlays(state: TrackingState): Promise<void> {
    const images: SceneImage[] = [{ resourceId: 'install/gbs/yi.bmp', x: 0, y: 172, width: 103, height: 137, transparentPaletteIndex: 1 }];
    for (let round = 0; round < 3; round++) {
      images.push({ resourceId: `install/gbs/yao${state.outcomes[round] ? 2 : 1}.bmp`, x: 213 + round * 76, y: 417, width: 38, height: 47, transparentPaletteIndex: 1 });
      if (state.outcomes[round] === false) images.push({ resourceId: 'install/gbs/x.bmp', x: 219 + round * 76, y: 422, width: 41, height: 41, transparentPaletteIndex: 1 });
    }
    await this.renderer.setOverlays(images, this.assets);
    if (state.phase === 'reveal') {
      const bitmap = await this.assets.bitmap('install/gbs/gbs02.bmp', 1);
      if (this.#active && this.#state?.phase === 'reveal') this.renderer.setAnimatedSprite('tracking-prompt', bitmap, { x: 252, y: 266 }, 100);
    } else this.renderer.removeAnimatedSprite('tracking-prompt');
  }

  #desired(state: TrackingState, index: number): { resource: string; position: Point; once: boolean } {
    const object = state.objects[index]!;
    let name = sprites[object.sprite];
    let y = object.y;
    let once = object.sprite >= 5;
    if (!name) throw new Error('原版小游戏动画索引不存在');
    if (state.phase === 'reveal' && index === state.revealedTarget && !this.#transformed) {
      name = object.x < 561 ? 'gbu12' : 'gbn02'; y -= 20; once = name === 'gbu12';
    }
    if (state.phase === 'roundResult') {
      if (index === state.selected) { name = index === state.target ? 'gbn01' : 'gbn07'; once = true; }
      else if (index === state.target) { name = 'change_n'; once = true; }
    }
    return { resource: resource(name), position: { x: object.x, y }, once };
  }

  async #syncSprites(state: TrackingState, generation: number): Promise<void> {
    await Promise.all(state.objects.map(async (_object, index) => {
      const desired = this.#desired(state, index);
      const id = `tracking-object-${index}`;
      const existing = this.#animations.get(index);
      if (existing?.resource === desired.resource) {
        this.renderer.animatedSpritePosition(id, desired.position);
        return;
      }
      if (existing) {
        this.#animations.delete(index);
        await this.engine.closeFlic(existing.flic.decoderId);
      }
      let bytes = this.#bytes.get(desired.resource);
      if (!bytes) {
        bytes = this.assets.bytes(desired.resource); this.#bytes.set(desired.resource, bytes);
        void bytes.catch(() => this.#bytes.delete(desired.resource));
      }
      const flic = await this.engine.openFlic((await bytes).slice(0));
      if (!this.#active || generation !== this.#generation) { await this.engine.closeFlic(flic.decoderId); return; }
      this.#animations.set(index, { resource: desired.resource, flic, frame: 0, elapsedMs: 0, finished: false, once: desired.once });
      this.renderer.setAnimatedSprite(id, flic, desired.position, index);
    }));
  }

  #controls(state: TrackingState): void {
    this.ui.replaceChildren(); this.ui.hidden = false;
    const status = document.createElement('p'); status.className = 'tracking-status'; status.setAttribute('role', 'status');
    const text: Record<TrackingState['phase'], string> = {
      reveal: `第 ${state.round} / 3 轮 · 记住异形藏身的位置`,
      memorize: '记住位置后，开始追踪。',
      shuffle: '留意目标移动的位置。',
      settle: '留意目标移动的位置。',
      choose: '异形藏在哪里？请选择一个。',
      roundResult: state.selected === state.target ? '找到了！' : '这次没有找到。',
      reward: `已找到 ${state.correct} 次异形。`,
      finished: `已找到 ${state.correct} 次异形。`,
    };
    status.textContent = text[state.phase]; this.ui.append(status);
    if (state.phase === 'memorize' || state.phase === 'roundResult') {
      const button = document.createElement('button'); button.className = 'tracking-action';
      button.textContent = state.phase === 'memorize' ? '开始追踪' : state.round < 3 ? '下一轮' : '返回人生';
      button.onclick = () => this.#perform({ type: 'minigameAction', action: state.phase === 'memorize' ? 'startShuffle' : 'nextRound' });
      this.ui.append(button);
    }
    if (state.phase === 'choose') {
      for (const [index, object] of state.objects.entries()) {
        const button = document.createElement('button'); button.className = 'game-hitbox tracking-target';
        button.setAttribute('aria-label', `选择第 ${index + 1} 个`);
        Object.assign(button.style, { left: `${object.x}px`, top: `${object.y}px`, width: '70px', height: '80px' });
        button.onclick = () => this.#perform({ type: 'minigameAction', action: 'choose', index });
        this.ui.append(button);
      }
    }
    this.inform(text[state.phase]);
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
      this.#userAction = false; this.#lastTime = 0; this.#schedule();
    });
  }

  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#userAction || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.min(100, Math.round(now - this.#lastTime))) : 16;
      this.#lastTime = now;
      const generation = this.#generation;
      this.#step = this.#frame(elapsedMs, generation).catch(async error => {
        if (this.#active && generation === this.#generation) { this.inform(`小游戏暂停：${String(error)}`); await this.hide(); }
      }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }

  async #frame(elapsedMs: number, generation: number): Promise<void> {
    await this.#bonus.tick(elapsedMs);
    await Promise.all([...this.#animations].map(async ([index, animation]) => {
      if (animation.finished) return;
      animation.elapsedMs += elapsedMs;
      const delay = Math.max(1, animation.flic.frameDelayMs || 16);
      let advances = 0;
      while (animation.elapsedMs >= delay && advances++ < 4) {
        animation.elapsedMs -= delay;
        const frame = await this.engine.nextFlic(animation.flic.decoderId);
        if (!this.#active || generation !== this.#generation) return;
        animation.frame = (animation.frame + 1) % animation.flic.frameCount;
        this.renderer.animatedSpriteFrame(`tracking-object-${index}`, frame);
        if (animation.once && animation.frame === animation.flic.frameCount - 1) {
          animation.finished = true;
          if (animation.resource === resource('gbu12')) this.#transformed = true;
          if (this.#state?.idleAnimation?.[0] === index) this.#idleFinished = true;
          break;
        }
      }
    }));
    if (!this.#active || generation !== this.#generation || !this.#state) return;
    if (this.#state.phase === 'roundResult' || this.#state.phase === 'finished') return;
    const result = await this.send(this.#idleFinished && this.#state.phase !== 'reward'
      ? { type: 'minigameAction', action: 'finishIdleAnimation' }
      : { type: 'minigameTick', elapsedMs });
    if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
  }

  async #playSound(state: TrackingState): Promise<void> {
    const name = state.phase === 'shuffle' || state.phase === 'settle' ? 'gbsc01'
      : state.phase === 'roundResult' ? state.selected === state.target ? 'gbsc02' : 'gbsc03' : 'gbsc04';
    if (name === this.#soundKey) return;
    this.#soundKey = name;
    let url = this.#soundUrls.get(name);
    if (!url) {
      const bytes = await this.assets.bytes(this.assets.resolveLegacy(`sound/gb/${name}.wav`));
      url = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })); this.#soundUrls.set(name, url);
    }
    if (!this.#active || this.#soundKey !== name) return;
    this.#sound.src = url; this.#sound.loop = state.phase !== 'roundResult';
    if (this.#soundEnabled && document.visibilityState !== 'hidden') void this.#sound.play().catch(() => {});
  }
}
