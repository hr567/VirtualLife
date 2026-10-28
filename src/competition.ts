import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { Command, CompetitionAnimation, CompetitionRendering, CompetitionState, DecodedBitmap, EngineResult, Point, Snapshot } from './types';

type Action = Extract<Command, { type: 'competitionAction' }>;

/** Native competition artwork, layout and ordered frame lists from the catalog/core. */
export class CompetitionView {
  #active = false; #generation = 0; #state: CompetitionState | undefined;
  #bitmaps = new Map<string, DecodedBitmap>(); #frames = new Map<string, DecodedBitmap[]>();
  #sprites = new Map<string, { resource: string; frame: number }>(); #wanted = new Set<string>();
  #raf = 0; #lastTime = 0; #step: Promise<void> | undefined; #acting = false;
  #scene = ''; #uiKey = ''; #text: HTMLElement | undefined; #selectedAnswer = 0; #hovered = -1;
  #hostFrame = 0; #hostAt = 0; #selectionFrame = 0; #selectionAt = 0;
  #sounds = new Map<string, HTMLAudioElement>(); #soundLoads = new Map<string, Promise<HTMLAudioElement>>();
  #wantedSounds = new Set<string>(); #soundEnabled = true;
  #rendering: CompetitionRendering;

  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#rendering = assets.rules.competition!.rendering;
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; if (document.visibilityState === 'hidden') { cancelAnimationFrame(this.#raf); this.#raf = 0; for (const sound of this.#sounds.values()) sound.pause(); } else { this.#schedule(); this.#playMusic(); } });
    document.addEventListener('keydown', event => {
      if (!this.#active || event.repeat || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      if (this.#state?.phase === 'running' && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
        event.preventDefault(); this.#perform({ type: 'competitionAction', action: event.key === 'ArrowLeft' ? 'left' : 'right' });
      } else if (this.#state?.phase === 'quizQuestion') {
        if (/^[a-dA-D]$/.test(event.key)) { event.preventDefault(); this.#selectAnswer(event.key.toLowerCase().charCodeAt(0) - 97); }
        else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') { event.preventDefault(); this.#selectAnswer(Math.max(0, Math.min(3, this.#selectedAnswer + (event.key === 'ArrowUp' ? -1 : 1)))); }
        else if (event.key === 'Enter') { event.preventDefault(); this.#perform({ type: 'competitionAction', action: 'answer', index: this.#selectedAnswer }); }
      } else if (['intro', 'quizIntro', 'quizResult'].includes(this.#state?.phase ?? '') && event.key === 'Enter') { event.preventDefault(); this.#perform({ type: 'competitionAction', action: 'continue' }); }
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#soundEnabled = enabled; if (enabled) this.#playMusic(); else for (const sound of this.#sounds.values()) sound.pause(); }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; this.#scene = this.#uiKey = ''; this.#hovered = -1;
    cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#wantedSounds.clear(); for (const sound of this.#sounds.values()) sound.pause();
    this.#sprites.clear(); this.renderer.clearAnimatedSprites(); this.ui.classList.remove('competition-active');
  }
  async #bitmap(resource: string): Promise<DecodedBitmap> {
    let image = this.#bitmaps.get(resource); if (!image) { image = await this.assets.bitmap(resource, 1); this.#bitmaps.set(resource, image); } return image;
  }
  async #animationFrames(resource: string): Promise<DecodedBitmap[]> {
    let frames = this.#frames.get(resource);
    if (!frames) {
      const flic = await this.engine.openFlic(await this.assets.bytes(resource), 1);
      try { frames = [flic]; for (let frame = 1; frame < flic.frameCount; frame++) frames.push(await this.engine.nextFlic(flic.decoderId)); }
      finally { await this.engine.closeFlic(flic.decoderId); }
      if (this.#frames.size >= 8) this.#frames.delete(this.#frames.keys().next().value!); this.#frames.set(resource, frames);
    }
    return frames;
  }
  async #drawBitmap(id: string, resource: string, position: Point, zIndex: number, source?: { x: number; y: number; width: number; height: number }): Promise<void> {
    this.#wanted.add(id); const old = this.#sprites.get(id);
    if (old?.resource !== resource) this.renderer.setAnimatedSprite(id, await this.#bitmap(resource), position, zIndex);
    else this.renderer.animatedSpritePosition(id, position);
    this.#sprites.set(id, { resource, frame: 0 }); if (source) this.renderer.animatedSpriteSource(id, source);
  }
  async #drawAnimation(id: string, animation: CompetitionAnimation, zIndex: number): Promise<void> {
    this.#wanted.add(id); const frames = await this.#animationFrames(animation.resource), frame = animation.frame ?? 0, old = this.#sprites.get(id);
    if (old?.resource !== animation.resource) this.renderer.setAnimatedSprite(id, frames[frame % frames.length]!, animation, zIndex);
    else { if (old.frame !== frame) this.renderer.animatedSpriteFrame(id, frames[frame % frames.length]!); this.renderer.animatedSpritePosition(id, animation); }
    this.#sprites.set(id, { resource: animation.resource, frame });
  }
  async show(snapshot: Snapshot): Promise<void> {
    const state = snapshot.pending!.state as CompetitionState, entering = !this.#active, previous = entering ? undefined : this.#state;
    this.#state = state; this.#active = true; if (entering) { this.#generation++; this.#lastTime = 0; this.#hostAt = state.clockMs; }
    this.ui.dataset.nativePhase = state.phase;
    this.ui.dataset.nativeDetail = state.running?.phase ?? state.beauty?.phase ?? '';
    this.ui.dataset.nativeClock = String(state.clockMs);
    const generation = this.#generation, rendering = this.#rendering;
    const scene = state.phase === 'arrival' ? 'arrival' : state.phase === 'rankPrize' || state.running?.clearedFrame && state.phase === 'running' ? 'black' : state.phase === 'intro' ? rendering.introBackgrounds[state.sex]![Math.max(0, Math.min(3, state.introPage - 1))]! : rendering.background.resource;
    if (scene !== this.#scene) {
      if (scene !== 'arrival') await this.renderer.setScene(scene === 'black' ? [] : [{ resourceId: scene, x: 0, y: 0, width: 640, height: 480 }], this.assets);
      this.#scene = scene; this.#sprites.clear(); this.#uiKey = '';
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#wanted.clear();
    if (state.phase === 'menu') {
      await this.#drawBitmap('competition-menu-title', rendering.menuTitle.resource, rendering.menuTitle, 10);
      await this.#drawBitmap('competition-menu-host', rendering.menuHost.resource, rendering.menuHost, 11);
      for (const [index, button] of rendering.menuButtons.entries()) await this.#drawBitmap(`competition-menu-${index}`, button.resource, button, 12, { x: 0, y: index === this.#hovered ? button.height : 0, width: button.width, height: button.height });
    } else if (['intro', 'beauty', 'running', 'quizIntro', 'quizQuestion', 'quizFeedback', 'quizResult'].includes(state.phase) && scene !== 'black') {
      await this.#drawBitmap('competition-talk', rendering.talkBackground.resource, rendering.talkBackground, 1);
      if (state.phase !== 'running') {
        if (state.clockMs - this.#hostAt > rendering.hostAnimation.interval) { this.#hostAt = state.clockMs; this.#hostFrame = (this.#hostFrame + 1) % rendering.hostAnimation.frameCount; }
        await this.#drawAnimation('competition-host', { ...rendering.hostAnimation, frame: this.#hostFrame }, 2);
      }
    }
    if (state.phase === 'moneyPrize') {
      const opening = this.assets.rules.competition!.openingAnimation;
      await this.#drawAnimation('competition-opening', { ...opening, frame: opening.frameCount - 1 }, 19);
    }
    if (state.animation) await this.#drawAnimation('competition-main', state.animation, 20);
    if (state.phase === 'beauty' && state.beauty) {
      const beauty = state.beauty;
      const portraits = [rendering.beautyPlayerPortraits[state.sex]![Math.max(0, Math.min(2, state.stage - 2))]!, ...rendering.beautyOtherPortraits];
      for (let index = 0; index < 4; index++) {
        const [x, y] = rendering.portraitPositions[index]!;
        const active = beauty.contestant === index && ['walkIn', 'pose', 'walkOut', 'afterWalk'].includes(beauty.phase);
        await this.#drawBitmap(`competition-portrait-${index}`, portraits[index]![active ? 0 : 1]!, { x, y }, 21);
        const [panelX, panelY] = rendering.scorePanelPositions[index]!;
        await this.#drawBitmap(`competition-panel-${index}`, rendering.scorePanelResource, { x: panelX, y: panelY }, 22);
      }
      for (const [index, animation] of beauty.layers.entries()) await this.#drawAnimation(`competition-beauty-${index}`, animation, 30 + index);
    }
    if (state.phase === 'running' && state.running) {
      const running = state.running, resources = this.assets.rules.competition!.running.rendering;
      if (running.showScorePanels && !running.clearedFrame) {
        const portraits = [resources.playerPortraits[state.sex]![Math.max(0, Math.min(2, state.stage - 2))]!, ...resources.otherPortraits];
        for (let index = 0; index < 4; index++) {
          const [x, y] = rendering.portraitPositions[index]!, [panelX, panelY] = rendering.scorePanelPositions[index]!;
          await this.#drawBitmap(`competition-portrait-${index}`, portraits[index]![running.displayedContestant === index ? 0 : 1]!, { x, y }, 21);
          await this.#drawBitmap(`competition-panel-${index}`, rendering.scorePanelResource, { x: panelX, y: panelY }, 22);
        }
      }
      for (const [index, animation] of running.layers.entries()) await this.#drawAnimation(`competition-running-${index}`, animation, index < (running.clearedFrame ? 0 : running.showScorePanels ? 2 : 1) ? index + 2 : index + 30);
    }
    if (state.phase.startsWith('quiz') && state.phase !== 'quizIntro') {
      await this.#drawBitmap('competition-scorebar', rendering.quizScoreBackground.resource, rendering.quizScoreBackground, 25);
      const score = state.exam?.score ?? 0;
      await this.#drawBitmap('competition-scoreicon', rendering.quizScoreResources[0]!, { x: rendering.quizScorePosition.x, y: rendering.quizScorePosition.baseY + Math.trunc(score / 10) * rendering.quizScorePosition.stepY }, 26);
      if (state.phase === 'quizFeedback') await this.#drawAnimation('competition-feedback', state.feedback[state.lastCorrect ? 1 : 0]!, 27);
    }
    const key = `${state.phase}:${state.introPage}:${state.exam?.answered}:${state.beauty?.phase}:${state.beauty?.contestant}:${state.beauty?.podiumPlace}:${state.running?.phase}:${state.running?.displayedContestant}:${state.running?.displayedTimes.join(',')}:${state.running?.podiumPlace}:${state.running?.clearedFrame}`;
    if (key !== this.#uiKey) { this.#uiKey = key; this.#controls(snapshot); }
    if (this.#text && state.phase !== 'menu') this.#text.textContent = snapshot.pending!.text ?? '';
    if (state.phase === 'quizQuestion') {
      if (state.clockMs - this.#selectionAt > rendering.quizSelection.interval) { this.#selectionAt = state.clockMs; this.#selectionFrame = (this.#selectionFrame + 1) % rendering.quizSelection.frameCount; }
      const answer = this.ui.querySelector<HTMLButtonElement>(`.competition-answer[data-index="${this.#selectedAnswer}"]`);
      if (answer) await this.#drawAnimation('competition-selection', { ...rendering.quizSelection, frame: this.#selectionFrame, x: rendering.quizText.x, y: rendering.quizText.y + answer.offsetTop }, 28);
    }
    for (const id of this.#sprites.keys()) if (!this.#wanted.has(id)) { this.renderer.removeAnimatedSprite(id); this.#sprites.delete(id); }
    if (!this.#active || generation !== this.#generation) return;
    this.#playMusic();
    if (state.phase !== previous?.phase) {
      const index = state.phase === 'opening' ? 15 : state.phase === 'rankPrize' ? 13 : state.phase === 'moneyPrize' ? 14 : previous?.phase === 'menu' ? 1 : undefined;
      if (index !== undefined) void this.#sound(rendering.sounds[index]!, false);
    }
    if (state.phase === 'beauty' && state.beauty?.phase === 'judges' && previous?.beauty?.phase !== 'judges') void this.#sound(rendering.sounds[5]!, false);
    if (state.phase === 'quizFeedback' && previous?.phase !== 'quizFeedback') void this.#sound(rendering.quizSounds[state.lastCorrect ? 0 : 1]!, false);
    if (state.phase === 'running' && state.clockMs !== previous?.clockMs) for (const cue of state.running?.sounds ?? []) if (cue === 'launch') void this.#sound(this.assets.rules.competition!.running.rendering.sounds.launch!, false);
    this.#schedule();
  }
  #textBox(value: string, area: { x: number; y: number; width: number; height: number; fontSize: number; color: string }, className = ''): HTMLElement {
    const element = document.createElement('div'); element.className = `competition-text ${className}`; element.textContent = value;
    Object.assign(element.style, { left: `${area.x}px`, top: `${area.y}px`, width: `${area.width}px`, height: `${area.height}px`, fontSize: `${area.fontSize}px`, lineHeight: `${area.fontSize}px`, color: area.color }); this.ui.append(element); return element;
  }
  #label(value: string, x: number, y: number, size = 12, color = '#0a0a0a'): void { this.#textBox(value, { x, y, width: 150, height: size, fontSize: size, color }); }
  #controls(snapshot: Snapshot): void {
    const state = this.#state!, rendering = this.#rendering;
    this.ui.replaceChildren(); this.ui.hidden = false; this.ui.classList.add('competition-active'); this.#text = undefined;
    const status = document.createElement('span'); status.className = 'sr-only competition-status'; status.textContent = snapshot.pending!.text ?? ''; this.ui.append(status);
    if (['arrival', 'opening', 'rankPrize', 'moneyPrize'].includes(state.phase)) return;
    this.#text = this.#textBox(snapshot.pending!.text ?? '', state.phase === 'menu' ? rendering.menuText : rendering.talkText, 'competition-commentary');
    if (state.phase === 'menu') {
      const messages = this.assets.rules.competition!.menu[Math.max(0, Math.min(2, state.stage - 2))]!;
      rendering.menuButtons.forEach((definition, index) => {
        const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', ['造型大赛', '飞人大赛', '智力大赛'][index]!);
        Object.assign(button.style, { left: `${definition.x}px`, top: `${definition.y}px`, width: `${definition.width}px`, height: `${definition.height}px` });
        button.onpointerenter = button.onfocus = () => { this.#hovered = index; this.renderer.animatedSpriteSource(`competition-menu-${index}`, { x: 0, y: definition.height, width: definition.width, height: definition.height }); if (this.#text) this.#text.textContent = messages[index + 1]!; };
        button.onpointerleave = button.onblur = () => { this.#hovered = -1; this.renderer.animatedSpriteSource(`competition-menu-${index}`, { x: 0, y: 0, width: definition.width, height: definition.height }); };
        button.onpointerdown = () => this.renderer.animatedSpriteSource(`competition-menu-${index}`, { x: 0, y: definition.height * 2, width: definition.width, height: definition.height });
        button.onclick = () => this.#perform({ type: 'competitionAction', action: 'choose', index }); this.ui.append(button);
      });
    } else if (['intro', 'quizIntro', 'quizResult'].includes(state.phase)) {
      const button = document.createElement('button'); button.className = 'game-hitbox competition-continue'; button.setAttribute('aria-label', '继续比赛'); button.onclick = () => this.#perform({ type: 'competitionAction', action: 'continue' }); this.ui.append(button);
    }
    if (state.phase === 'beauty' && state.beauty) {
      const beauty = state.beauty, names = [state.name, ...this.assets.rules.competition!.beautyOpponents[state.sex]!];
      for (let index = 0; index < 4; index++) {
        const [x, y] = rendering.scoreNamePositions[index]!, [scoreX, scoreY] = rendering.scoreValuePositions[index]!;
        this.#label(rendering.scoreLabels[0]!, x - 40, y);
        this.#label(rendering.scoreLabels[1]!, scoreX - 40, scoreY);
        this.#label(names[index]!, x, y);
        if (index < beauty.contestant || ['scores', 'podium', 'finished'].includes(beauty.phase) && index === beauty.contestant) this.#label(String(beauty.totals[index]), scoreX, scoreY);
      }
      if (beauty.phase === 'scores') beauty.scores[beauty.contestant]!.forEach((score, judge) => { const [x, y] = rendering.judgeScorePositions[judge]!; this.#label(String(score), x, y, 16, '#0a0a0a'); });
      if (beauty.phase === 'podium') for (let place = 3 - beauty.podiumPlace; place < 3; place++) { const [x, y] = rendering.podiumNamePositions[place]!; this.#label(names[beauty.winners[place]!]!, x, y, 16, '#ffffff'); }
    }
    if (state.phase === 'running' && state.running) {
      const running = state.running, resources = this.assets.rules.competition!.running.rendering, names = [state.name, ...this.assets.rules.competition!.runningOpponents[state.sex]!];
      if (running.clearedFrame) { this.#text?.remove(); this.#text = undefined; }
      if (running.showScorePanels && !running.clearedFrame) for (let index = 0; index < 4; index++) {
        const [x, y] = rendering.scoreNamePositions[index]!, [scoreX, scoreY] = rendering.scoreValuePositions[index]!;
        this.#label(resources.scoreLabels[0]!, x - 40, y); this.#label(resources.scoreLabels[1]!, scoreX - 40, scoreY);
        this.#label(names[index]!, x, y);
        if (index < running.displayedContestant) { const value = running.displayedTimes[index]!; this.#label(`${Math.trunc(value / 10).toString().padStart(2, '0')}'${value % 10}`, scoreX, scoreY); }
      }
      if (running.phase === 'podium') for (let place = 4 - running.podiumPlace; place < 3; place++) { const [x, y] = rendering.podiumNamePositions[place]!; this.#label(names[running.winners[place]!]!, x, y, 16, '#ffffff'); }
      if (running.contestant === 0 && ['preparing', 'running'].includes(running.phase)) for (const [index, action] of (['left', 'right'] as const).entries()) {
        const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', action === 'left' ? '左步加速' : '右步加速');
        Object.assign(button.style, { left: `${index * 320}px`, top: '0', width: '320px', height: '480px' });
        button.onpointerdown = event => { event.preventDefault(); this.#perform({ type: 'competitionAction', action }); };
        button.onclick = event => { if (event.detail === 0) this.#perform({ type: 'competitionAction', action }); }; this.ui.append(button);
      }
    }
    if (state.phase === 'quizQuestion' || state.phase === 'quizFeedback') {
      const question = state.phase === 'quizFeedback' && state.answeredQuestion ? state.answeredQuestion : state.exam!.current;
      const box = this.#textBox('', rendering.quizText, 'competition-question');
      const prompt = document.createElement('p'); prompt.textContent = question.question; box.append(prompt);
      const spacer = document.createElement('div'); spacer.style.height = '16px'; box.append(spacer);
      this.#selectedAnswer = 0;
      question.options.forEach((answer, index) => {
        const button = document.createElement('button'); button.className = 'competition-answer'; button.dataset.index = String(index); button.textContent = `   ${String.fromCharCode(65 + index)}:${answer}`; button.disabled = state.phase !== 'quizQuestion';
        const bytes = 5 + [...answer].reduce((count, character) => count + (character.charCodeAt(0) < 128 ? 1 : 2), 0);
        button.style.setProperty('--selection-width', `${Math.min(274, bytes * 8 - 2)}px`);
        button.onpointerenter = button.onfocus = () => this.#selectAnswer(index);
        button.onclick = () => this.#perform({ type: 'competitionAction', action: 'answer', index }); box.append(button);
      }); this.#selectAnswer(0);
    }
  }
  #selectAnswer(index: number): void { if (this.#selectedAnswer !== index) void this.#sound(this.#rendering.quizSounds[2]!, false); this.#selectedAnswer = index; for (const answer of this.ui.querySelectorAll<HTMLElement>('.competition-answer')) answer.classList.toggle('selected', Number(answer.dataset.index) === index); }
  #perform(command: Action): void {
    if (!this.#active || this.#acting) return; this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    if (command.action === 'answer') void this.#sound(this.#rendering.quizSounds[3]!, false);
    void (async () => { await this.#step; if (!this.#active) return; const result = await this.send(command); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = this.send({ type: 'competitionTick', elapsedMs }).then(result => { if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }).catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`比赛暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
  #playMusic(): void {
    const state = this.#state, indexes: number[] = [];
    if (state && this.#active) {
      if (['opening', 'menu'].includes(state.phase)) indexes.push(0);
      if (state.phase === 'intro') indexes.push(2);
      if (state.phase === 'beauty') { indexes.push(3); if (['walkIn', 'pose', 'walkOut'].includes(state.beauty?.phase ?? '')) indexes.push(4); }
      if (state.phase === 'running') { indexes.push(7); if (['resultPause', 'podium'].includes(state.running?.phase ?? '')) indexes.push(10); }
      if (state.phase.startsWith('quiz')) { indexes.push(11); if (state.phase === 'quizResult') indexes.push(13); }
    }
    this.#wantedSounds = new Set(indexes.map(index => this.#rendering.sounds[index]!));
    for (const [resource, sound] of this.#sounds) if (sound.loop && !this.#wantedSounds.has(resource)) sound.pause();
    for (const resource of this.#wantedSounds) void this.#sound(resource, true);
  }
  async #sound(resource: string, loop: boolean): Promise<void> {
    if (!resource || !this.#active || !this.#soundEnabled || document.visibilityState === 'hidden') return;
    let loading = this.#soundLoads.get(resource);
    if (!loading) {
      loading = this.assets.bytes(resource).then(bytes => { const sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); this.#sounds.set(resource, sound); return sound; });
      this.#soundLoads.set(resource, loading);
    }
    const sound = await loading;
    if (!this.#active || !this.#soundEnabled || document.hidden || loop && !this.#wantedSounds.has(resource)) return;
    sound.loop = loop;
    if (sound.paused) { sound.currentTime = 0; void sound.play().catch(() => {}); }
  }
}
