import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { Command, DecodedBitmap, EndingState, EngineResult, EpilogueRendering, Snapshot } from './types';
import { EndingMedia } from './ending-media';
import { EndingSummaryView } from './ending-summary';

type Action = Extract<Command, { type: 'endingAction' }>;

/** Original evaluation, epilogue, staff scroll and reincarnation selection. */
export class EndingView {
  #active = false; #generation = 0; #state: EndingState | undefined;
  #raf = 0; #lastTime = 0; #step: Promise<void> | undefined; #acting = false;
  #actions: { command: Action; phase: EndingState['phase'] }[] = [];
  #phase = ''; #hovered = -1; #pressed = -1;
  #text: HTMLElement | undefined; #credits: HTMLElement | undefined;
  #media: EndingMedia; #rules: EpilogueRendering;
  #summary: EndingSummaryView;
  #buttons: DecodedBitmap[] = []; #cursor: DecodedBitmap | undefined;
  #cursorFrame = 0; #cursorLast = 0; #cursorInitial = true;
  #pointer: { x: number; y: number; type: string } | undefined;

  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#rules = assets.rules.epilogue!; this.#media = new EndingMedia(assets);
    this.#summary = new EndingSummaryView(renderer, assets, engine);
    document.addEventListener('pointermove', event => { this.#pointer = { x: event.clientX, y: event.clientY, type: event.pointerType }; this.#moveCursor(); });
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0; this.#media.pause(document.hidden || !!document.querySelector('dialog[open]'));
      if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule();
    });
    document.addEventListener('keydown', event => {
      if (!this.#acceptsKeyboard(event) || event.repeat) return;
      const phase = this.#state?.phase;
      if ((phase === 'summary' || phase === 'credits') && event.key.toLowerCase() === 's') { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'sDown' }); }
      else if (phase === 'endImage' && ['Escape', 'Enter', ' '].includes(event.key)) { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'continue' }); }
      else if (phase === 'reincarnation' && /^[yn]$/i.test(event.key)) { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'choose', index: event.key.toLowerCase() === 'y' ? 0 : 1 }); }
    });
    document.addEventListener('keyup', event => {
      if (!this.#acceptsKeyboard(event)) return;
      const phase = this.#state?.phase;
      if (event.key === 'Escape' && ['summary', 'movie', 'credits'].includes(phase ?? '')) { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'escape' }); }
      else if (event.key.toLowerCase() === 's' && (phase === 'summary' || phase === 'credits')) { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'sUp' }); }
    });
  }
  #acceptsKeyboard(event: KeyboardEvent): boolean { return this.#active && !document.querySelector('dialog[open]') && !(event.target as HTMLElement).closest('input, select, .toolbar'); }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#media.setSound(enabled); }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; this.#phase = ''; this.#lastTime = 0; this.#hovered = this.#pressed = -1; this.#actions = [];
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#media.hide(); this.#summary.hide(); this.#text = this.#credits = undefined;
    this.ui.classList.remove('ending-active', 'ending-choice'); this.renderer.paletteEffect(); this.renderer.paletteColors(); this.renderer.clearAnimatedSprites();
  }
  async show(snapshot: Snapshot): Promise<void> {
    const state = snapshot.pending!.state as EndingState, entering = !this.#active;
    this.#state = state; this.#active = true; if (entering) { this.#generation++; this.#lastTime = 0; }
    const generation = this.#generation;
    this.ui.classList.add('ending-active'); this.ui.classList.toggle('ending-choice', state.phase === 'reincarnation');
    this.ui.dataset.nativePhase = state.phase; this.ui.dataset.nativeClock = String(state.phase === 'summary' ? state.summary!.clockMs : state.phase === 'credits' ? state.credits?.clockMs ?? 0 : state.movie?.clockMs ?? 0);
    const phaseChanged = this.#phase !== state.phase;
    if (phaseChanged) {
      this.ui.dataset.nativeReady = 'false';
      this.#phase = state.phase; this.#media.hide(); this.ui.replaceChildren(); this.#text = this.#credits = undefined;
      this.renderer.paletteEffect(); this.renderer.paletteColors();
      const image = state.phase === 'endImage' ? this.#rules.credits.end : state.phase === 'reincarnation' ? this.#rules.choice.background : undefined;
      await this.renderer.setScene(image ? [{ resourceId: image.resource, x: (640 - image.width) / 2, y: (480 - image.height) / 2, width: image.width, height: image.height }] : [], this.assets);
      if (!this.#active || generation !== this.#generation) return;
      if (state.phase === 'summary') {
        const button = this.#control('继续结局', 0, 0, 640, 480, () => this.#perform({ type: 'endingAction', action: 'continue' }));
        button.oncontextmenu = event => { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'continue' }); }; this.ui.append(button);
        const description = document.createElement('div'); description.className = 'sr-only'; description.textContent = state.summary!.paragraphs.map(paragraph => paragraph.text).join('\n'); this.ui.append(description);
      } else if (state.phase === 'movie') {
        const area = this.#rules.video;
        Object.assign(this.#media.element.style, { left: `${area.x}px`, top: `${area.y}px`, width: `${area.width}px`, height: `${area.height}px` });
        this.#media.element.classList.toggle('scanlines', area.scanlines); this.ui.append(this.#media.element);
        const rect = this.#rules.subtitleRect; this.#text = document.createElement('p'); this.#text.className = 'ending-subtitle';
        Object.assign(this.#text.style, { left: `${rect[0]}px`, top: `${rect[1]}px`, width: `${rect[2] - rect[0]}px`, height: `${rect[3] - rect[1]}px`, fontSize: `${this.#rules.fontSize}px`, letterSpacing: `${this.#rules.characterSpacing}px` });
        this.ui.append(this.#text);
      } else if (state.phase === 'credits') {
        const rules = this.#rules.credits, rect = rules.rect, clip = document.createElement('div'); clip.className = 'ending-credits';
        Object.assign(clip.style, { left: `${rect[0]}px`, top: `${rect[1]}px`, width: `${rect[2] - rect[0]}px`, height: `${rect[3] - rect[1]}px`, fontSize: `${rules.fontSize}px`, letterSpacing: `${rules.characterSpacing}px`, color: rules.color });
        this.#credits = document.createElement('div'); this.#credits.className = 'ending-credit-content';
        for (const paragraph of rules.paragraphs) { const text = document.createElement('p'); text.textContent = paragraph.text; Object.assign(text.style, { top: `${paragraph.y}px`, height: `${paragraph.height}px` }); this.#credits.append(text); }
        clip.append(this.#credits); this.ui.append(clip);
      } else if (state.phase === 'endImage') {
        const button = this.#control('继续', 0, 0, 640, 480, () => this.#perform({ type: 'endingAction', action: 'continue' }));
        button.oncontextmenu = event => { event.preventDefault(); this.#perform({ type: 'endingAction', action: 'continue' }); }; this.ui.append(button);
      } else if (state.phase === 'reincarnation') await this.#choice();
    }
    if (!this.#active || generation !== this.#generation) return;
    if (state.phase === 'summary') {
      await this.#summary.show(state.summary!);
      if (!this.#active || generation !== this.#generation) return;
      this.#media.sounds([{ resource: this.assets.rules.endingSummary!.music, position: state.summary!.clockMs, loop: true }]);
    } else if (state.phase === 'movie' && state.movie) {
      const movie = state.movie;
      await this.#media.frame(movie.phase === 'subtitles' ? this.#rules.movie : this.#rules.outro, movie.displayedVideoTimeMs);
      if (!this.#active || generation !== this.#generation) return;
      const caption = movie.phase === 'subtitles' && movie.displayedSubtitle !== null ? this.#rules.subtitles[Number(state.victory)]![movie.displayedSubtitle] : undefined;
      this.#text!.textContent = caption?.text ?? ''; this.#text!.style.color = caption ? this.#rules.segments[caption.segment]!.color : '#fff';
      this.ui.dataset.nativeDetail = movie.phase;
      this.#media.sounds([
        { resource: this.#rules.music, position: movie.clockMs, loop: true },
        ...(movie.narrationStarted ? [{ resource: this.#rules.narration[Number(state.victory)]!, position: movie.clockMs - movie.narrationStartedMs, loop: false }] : []),
      ]);
    } else if (state.phase === 'credits' && state.credits && this.#credits) {
      this.#credits.style.transform = `translateY(${state.credits.offset}px)`; this.#credits.style.opacity = String(state.credits.brightness / 224);
    }
    this.ui.dataset.nativeReady = 'true';
    if (phaseChanged) this.#lastTime = 0;
    this.#schedule();
  }
  #control(label: string, x: number, y: number, width: number, height: number, action: () => void): HTMLButtonElement {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label);
    Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` }); button.onclick = action; return button;
  }
  async #choice(): Promise<void> {
    const rules = this.#rules.choice;
    this.#cursorFrame = 0; this.#cursorLast = 0; this.#cursorInitial = true;
    if (!this.#buttons.length) this.#buttons = await Promise.all(rules.buttons.map(button => this.assets.bitmap(button.resource, 1)));
    for (const [index, image] of rules.buttons.entries()) {
      this.renderer.setAnimatedSprite(`ending-choice-${index}`, this.#buttons[index]!, image, 1);
      this.#buttonFrame(index);
      const button = this.#control(index === 0 ? '再次轮回' : '结束人生', image.x, image.y, image.width, image.height / 3, () => this.#perform({ type: 'endingAction', action: 'choose', index }));
      button.onpointerenter = () => { this.#hovered = index; this.#buttonFrame(index); };
      button.onpointerleave = () => { this.#hovered = this.#pressed = -1; this.#buttonFrame(index); };
      button.onpointerdown = () => { this.#pressed = index; this.#buttonFrame(index); };
      button.onpointerup = () => { this.#pressed = -1; this.#buttonFrame(index); };
      this.ui.append(button);
    }
    this.#cursor ??= await this.assets.bitmap(rules.cursor.resource, 1);
    this.#moveCursor();
  }
  #moveCursor(): void {
    if (!this.#active || this.#state?.phase !== 'reincarnation' || !this.#cursor || !this.#pointer) return;
    const rect = this.ui.getBoundingClientRect(), pointer = this.#pointer;
    if (pointer.type === 'touch' || pointer.x < rect.left || pointer.x >= rect.right || pointer.y < rect.top || pointer.y >= rect.bottom) { this.renderer.removeAnimatedSprite('ending-cursor'); this.renderer.draw(); return; }
    this.renderer.setAnimatedSprite('ending-cursor', this.#cursor, { x: (pointer.x - rect.left) * 640 / rect.width, y: (pointer.y - rect.top) * 480 / rect.height }, 10);
    this.#drawCursorFrame();
  }
  #drawCursorFrame(): void { const cursor = this.#rules.choice.cursor; this.renderer.animatedSpriteSource('ending-cursor', { x: 0, y: this.#cursorFrame * cursor.height / 2, width: cursor.width, height: cursor.height / 2 }); }
  #buttonFrame(index: number): void { const image = this.#rules.choice.buttons[index]!; this.renderer.animatedSpriteSource(`ending-choice-${index}`, { x: 0, y: image.height / 3 * (this.#pressed === index ? 2 : this.#hovered === index ? 1 : 0), width: image.width, height: image.height / 3 }); }
  #perform(action: Action): void {
    if (!this.#active) return;
    this.#actions.push({ command: action, phase: this.#state!.phase }); if (this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step;
      while (this.#active && this.#actions.length) {
        const queued = this.#actions.shift()!;
        if (queued.phase !== this.#state?.phase) continue;
        const result = await this.send(queued.command);
        if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
      }
    })().catch(error => { this.#actions = []; this.inform(String(error)); }).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.hidden || this.#state?.phase === 'endImage') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      const paused = !!document.querySelector('dialog[open]'); this.#media.pause(paused);
      if (paused) { this.#lastTime = 0; this.#schedule(); return; }
      if (this.#state?.phase === 'reincarnation') {
        if (now - this.#cursorLast > 250) {
          this.#cursorLast = now;
          if (this.#cursorInitial) this.#cursorInitial = false;
          else { this.#cursorFrame = (this.#cursorFrame + 1) % 2; this.#drawCursorFrame(); }
        }
        this.#schedule(); return;
      }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = this.send({ type: 'endingTick', elapsedMs }).then(result => { if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }).catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`结局播放暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
