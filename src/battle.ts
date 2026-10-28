import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { NativeRendering, type NativeFrames } from './native-rendering';
import type { BattleState, Command, EngineResult, Snapshot } from './types';

type Action = Extract<Command, { type: 'battleAction' }>;

/** Original 80-second card encounter, including its animated nine-page tutorial. */
export class BattleView {
  #active = false; #generation = 0; #state: BattleState | undefined;
  #render: NativeRendering; #meta = new Map<string, NativeFrames>();
  #raf = 0; #lastTime = 0; #step: Promise<unknown> | undefined; #acting = false; #actions: Action[] = [];
  #scene = ''; #uiKey = ''; #hover = -1; #attackHover = false; #attackPressed = false;
  #labels: HTMLElement | undefined; #attackButton: HTMLButtonElement | undefined;
  #bonusFrame = 0; #bonusAt = 0; #bonusKind = -1;
  #paletteKey = '';
  #playerName = '';
  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#render = new NativeRendering(renderer, assets, engine, 'battle');
    const collect = (value: unknown): void => { if (Array.isArray(value)) value.forEach(collect); else if (value && typeof value === 'object') { if ('resource' in value && 'frameCount' in value) this.#meta.set(String(value.resource), value as NativeFrames); else Object.values(value).forEach(collect); } };
    collect(assets.rules.battle!.rendering);
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; this.#render.pauseSounds(); } else this.#schedule(); });
    document.addEventListener('keydown', event => {
      if (!this.#active || event.repeat || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      const state = this.#state!;
      if (event.key === 'Enter' && (state.help || state.phase === 'intro')) { event.preventDefault(); this.#perform({ type: 'battleAction', action: 'start' }); }
      else if (event.key === 'F1' && !state.help && ['playing', 'result'].includes(state.phase)) { event.preventDefault(); this.#perform({ type: 'battleAction', action: 'showHelp' }); }
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#render.setSound(enabled); if (enabled && this.#active) this.#render.loop(this.assets.rules.battle!.rendering.sounds.music); }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    this.#scene = this.#uiKey = this.#paletteKey = ''; this.#hover = -1; this.#actions = []; this.#render.hide(); this.renderer.paletteColors();
    this.ui.classList.remove('battle-active');
  }
  #animation(animation: NativeFrames): NativeFrames { return { ...this.#meta.get(animation.resource)!, ...animation }; }
  #canAttack(state: BattleState): boolean { return state.phase === 'playing' && !state.help && !state.resultStarted && !state.won && !state.lost && !state.playerAttacking && !state.enemyAttacking && !state.dealing && state.selected.some(Boolean) && state.playerCooldown < Math.trunc(state.playerTenths / 10) && state.playerCooldown <= state.playerSeconds; }
  async show(snapshot: Snapshot): Promise<void> {
    const state = snapshot.pending!.state as BattleState, previous = this.#active ? this.#state : undefined;
    if (!this.#active) { this.#generation++; this.#lastTime = 0; }
    this.#active = true; this.#state = state; const generation = this.#generation, rules = this.assets.rules.battle!, r = rules.rendering;
    this.#playerName = snapshot.player?.name ?? '';
    this.ui.classList.add('battle-active'); this.ui.dataset.nativePhase = state.showOpening ? 'opening' : state.help ? 'help' : state.phase;
    this.ui.dataset.nativeDetail = `${state.attackPhase}`; this.ui.dataset.nativeClock = String(state.clockMs);
    this.ui.dataset.nativeEnemy = String(state.enemy); this.ui.dataset.nativeHp = String(state.hp); this.ui.dataset.nativeEnemyHp = String(state.enemyHp);
    const scene = r.backgrounds[state.background]!, sceneKey = state.showOpening ? 'opening' : scene.resource;
    if (this.#scene !== sceneKey) { this.ui.dataset.nativeReady = 'false'; await this.renderer.setScene(state.showOpening ? [] : [{ resourceId: scene.resource, x: 0, y: 0, width: 640, height: 480 }], this.assets); this.#scene = sceneKey; this.#render.clear(); this.#paletteKey = ''; }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.begin(); await this.#palette(state); this.#render.loop(r.sounds.music);
    if (state.showOpening) {
      for (let index = 0; index < 2; index++) await this.#render.draw(`door-${index}`, r.versusDoors[index]!, 1, { x: index ? state.openingRightX : state.openingLeftX, y: 0 });
      if (state.openingLeftX === 0 && state.openingRightX === 228) {
        const { stage, career, sex } = state.input, index = stage < 2 ? 25 : stage < 4 ? 26 : stage === 4 ? 27 : career < 0 ? 28 : career;
        await this.#render.draw('versus-enemy', r.versusEnemyPortraits[state.enemy]!, 2, { x: 23, y: 24 });
        await this.#render.draw('versus-player', r.versusPlayerPortraits[sex]![index]!, 2, { x: 523, y: 347 });
      }
      if (!previous) void this.#render.sound(r.sounds.closeDoor);
    } else if (state.phase === 'reward') await this.#reward(state);
    else if (state.help) {
      if (state.background < 5) await this.#render.draw('foreground', r.backgroundOverlays[state.background]!, 10);
      await this.#render.animation('help', this.#animation(state.layers[0] ?? state.tracks[`help${state.helpPage}`]!), 20);
    } else {
      const playing = state.phase !== 'intro';
      if (playing) {
        for (const [index, image] of r.panels.entries()) await this.#render.draw(`panel-${index}`, image, 1);
        for (let index = 0; index < 2; index++) await this.#render.draw(`time-${index}`, r.timeDigits, 2, { x: 304 + index * 16, y: 37 }, { x: 0, y: (index ? state.remaining % 10 : Math.trunc(state.remaining / 10)) * 23, width: 16, height: 23 });
        await this.#render.draw('help-button', r.healthBar, 2, { x: 620, y: 0 });
        await this.#health(state);
      }
      // The background's foreground is blitted after both actors, before cards/effects.
      const actorResources = new Set([...r.actors[state.input.sex]![state.playerClass]!, ...r.enemyActors[state.enemy]!].map(image => image.resource));
      let effectsStarted = false;
      for (const [index, layer] of state.layers.entries()) {
        const meta = this.#animation(layer), actor = actorResources.has(layer.resource), ready = r.healthFlares.some(image => image.resource === layer.resource);
        if (!ready && !actor) effectsStarted = true;
        await this.#render.animation(`layer-${index}`, meta, ready ? 5 : actor && !effectsStarted ? 10 + index : 40 + index);
      }
      if (!state.layers.length) for (const [index, name] of [`player${state.playerActor}`, `enemy${state.enemyActor}`].entries()) await this.#render.animation(`initial-actor-${index}`, this.#animation(state.tracks[name]!), 10 + index);
      if (state.background < 5) await this.#render.draw('foreground', r.backgroundOverlays[state.background]!, 25);
      for (let index = 5; index < 8; index++) if (playing || index === 5) await this.#render.draw(`front-panel-${index}`, r.panels[index]!, 26);
      if (playing && !state.resultStarted) {
        for (let index = 0; index < state.cardCount; index++) {
          const card = state.cards[index]!, selected = state.selected[index] && !state.dealing;
          const position = selected ? r.selectedCardPositions[index]! : [state.cardX[index]!, rules.cardPositions[index]![1]!];
          await this.#render.draw(`card-${index}`, (selected ? r.selectedCards : r.cards)[card.kind]!, 30, { x: position[0]!, y: position[1]! });
        }
        if (state.playerAttacking && state.attackPhase === 0) {
          let slot = 0;
          // The original uses the post-consumption card count for this loop.
          for (let index = 0; index < state.cardCount; index++) if (state.usedSelected[index]) { const position = r.comboCardPositions[state.comboCount - 1]![slot++]!; await this.#render.draw(`used-${index}`, r.cards[state.usedCards[index]!.kind]!, 40, { x: position[0]!, y: position[1]! }); }
        }
        if (state.strongAttack && state.playerAttacking && state.attackPhase === 1) {
          const portraits = state.input.career < 0 ? r.studentPortraits[state.input.sex]![Math.min(state.input.stage, 4)]! : r.careerPortraits[state.input.sex]![state.input.career]!;
          for (const [index, timer] of state.playerPortraitTimers.entries()) if (timer === 0) await this.#render.draw(`player-portrait-${index}`, portraits[index]!, 70, { x: 300 + index * 80, y: 100 });
        }
        if (state.enemyAttacking && state.attackPhase === 4 && state.enemyEffect === 1) for (const [index, timer] of state.enemyPortraitTimers.entries()) if (timer === 0) await this.#render.draw(`enemy-portrait-${index}`, r.enemyPortraits[state.enemy]![index]!, 70, { x: 200 + index * 80, y: 100 });
        if (state.attackPhase === 9) for (let index = 0; index < 8; index++) { const word = index < 4 ? index : 4 + state.godEffect * 4 + index - 4; await this.#render.draw(`god-word-${index}`, r.words[word]!, 70, { x: (index < 4 ? 100 : 140) + index % 4 * 80, y: index < 4 ? 50 : 120 }); }
        if (this.#canAttack(state)) { const button = r.attackButton, height = Math.trunc(button.height / 3); await this.#render.draw('attack', button, 80, button, { x: 0, y: height * (this.#attackPressed ? 2 : this.#attackHover ? 1 : 0), width: button.width, height }); }
      }
      if (state.lost && state.resultStarted && state.remaining !== 0) await this.#render.draw('loss', r.cross, 90, { x: 163, y: state.lossY });
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.end(); this.#controls(state); this.#text(state); this.#sounds(state, previous);
    this.ui.dataset.nativeReady = 'true'; this.#schedule();
  }
  async #palette(state: BattleState): Promise<void> {
    const r = this.assets.rules.battle!.rendering, effect = state.phase === 'intro' ? '' : state.paletteResource;
    if (state.showOpening) { if (this.#paletteKey !== 'opening') { this.renderer.paletteColors(await this.#render.palette(r.versusDoors[0]!.resource)); this.#paletteKey = 'opening'; } return; }
    const key = `${state.background}:${state.input.sex}:${state.playerClass}:${state.enemy}:${effect}`; if (key === this.#paletteKey) return;
    const palette = new Uint8Array(await this.#render.palette(r.backgrounds[state.background]!.resource));
    for (const [resource, first, last] of [[r.panels[2]!.resource, 142, 161], [r.cards[0]!.resource, 202, 254], [r.enemyActors[state.enemy]![0]!.resource, 102, 141], [r.actors[state.input.sex]![state.playerClass]![0]!.resource, 162, 201], ...(effect ? [[effect, 62, 101] as const] : [])] as const) {
      const source = await this.#render.palette(resource); palette.set(source.subarray(first * 4, (last + 1) * 4), first * 4);
    }
    this.renderer.paletteColors(palette); this.#paletteKey = key;
  }
  async #health(state: BattleState): Promise<void> {
    const r = this.assets.rules.battle!.rendering, images = r.hpDigits;
    for (let side = 0; side < 2; side++) {
      const width = Math.min(270, side ? state.maxHp : state.enemyMaxHp), hp = side ? state.displayedHp : state.displayedEnemyHp;
      const quotient = Math.trunc(hp / 270), remainder = hp % 270, x = side ? 621 - width : 16;
      await this.#render.draw(`hp-start-${side}`, images[side ? 8 : 0]!, 3, { x: side ? 617 - width : 10, y: 20 });
      await this.#render.draw(`hp-end-${side}`, images[side ? 9 : 2]!, 3, { x: side ? 621 : width + 16, y: 20 });
      await this.#render.strip(`hp-border-${side}`, images[1]!, x, 20, width, 3);
      if (quotient > 0) await this.#render.strip(`hp-under-${side}`, images[Math.min(quotient + 2, 7)]!, x, 23, width, 4);
      else this.#render.rectangle(`hp-empty-${side}`, x, 23, width, 10, 4);
      await this.#render.strip(`hp-full-${side}`, images[Math.min(quotient + 3, 7)]!, side ? 622 - remainder : 16, 23, remainder, 4);
    }
    if (state.playerTenths < state.playerCooldown * 10) { const x = Math.trunc(state.playerTenths * 148 / (state.playerCooldown * 10)) + 474; this.#render.rectangle('player-cooldown', x, 45, 623 - x, 11, 4); }
    if (state.enemyTenths < state.enemyCooldown * 10) { const x = Math.trunc(state.enemyTenths * 146 / (state.enemyCooldown * 10)) + 15; this.#render.rectangle('enemy-cooldown', x, 45, 162 - x, 11, 4); }
  }
  async #reward(state: BattleState): Promise<void> {
    const bonus = state.bonus; if (!bonus?.visible) return; const r = this.assets.rules.battle!.rendering.bonus;
    if (this.#bonusKind !== state.bonusKind) { this.#bonusKind = state.bonusKind; this.#bonusFrame = 0; this.#bonusAt = state.clockMs; }
    const animation = state.bonusKind === 2 ? r.loss : r.gain;
    if (state.clockMs - this.#bonusAt > 40 && this.#bonusFrame < animation.frameCount - 1) { this.#bonusAt = state.clockMs; this.#bonusFrame++; }
    await this.#render.draw('bonus-title', state.bonusKind === 1 ? r.experience : r.money, 100);
    await this.#render.animation('bonus-animation', { ...animation, frame: this.#bonusFrame }, 101);
    for (const [row, value, units, y] of [['value', bonus.value, bonus.valueUnits, 103], ['increment', bonus.increment, bonus.incrementUnits, 166]] as const) {
      const digits = String(Math.abs(value)).split(''); if (units !== null) digits[digits.length - 1] = String(units);
      for (let index = 0; index < digits.length; index++) await this.#render.draw(`bonus-${row}-${index}`, r.digits, 102, { x: 330 + (bonus.digitCount - digits.length + index) * 28, y }, { x: Number(digits[index]) * 26, y: 0, width: 26, height: 38 });
    }
  }
  #button(label: string, x: number, y: number, width: number, height: number, action: () => void): HTMLButtonElement {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label); Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` }); button.onclick = action; this.ui.append(button); return button;
  }
  #controls(state: BattleState): void {
    const key = `${state.phase}:${state.help}:${state.cardCount}:${state.showOpening}`;
    if (key !== this.#uiKey) {
      this.#uiKey = key; this.ui.replaceChildren(); this.#attackButton = undefined;
      if (state.showOpening) { /* The original door animation has no skip control. */ }
      else if (state.help || state.phase === 'intro') this.#button(state.help ? '下一页战斗教学' : '开始战斗', 0, 0, 640, 480, () => this.#perform({ type: 'battleAction', action: 'start' }));
      else if (state.phase === 'reward') this.#button('完成战斗奖励', 0, 0, 640, 480, () => this.#perform({ type: 'battleAction', action: 'skipReward' }));
      else if (state.phase === 'playing' || state.phase === 'result') {
        this.#button('战斗帮助', 620, 0, 20, 24, () => this.#perform({ type: 'battleAction', action: 'showHelp' }));
        if (state.phase === 'playing') {
          for (let index = 0; index < state.cardCount; index++) { const [x, y] = this.assets.rules.battle!.cardPositions[index]!; const button = this.#button(`选择战斗卡片 ${index + 1}`, x!, y!, 48, 60, () => this.#perform({ type: 'battleAction', action: 'selectCard', index })); button.dataset.card = String(index); button.onpointerenter = button.onfocus = () => this.#hoverCard(index); button.onpointerleave = button.onblur = () => this.#hoverCard(-1); }
          const image = this.assets.rules.battle!.rendering.attackButton; this.#attackButton = this.#button('出招', image.x, image.y, image.width, image.height / 3, () => this.#perform({ type: 'battleAction', action: 'attack' }));
          this.#attackButton.onpointerenter = () => { this.#attackHover = true; }; this.#attackButton.onpointerleave = () => { this.#attackHover = this.#attackPressed = false; }; this.#attackButton.onpointerdown = () => { this.#attackPressed = true; }; this.#attackButton.onpointerup = () => { this.#attackPressed = false; };
        }
      }
      this.#labels = document.createElement('div'); this.#labels.className = 'battle-labels'; this.ui.append(this.#labels);
    }
    for (const button of this.ui.querySelectorAll<HTMLButtonElement>('[data-card]')) { const index = Number(button.dataset.card); button.disabled = state.playerAttacking || state.dealing || state.resultStarted; button.setAttribute('aria-pressed', String(state.selected[index])); }
    if (this.#attackButton) this.#attackButton.hidden = !this.#canAttack(state);
  }
  #hoverCard(index: number): void { if (this.#hover === index) return; this.#hover = index; this.#perform({ type: 'battleAction', action: 'hoverCard', index: index < 0 ? undefined : index }); }
  #label(text: string, x: number, y: number, size = 16, color = '#fff', weight = '700'): HTMLElement {
    const element = document.createElement('span'); element.textContent = text; element.className = 'battle-label'; Object.assign(element.style, { left: `${x}px`, top: `${y}px`, fontSize: `${size}px`, color, fontWeight: weight }); this.#labels!.append(element); return element;
  }
  #text(state: BattleState): void {
    this.#labels!.replaceChildren();
    const r = this.assets.rules.battle!.rendering;
    if (state.showOpening) {
      if (state.openingLeftX === 0 && state.openingRightX === 228) {
        const rank = state.persistent.enemies[state.enemy]!.rank;
        for (const [text, x, y] of [[this.assets.rules.battle!.enemyNames[state.enemy]!, 34, 161], [String(rank + 1), 34, 213], [this.assets.rules.battle!.enemyDescriptions[state.enemy]!, 34, 267], [String(rank === 9 ? 0 : state.enemyHp), 34, 322], [this.#playerName, 526, 137], [String(state.input.lifeLevel), 556, 189], [r.classNames[state.input.sex]![state.playerClass]!, 526, 243], [String(state.hp), 504, 298]] as const) this.#label(text, x, y, 16, '#fff', '400');
      }
      return;
    }
    if (state.help || state.phase === 'reward') return;
    if (state.phase !== 'intro') { this.#label(this.assets.rules.battle!.enemyNames[state.enemy]!, 10, 1, 16, '#f00'); this.#label(String(state.attackPower), 478, 426, 16, '#f00'); this.#label(String(state.defensePower), 478, 441, 16, '#f00'); }
    if (state.text) { const text = this.#label(state.text, 120, 360); text.classList.add('battle-talk'); }
    if (state.shownPlayerDamage) this.#label(`${r.damagePrefix}${state.playerDamage}${r.damageSuffix}`, 420, 2, 16, '#f00');
    if (state.shownEnemyDamage) this.#label(`${r.damagePrefix}${state.enemyDamage}${r.damageSuffix}`, 100, 2, 16, '#f00');
    if (this.#hover >= 0 && this.#hover < state.cardCount && !state.dealing && !state.resultStarted) {
      const card = state.cards[this.#hover]!, [x, y] = this.assets.rules.battle!.cardPositions[this.#hover]!;
      if (card.kind < 4) { this.#label(r.cardAttackLabel, x! + 10, y! + 70, 12, '#f00', '400'); this.#label(String(card.attack), x! + 60, y! + 70, 12, '#f00', '400'); this.#label(r.cardDefenseLabel, x! + 10, y! + 82, 12, '#f00', '400'); this.#label(String(card.defense), x! + 60, y! + 82, 12, '#f00', '400'); }
      else r.cardDescriptions[card.kind - 4]!.forEach((text, index) => this.#label(text, x! + 10, y! + 70 + index * 12, 12, '#f00', '400'));
    }
  }
  #sounds(state: BattleState, previous?: BattleState): void {
    const r = this.assets.rules.battle!.rendering.sounds, sex = state.input.sex, stage = state.input.stage;
    const career = stage < 4 ? [29, 26, 27, 28][stage]! : state.input.career < 0 ? 25 : state.input.career;
    const resources: Record<string, string | undefined> = {
      card: r.card, showCards: r.showCards, cooldownReady: r.cooldownReady,
      playerWeak: r.playerVoices[sex]![stage < 3 ? 0 : 1], playerStrong: r.playerCareerVoice[sex]![career],
      enemyDefence: r.enemyDefence[state.enemy], shield: r.shield, shieldBounce0: r.cardSelect[0], shieldBounce1: r.cardSelect[1],
      playerHit: r.playerVoices[sex]![stage < 3 ? 2 : 3], enemyStrong: r.enemyAttack[state.enemy]![1],
      enemyDamage: r.enemyDamage[state.enemy], playerLoss: r.playerVoices[sex]![stage < 3 ? 4 : 5],
    };
    for (let index = 1; index <= 5; index++) resources[`combo${index}`] = r.cardCombo[index - 1];
    if (previous?.clockMs !== state.clockMs) {
      for (const sound of state.soundStops ?? []) this.#render.stopSound(resources[sound]);
      for (const sound of state.sounds) void this.#render.sound(resources[sound]);
    }
    if (state.bonus && previous?.bonus && state.bonus.increment !== previous.bonus.increment) void this.#render.sound('install/public/bonus/bonus.wav', true);
  }
  #perform(command: Action): void {
    if (!this.#active) return; this.#actions.push(command); if (this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => { await this.#step; while (this.#active && this.#actions.length) { const next = this.#actions.shift()!, result = await this.send(next); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); } })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.hidden) return;
    this.#raf = requestAnimationFrame(now => { this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#render.pauseSounds(); this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = this.send({ type: 'battleTick', elapsedMs }).then(result => { if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }).catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`战斗暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
