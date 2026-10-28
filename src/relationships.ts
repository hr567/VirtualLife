import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer, SceneImage } from './renderer';
import type { Command, DecodedBitmap, EngineResult, OpenedFlic, Point, RelationshipHouse, Snapshot } from './types';

type Action = Extract<Command, { type: 'relationshipAction' }>;
interface Animation { flic: OpenedFlic; image: DecodedBitmap; elapsed: number }

/** The native relationship house. Each rendered hover frame is sent to Rust. */
export class RelationshipsView {
  #active = false;
  #generation = 0;
  #state: RelationshipHouse | undefined;
  #hover: number | undefined;
  #animations = new Map<string, Animation>();
  #visibleAnimations = new Map<string, { key: string; position: Point }>();
  #uiKey = '';
  #overlayKey = '';
  #text: HTMLElement | undefined;
  #numbers: HTMLElement[] = [];
  #answers: HTMLButtonElement[] = [];
  #answer = 0;
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #action = false;
  #music = gameAudio();
  #musicUrl = '';
  #soundEnabled = true;

  constructor(
    private renderer: GameRenderer,
    private assets: Assets,
    private engine: EngineClient,
    private ui: HTMLElement,
    private send: (command: Command) => Promise<EngineResult>,
    private inform: (message: string) => void,
  ) {
    this.#music.loop = true;
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') { cancelAnimationFrame(this.#raf); this.#raf = 0; this.#music.pause(); }
      else if (this.#active) { this.#schedule(); if (this.#soundEnabled) void this.#music.play().catch(() => {}); }
    });
    document.addEventListener('keydown', event => {
      if (!this.#active || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input,select,.toolbar')) return;
      if (event.key === 'Escape') { event.preventDefault(); this.#perform({ type: 'relationshipAction', action: 'close' }); }
      else if (this.#state?.phase === 'question' && ['ArrowUp', 'ArrowDown'].includes(event.key)) {
        event.preventDefault(); this.#selectAnswer(Math.max(0, Math.min(2, this.#answer + (event.key === 'ArrowUp' ? -1 : 1)))); this.#answers[this.#answer]?.focus();
      } else if (event.key === 'Enter' && this.#state?.phase === 'notice') {
        event.preventDefault(); this.#perform({ type: 'relationshipAction', action: 'advanceNotice' });
      }
    });
  }

  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void {
    this.#soundEnabled = enabled;
    if (enabled && this.#active && document.visibilityState === 'visible') void this.#music.play().catch(() => {});
    else this.#music.pause();
  }

  async hide(): Promise<void> {
    this.#active = false; this.#generation++;
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#hover = undefined;
    this.#uiKey = ''; this.#overlayKey = ''; this.#music.pause();
    this.ui.classList.remove('relationship-active'); this.ui.oncontextmenu = null;
    const animations = [...this.#animations.values()]; this.#animations.clear(); this.#visibleAnimations.clear();
    this.renderer.clearAnimatedSprites();
    await Promise.all(animations.map(animation => this.engine.closeFlic(animation.flic.decoderId)));
  }

  async show(snapshot: Snapshot): Promise<void> {
    const house = snapshot.pending!.state as RelationshipHouse;
    this.#state = house;
    const entering = !this.#active;
    if (entering) { this.#active = true; this.#generation++; this.#lastTime = 0; }
    const generation = this.#generation;
    if (entering) {
      const keys = ['pm0lb04a', 'pm0lb04b', 'pm0lb05a', 'pm0lb05b', 'pm0lb06a', 'pm0lb06b', 'select1', 'exit'];
      await Promise.all(keys.map(async key => {
        const path = key === 'exit' ? 'install/public/button/exita.flc' : `install/pm0l/${key}.flc`;
        const flic = await this.engine.openFlic(await this.assets.bytes(path), 1);
        if (!this.#active || generation !== this.#generation) { await this.engine.closeFlic(flic.decoderId); return; }
        this.#animations.set(key, { flic, image: flic, elapsed: 0 });
      }));
      if (!this.#active || generation !== this.#generation) return;
      await this.renderer.setScene([{ resourceId: 'install/pm0l/pm0lb01.bmp', x: 0, y: 0, width: 640, height: 480 }], this.assets);
      void this.#playMusic().catch(error => this.inform(`关系屋音乐读取失败：${String(error)}`));
    }
    if (!this.#active || generation !== this.#generation) return;
    const phase = house.phase;
    const candidates = phase === 'dates' || phase === 'dateConfirm' ? house.state.contacts.filter(id => id >= 0).slice(0, 4)
      : phase === 'marriages' || phase === 'marriageConfirm' ? house.marriageCandidates : house.offers;
    const key = `${phase}:${house.questionIndex}:${candidates.join(',')}:${house.selectedPerson}`;
    if (key !== this.#overlayKey) {
      this.#overlayKey = key;
      await this.#overlays(snapshot, candidates);
      if (!this.#active || generation !== this.#generation) return;
    }
    if (key !== this.#uiKey) { this.#uiKey = key; this.#controls(snapshot, candidates); }
    if (this.#text && phase !== 'question') this.#text.textContent = house.message;
    [house.state.money, house.state.charm, house.state.romance].forEach((value, index) => { if (this.#numbers[index]) this.#numbers[index]!.textContent = String(value); });
    this.#schedule();
  }

  async #overlays(snapshot: Snapshot, candidates: number[]): Promise<void> {
    const house = snapshot.pending!.state as RelationshipHouse;
    const career = snapshot.life?.profession ?? -1;
    const index = career < 0 ? house.state.stage < 2 ? 25 : house.state.stage <= 3 ? 26 : 27 : career;
    const portrait = this.assets.rules.relationships?.playerPortraits[house.state.sex]?.[index];
    if (!portrait) throw new Error('原版关系屋人物头像不存在。');
    const image = (path: string, position: Point): SceneImage => {
      const resourceId = this.assets.resolveLegacy(path), media = this.assets.object(resourceId).media!;
      return { resourceId, ...position, width: media.width!, height: media.height!, transparentPaletteIndex: 1 };
    };
    const images = [image(portrait, { x: 27, y: 265 })];
    if (house.phase !== 'main') {
      images.push(image('pm0l/pm0102.bmp', { x: 194, y: 66 }));
      for (const [index, id] of candidates.entries()) {
        const person = house.people[id];
        if (!person) throw new Error('原版关系屋人物编号不存在。');
        images.push(image(house.selectedPerson === id ? person.selectedPortrait : person.portrait, { x: 210 + index * 101, y: 78 }));
      }
    }
    images.push(image('pm0l/bl201a.bmp', { x: -91, y: 134 }));
    if (house.phase === 'dateConfirm' || house.phase === 'marriageConfirm') {
      for (const [name, x] of [['pm0lb02', 300], ['pm0lb08', 350]] as const) images.push({ ...image(`pm0l/${name}.bmp`, { x, y: 419 }), height: 13, frames: 3 });
    }
    await this.renderer.setOverlays(images, this.assets);
  }

  #control(label: string, x: number, y: number, width: number, height: number, action: Action): HTMLButtonElement {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label);
    Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` });
    button.onclick = () => this.#perform(action); this.ui.append(button); return button;
  }

  #controls(snapshot: Snapshot, candidates: number[]): void {
    const house = snapshot.pending!.state as RelationshipHouse;
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('relationship-active');
    this.ui.oncontextmenu = event => { event.preventDefault(); this.#perform({ type: 'relationshipAction', action: 'close' }); };
    this.#hover = undefined; this.#answers = []; this.#answer = 0;
    for (const id of this.#visibleAnimations.keys()) this.renderer.removeAnimatedSprite(id);
    this.#visibleAnimations.clear();
    const text = document.createElement('div'); text.className = 'relationship-text'; text.setAttribute('role', 'status');
    this.#text = text; this.ui.append(text);
    this.#numbers = [77, 229, 401].map(x => {
      const label = document.createElement('span'); label.className = 'relationship-number'; label.style.left = `${x}px`; this.ui.append(label); return label;
    });
    this.#animation('relationship-exit', 'exit', { x: 519, y: 292 });
    this.#control('离开关系屋', 519, 292, 62, 62, { type: 'relationshipAction', action: 'close' });
    if (house.phase === 'main' && house.state.stage >= 3) {
      const count = house.state.contacts.slice(0, 4).filter(id => id >= 0).length;
      for (const [index, action, label, available] of [[4, 'meet', '结识新朋友', count < 4], [5, 'dates', '选择约会对象', count > 0], [6, 'marriages', '选择结婚对象', house.marriageCandidates.length > 0]] as const) {
        if (!available) continue;
        const x = 366 + (index - 4) * 85, id = `relationship-menu-${index}`;
        this.#animation(id, `pm0lb0${index}a`, { x, y: 212 });
        const button = this.#control(label, x, 212, 70, 70, { type: 'relationshipAction', action });
        button.onpointerenter = button.onfocus = () => this.#animation(id, `pm0lb0${index}b`, { x, y: 212 });
        button.onpointerleave = button.onblur = () => this.#animation(id, `pm0lb0${index}a`, { x, y: 212 });
      }
    } else if (house.phase === 'main' || house.phase === 'notice') {
      this.#control('继续关系屋对话', 0, 0, 640, 480, { type: 'relationshipAction', action: house.phase === 'notice' ? 'advanceNotice' : 'close' });
    } else if (house.phase === 'question') {
      const question = snapshot.pending!.question;
      if (!question) throw new Error('原版关系屋问题尚未提供。');
      const prompt = document.createElement('p'); prompt.textContent = question.prompt; text.append(prompt);
      for (const [index, answer] of question.answers.entries()) {
        const button = document.createElement('button'); button.className = 'relationship-answer';
        button.textContent = `${this.assets.rules.relationships?.answerPrefixes[index] ?? ''}${answer}`;
        button.onclick = () => this.#perform({ type: 'relationshipAction', action: 'answer', index });
        button.onfocus = button.onpointerenter = () => this.#selectAnswer(index);
        this.#answers.push(button); text.append(button);
      }
      this.#selectAnswer(0);
    } else {
      for (const [index, id] of candidates.entries()) {
        const action = house.phase === 'meet' ? 'selectOffer' : house.phase === 'dates' || house.phase === 'dateConfirm' ? 'selectDate' : 'selectMarriage';
        const button = this.#control(`选择第 ${index + 1} 位人物`, 210 + index * 101, 78, 90, 100, { type: 'relationshipAction', action, index });
        button.dataset.person = String(id);
        if (house.phase === 'meet') {
          button.onpointerenter = button.onfocus = () => { this.#hover = index; };
          button.onpointerleave = button.onblur = () => { if (this.#hover === index) this.#hover = undefined; };
        }
      }
      if (house.phase === 'dateConfirm' || house.phase === 'marriageConfirm') {
        for (const [name, x, action, label] of [['pm0lb02', 300, 'confirm', '确定人选'], ['pm0lb08', 350, 'reject', '重新选择']] as const) {
          const resource = `install/pm0l/${name}.bmp`;
          const button = this.#control(label, x, 419, 46, 13, { type: 'relationshipAction', action });
          button.onpointerenter = button.onfocus = () => this.renderer.setSpriteFrame(resource, 1, 3);
          button.onpointerleave = button.onblur = () => this.renderer.setSpriteFrame(resource, 0, 3);
          button.onpointerdown = () => this.renderer.setSpriteFrame(resource, 2, 3);
        }
      }
    }
    this.inform('关系屋：按照画面提示选择人物或回答问题。');
  }

  #selectAnswer(index: number): void {
    this.#answer = index;
    const button = this.#answers[index];
    if (button) this.#animation('relationship-answer-cursor', 'select1', { x: 114, y: 371 + button.offsetTop });
  }

  #animation(id: string, key: string, position: Point): void {
    const animation = this.#animations.get(key);
    if (!animation) return;
    this.renderer.setAnimatedSprite(id, animation.image, position, 50);
    this.#visibleAnimations.set(id, { key, position });
  }

  #perform(command: Action): void {
    if (!this.#active || this.#action) return;
    this.#action = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step;
      if (!this.#active) return;
      const result = await this.send(command);
      if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
    })().catch(error => this.inform(String(error))).finally(() => { this.#action = false; this.#schedule(); });
  }

  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#action || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now;
      const generation = this.#generation;
      this.#step = this.#frame(elapsedMs, generation).catch(async error => {
        if (this.#active && generation === this.#generation) { this.inform(`关系屋暂停：${String(error)}`); await this.hide(); }
      }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }

  async #frame(elapsedMs: number, generation: number): Promise<void> {
    const used = new Set([...this.#visibleAnimations.values()].map(item => item.key));
    for (const key of [...used]) if (/pm0lb0[456][ab]/.test(key)) { used.add(key.slice(0, -1) + 'a'); used.add(key.slice(0, -1) + 'b'); }
    for (const [key, animation] of this.#animations) {
      animation.elapsed += elapsedMs;
      if (!used.has(key) || animation.flic.frameCount < 2 || animation.elapsed <= animation.flic.frameDelayMs) continue;
      animation.elapsed = 0;
      const frame = await this.engine.nextFlic(animation.flic.decoderId);
      if (!this.#active || generation !== this.#generation) return;
      animation.image = frame;
      for (const [id, visible] of this.#visibleAnimations) if (visible.key === key) this.renderer.animatedSpriteFrame(id, frame);
    }
    if (!this.#active || generation !== this.#generation) return;
    const result = await this.send({ type: 'relationshipTick', elapsedMs, ...(this.#state?.phase === 'meet' && this.#hover !== undefined ? { hoverIndex: this.#hover } : {}) });
    if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
  }

  async #playMusic(): Promise<void> {
    if (!this.#musicUrl) {
      const bytes = await this.assets.bytes(this.assets.resolveLegacy('sound/music/n2love.wav'));
      this.#musicUrl = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })); this.#music.src = this.#musicUrl;
    }
    if (this.#active && this.#soundEnabled && document.visibilityState === 'visible') void this.#music.play().catch(() => {});
  }
}
