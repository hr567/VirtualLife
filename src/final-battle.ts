import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { NativeRendering, type NativeFrames, type NativeImage } from './native-rendering';
import { EndingMedia } from './ending-media';
import type { Command, EngineResult, FinalBattleState, FinalResource, Snapshot } from './types';

type Action = Extract<Command, { type: 'battleAction' }>;
type Control = { element: HTMLButtonElement; image?: NativeImage; frame: number };

/** Final encounter's original presentation (00441a70, 00442930, 00443b40). */
export class FinalBattleView {
  #active = false; #generation = 0; #state: FinalBattleState | undefined;
  #render: NativeRendering; #media: EndingMedia; #meta = new Map<string, FinalResource>();
  #raf = 0; #lastTime = 0; #step: Promise<unknown> | undefined; #acting = false; #actions: Action[] = [];
  #scene = ''; #uiKey = ''; #controlsList: Control[] = []; #hover = -1;
  #labels: HTMLElement | undefined;
  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#render = new NativeRendering(renderer, assets, engine, 'final-battle'); this.#media = new EndingMedia(assets);
    for (const resource of Object.values({ ...assets.rules.finalBattle!.resourcesByAddress, ...assets.rules.finalBattle!.runtimeObjects })) this.#meta.set(resource.resource, resource);
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; this.#media.pause(document.hidden); if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; this.#render.pauseSounds(); } else this.#schedule(); });
    document.addEventListener('keydown', event => {
      if (!this.#active || event.repeat || document.querySelector('dialog[open]') || (event.target as HTMLElement).closest('input, select, .toolbar')) return;
      if (event.key === 'Enter' && ['intro', 'shop', 'opening'].includes(this.#state!.phase) || event.key === 'Escape' && this.#state!.phase === 'opening') { event.preventDefault(); this.#perform({ type: 'battleAction', action: 'continue' }); }
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#render.setSound(enabled); this.#media.setSound(enabled); }
  async hide(): Promise<void> {
    this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    this.#scene = this.#uiKey = ''; this.#actions = []; this.#hover = -1; this.#render.hide(); this.#media.hide(); this.renderer.paletteColors(); this.ui.classList.remove('final-battle-active');
  }
  #resource(address: number): FinalResource { const image = this.assets.rules.finalBattle!.resourcesByAddress[`0x${address.toString(16).padStart(8, '0')}`]; if (!image) throw new Error(`最终战素材缺失：${address.toString(16)}`); return image; }
  #image(address: number, x = 0, y = 0): NativeImage { const r = this.#resource(address); return { resource: r.resource, width: r.width!, height: r.height!, x, y, transparent: 1 }; }
  #animation(resource: FinalResource, frame: number, x = 0, y = 0, transparent: NativeImage['transparent'] = 1): NativeFrames { return { resource: resource.resource, width: resource.width!, height: resource.height!, frameCount: resource.frameCount!, interval: resource.frameDurationMs!, frame, x, y, transparent }; }
  async show(snapshot: Snapshot): Promise<void> {
    const state = snapshot.pending!.state as FinalBattleState, previous = this.#active ? this.#state : undefined;
    if (!this.#active) { this.#generation++; this.#lastTime = 0; }
    this.#active = true; this.#state = state; const generation = this.#generation, rules = this.assets.rules.finalBattle!;
    this.ui.classList.add('final-battle-active'); this.ui.dataset.nativePhase = state.phase; this.ui.dataset.nativeDetail = String(state.phase === 'intro' ? state.introStep : state.combatState); this.ui.dataset.nativeClock = String(state.clockMs);
    this.ui.dataset.nativeHp = String(state.hp); this.ui.dataset.nativeEnemyHp = String(state.enemyHp); this.ui.dataset.nativeItems = String(state.items.length); this.ui.dataset.nativeMoney = String(state.money);
    const scene = state.phase === 'intro' || state.phase === 'shop' ? 'intro' : state.phase;
    if (this.#scene !== scene) {
      this.ui.dataset.nativeReady = 'false'; await this.renderer.setScene(scene === 'intro' ? [{ resourceId: this.#resource(0x4960a4).resource, x: 0, y: 0, width: 640, height: 480 }] : [], this.assets);
      this.#scene = scene; this.#render.clear();
      if (state.phase !== 'opening') this.renderer.paletteColors(await this.#render.palette(this.#resource(scene === 'intro' ? 0x4960a4 : 0x4961fc).resource));
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.begin(); this.#controls(state);
    if (state.phase === 'opening') {
      this.#render.loop(); this.#media.pause(false);
      Object.assign(this.#media.element.style, { left: '0px', top: '60px', width: '640px', height: '360px' }); this.#media.element.classList.add('scanlines'); this.#media.element.setAttribute('aria-label', '最终战开场影片');
      this.ui.prepend(this.#media.element); await this.#media.frame(rules.openingVideo, state.clockMs - state.openingStartedMs);
      this.#media.sounds([{ resource: rules.openingAudio, position: state.clockMs - state.openingStartedMs, loop: false }]);
    } else {
      this.#media.hide(); this.#render.loop(this.#resource(0x496208).resource);
      if (state.phase === 'intro' || state.phase === 'shop') await this.#introduction(state);
      else await this.#combat(state);
    }
    for (const [index, control] of this.#controlsList.entries()) if (control.image && !control.element.hidden) {
      const image = control.image, height = image.height / 3; await this.#render.draw(`button-${index}`, image, state.phase === 'playing' ? 25 : 90, image, { x: 0, y: control.frame * height, width: image.width, height });
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.end(); this.#text(snapshot, state);
    if (previous?.soundSerial !== state.soundSerial) {
      for (const sound of state.soundStops ?? []) this.#render.stopSound(sound);
      for (const sound of state.sounds) if (this.assets.manifest.resources[sound]) void this.#render.sound(sound, true);
    }
    this.ui.dataset.nativeReady = 'true'; this.#schedule();
  }
  async #introduction(state: FinalBattleState): Promise<void> {
    const intro = state.phase === 'intro', step = state.introDisplayStep ?? state.introStep;
    if (!intro) {
      await this.#render.draw('shop-panel', this.#image(0x4960a8, 328, state.shopDisplayedPanelY ?? state.shopPanelY), 2);
      await this.#render.draw('money-panel', this.#image(0x496200, 27, 333), 3);
      await this.#render.draw('card-count', this.#image(0x496088, 209, 322), 3);
    }
    await this.#render.draw('dialogue-panel', this.#image(0x496204, 0, 356), 4);
    for (const [index, layer] of (state.layers ?? []).entries()) await this.#layer(`intro-${index}`, layer, 5 + index);
    if (intro && (step === 2 || step > 3)) await this.#render.draw('valuation-panel', this.#image(0x495fc4, 328, 9), 40);
    if (intro) return;
    await this.#items(state, true);
  }
  async #layer(id: string, layer: NonNullable<FinalBattleState['layers']>[number], z: number): Promise<void> {
    const resource = this.#meta.get(layer.resource ?? layer.assetPath!); if (!resource) throw new Error(`最终战素材不存在：${layer.resource ?? layer.assetPath}`);
    const transparent: NativeImage['transparent'] = layer.firstPixelTransparent ? 'firstPixel' : layer.transparentIndex;
    if (resource.kind === 'flc' || resource.kind === 'fli') await this.#render.animation(id, this.#animation(resource, layer.frame, layer.x, layer.y, transparent), z);
    else {
      const image = { resource: resource.resource, width: resource.width!, height: resource.height!, x: layer.x, y: layer.y, transparent };
      await this.#render.draw(id, image, z, image, { x: 0, y: layer.frame * 66, width: 55, height: 66 });
    }
  }
  async #items(state: FinalBattleState, shop: boolean): Promise<void> {
    const rules = this.assets.rules.finalBattle!, positions = shop ? rules.heldShopPositions : rules.heldBattlePositions;
    for (const [slot, item] of (shop && state.shopVisibleItems ? state.shopVisibleItems : state.items.slice(state.itemScroll, state.itemScroll + positions.length)).entries()) {
      if (item > 9) continue;
      const [x, y] = positions[slot]!; await this.#render.draw(`held-${slot}`, this.#image((shop ? 0x495ff4 : 0x495fc8) + item * 4, x, y), 20);
    }
    const selected = shop ? state.shopSelectedSell : state.selectedItem;
    if (selected !== null && selected !== undefined) { const position = positions[selected - state.itemScroll]; if (position) await this.#render.draw('selection', this.#image(0x495ff0, position[0]! - 1, position[1]! - 1), 21); }
  }
  async #combat(state: FinalBattleState): Promise<void> {
    const rules = this.assets.rules.finalBattle!;
    const layers = state.layers ?? [];
    const background = layers.find(layer => layer.resource === this.#resource(0x4961fc).resource || layer.assetPath === this.#resource(0x4961fc).resource);
    await this.#render.animation('background', this.#animation(this.#resource(0x4961fc), background?.frame ?? 0, 0, 0, null), 0);
    await this.#health(state);
    await this.#render.draw('attack-label', this.#image(0x49608c, 387, 440), 5); await this.#render.draw('defense-label', this.#image(0x496090, 387, 456), 5);
    for (const [index, card] of state.cards.entries()) {
      const selected = card.selected && !state.cardsMoving, position = selected ? rules.selectedCardPositions[index]! : [card.x, rules.cardPositions[index]![1]!];
      const image = this.#image((selected ? 0x4960f0 : 0x496128) + card.kind * 4, position[0], position[1]);
      await this.#render.draw(`card-${index}`, image, 10, image, selected ? undefined : { x: 0, y: 0, width: 55, height: 66 });
      if (selected) await this.#render.animation(`card-selected-${index}`, this.#animation(this.#resource(0x496098), Math.floor(state.clockMs / Math.max(1, this.#resource(0x496098).frameDurationMs!)) % 2, position[0], position[1]), 11);
      if (this.#hover === index && !state.cardsMoving) await this.#render.animation(`card-hover-${index}`, this.#animation(this.#resource(0x496094), Math.floor(state.clockMs / Math.max(1, this.#resource(0x496094).frameDurationMs!)) % 2, rules.cardPositions[index]![0], rules.cardPositions[index]![1]), 12);
    }
    await this.#items(state, false);
    if (state.selectedItem !== null && state.selectedItem !== undefined) { await this.#render.draw('selected-item-large', this.#image(0x496100 + state.items[state.selectedItem]! * 4, 463, 293), 22); await this.#render.draw('use-item', this.#image(0x4960c4, 538, 344), 22); }
    for (const [index, layer] of layers.entries()) {
      if (layer === background) continue; await this.#layer(`layer-${index}`, layer, 30 + index);
    }
    for (const [left, x] of [[true, 3], [false, 417]] as const) if (left ? state.itemScroll > 0 : state.itemScroll + 16 < state.items.length) { const resource = this.#resource(left ? 0x4960b4 : 0x4960b8); await this.#render.animation(`scroll-${left}`, this.#animation(resource, Math.floor(state.clockMs / Math.max(1, resource.frameDurationMs!)) % resource.frameCount!, x, 342), 80); }
  }
  async #health(state: FinalBattleState): Promise<void> {
    for (const [side, hp, range, x, y] of [[0, state.displayedEnemyHp, 6000, 12, 12], [1, state.displayedHp, 12000, 17, 317]] as const) {
      const band = Math.max(0, Math.trunc(hp / range)), remainder = Math.max(0, Math.trunc(hp % range / (range / 600)));
      await this.#render.draw(`hp-start-${side}`, this.#image(0x495f2c, x - 4, y), 2); await this.#render.draw(`hp-end-${side}`, this.#image(0x495f34, x + (side ? 601 : 600), y), 2);
      await this.#render.strip(`hp-border-${side}`, this.#image(0x495f30), x, y, 600, 2);
      if (band) await this.#render.strip(`hp-under-${side}`, this.#image(0x495f34 + Math.min(band, 5) * 4), x, y + 3, 600, 3); else this.#render.rectangle(`hp-empty-${side}`, x, y + 3, 600, 10, 3);
      await this.#render.strip(`hp-full-${side}`, this.#image(0x495f38 + Math.min(band, 4) * 4), x, y + 3, remainder, 3);
    }
  }
  #button(label: string, x: number, y: number, width: number, height: number, command: Action, image?: NativeImage): HTMLButtonElement {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label); Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` }); button.onclick = () => this.#perform(command);
    const control: Control = { element: button, image, frame: 0 }; button.onpointerenter = () => { control.frame = 1; }; button.onpointerleave = () => { control.frame = 0; }; button.onpointerdown = () => { control.frame = 2; }; button.onpointerup = () => { control.frame = 1; };
    this.#controlsList.push(control); this.ui.append(button); return button;
  }
  #controls(state: FinalBattleState): void {
    const rules = this.assets.rules.finalBattle!, shop = state.phase === 'shop';
    const key = `${state.phase}:${state.cards.length}:${state.items.length}:${state.itemScroll}:${state.shopSelectedBuy}:${state.shopSelectedSell}:${state.selectedItem}`;
    if (key !== this.#uiKey) {
      this.#uiKey = key; this.ui.replaceChildren(); this.#controlsList = [];
      if (state.phase === 'intro' || state.phase === 'opening') this.#button(state.phase === 'intro' ? '继续最终战介绍' : '跳过最终战开场', 0, 0, 640, 480, { type: 'battleAction', action: 'continue' });
      else if (shop) {
        rules.shopCardPositions.forEach(([x, y], index) => this.#button(`查看最终战道具 ${index + 1}`, x!, y!, 48, 48, { type: 'battleAction', action: 'inspectBuy', index }));
        this.#button('开始最终战', 542, 385, 52, 59, { type: 'battleAction', action: 'continue' }, this.#image(0x4960bc, 542, 385));
        if (state.shopSelectedBuy !== null && state.shopSelectedBuy !== undefined || state.shopSelectedSell !== null && state.shopSelectedSell !== undefined) {
          const buy = state.shopSelectedBuy !== null && state.shopSelectedBuy !== undefined;
          this.#button(buy ? '购买最终战道具' : '出售最终战道具', 393, 423, 59, 25, { type: 'battleAction', action: buy ? 'buy' : 'sell', index: buy ? state.shopSelectedBuy! : state.shopSelectedSell! }, { resource: 'install/public/button/y01.bmp', x: 393, y: 423, width: 59, height: 75, transparent: 1 });
          this.#button('取消道具交易', 458, 423, 59, 25, { type: 'battleAction', action: 'cancelShop' }, { resource: 'install/public/button/n01.bmp', x: 458, y: 423, width: 59, height: 75, transparent: 1 });
        }
      } else {
        rules.cardPositions.slice(0, state.cards.length).forEach(([x, y], index) => { const button = this.#button(`选择最终战卡片 ${index + 1}`, x!, y!, 48, 60, { type: 'battleAction', action: 'selectCard', index }); button.dataset.card = String(index); button.onpointerenter = button.onfocus = () => { this.#hover = index; }; button.onpointerleave = button.onblur = () => { this.#hover = -1; }; });
        this.#button('最终战出招', 375, 389, 53, 51, { type: 'battleAction', action: 'attack' }, this.#image(0x4960c0, 375, 389)).dataset.attack = 'true';
        if (state.selectedItem !== null && state.selectedItem !== undefined) this.#button('使用最终战道具', 538, 344, 47, 25, { type: 'battleAction', action: 'useItem' }).dataset.use = 'true';
      }
      if (shop || state.phase === 'playing') {
        const positions = shop ? rules.heldShopPositions : rules.heldBattlePositions;
        state.items.slice(state.itemScroll, state.itemScroll + positions.length).forEach((_, slot) => { const [x, y] = positions[slot]!; this.#button(`选择最终战道具 ${slot + state.itemScroll + 1}`, x!, y!, 21, 25, { type: 'battleAction', action: shop ? 'inspectSell' : 'selectItem', index: slot + state.itemScroll }).dataset.item = 'true'; });
        if (state.itemScroll > 0) this.#button('最终战道具向左', shop ? 24 : 3, shop ? 45 : 342, shop ? 21 : 30, shop ? 29 : 20, { type: 'battleAction', action: 'scrollLeft' }, shop ? this.#image(0x4960ac, 24, 45) : undefined);
        if (state.itemScroll + positions.length < state.items.length) this.#button('最终战道具向右', shop ? 295 : 417, shop ? 45 : 342, shop ? 21 : 30, shop ? 29 : 20, { type: 'battleAction', action: 'scrollRight' }, shop ? this.#image(0x4960b0, 295, 45) : undefined);
      }
      this.#labels = document.createElement('div'); this.#labels.className = 'battle-labels'; this.ui.append(this.#labels);
    }
    for (const button of this.ui.querySelectorAll<HTMLButtonElement>('[data-card]')) { button.disabled = state.combatState !== 0 || state.cardsMoving; button.setAttribute('aria-pressed', String(state.cards[Number(button.dataset.card)]?.selected)); }
    for (const button of this.ui.querySelectorAll<HTMLButtonElement>('[data-attack], [data-use]')) button.disabled = state.combatState !== 0 || !!button.dataset.use && state.itemUsed;
    for (const button of this.ui.querySelectorAll<HTMLButtonElement>('[data-item]')) button.disabled = shop ? !!state.shopAnimation : state.itemUsed;
  }
  #label(text: string, x: number, y: number, size = 16, color = '#fff', weight = '700'): HTMLElement {
    const element = document.createElement('span'); element.textContent = text; element.className = 'battle-label'; Object.assign(element.style, { left: `${x}px`, top: `${y}px`, fontSize: `${size}px`, color, fontWeight: weight }); this.#labels!.append(element); return element;
  }
  #text(snapshot: Snapshot, state: FinalBattleState): void {
    this.#labels!.replaceChildren(); const rules = this.assets.rules.finalBattle!, step = state.introDisplayStep ?? state.introStep;
    if (state.phase === 'intro' || state.phase === 'shop') {
      if (state.phase === 'shop' || step > 0) { const text = this.#label(state.phase === 'shop' ? state.shopMessage : state.introDisplayedMessage ?? rules.introTexts[Math.min(step - 1, 3)]!, 128, 371); Object.assign(text.style, { width: '400px', height: '84px', whiteSpace: 'pre-wrap', overflow: 'hidden', lineHeight: '16px' }); }
      if (state.phase === 'shop') { this.#label(String(state.money), 77, 334, 14, '#fff', '400'); this.#label(String(state.shopDisplayedItemCount ?? state.items.length), 269, 326, 14, '#fff', '400'); }
      else if (step === 2 || step > 3) {
        const career = snapshot.life?.profession ?? -1;
        const values = step === 2 ? [snapshot.life?.lifeLevel ?? 1, career < 0 ? rules.unemployedCareerName : rules.careerNames[career]!, (rules.careerHpMultipliers[career + 1]! / 10).toFixed(1), (rules.careerPower[career + 1]! / 10).toFixed(1), '0.0', (state.power / 10).toFixed(1), state.hp] : [...state.valuation, state.money];
        const labels = step === 2 ? rules.battleIndexLabels : [...rules.valuationLabels, rules.totalValuationLabel];
        for (const [index, label] of labels.entries()) { const [x, y] = rules.valuationLabelPositions[index]!, [vx, vy] = rules.valuationValuePositions[index]!; this.#label(label, 328 + x!, 9 + y!, 14, '#fff', '400'); this.#label(String(values[index]), 328 + vx!, 9 + vy!, 14, '#fff', '400'); }
      }
    } else if (state.phase !== 'opening') {
      this.#label(String(state.attack), 403, 440, 14, '#f00', '400'); this.#label(String(state.defense), 403, 456, 14, '#f00', '400');
      if (state.hpTextY > 300 && state.hpDelta) this.#label(`${state.hpDelta < 0 ? '减少' : '增加'}${Math.abs(state.hpDelta)}点`, 50, 300, 16, '#fff', '400');
      if (state.enemyTextY > 0) this.#label(`减少${state.enemyHpDelta}点`, 50, 0, 16, '#fff', '400');
      if (state.selectedItem !== null && state.selectedItem !== undefined) { const text = this.#label(rules.itemDescriptions[state.items[state.selectedItem]!]!, 470, 381); Object.assign(text.style, { width: '160px', height: '64px', whiteSpace: 'pre-wrap', overflow: 'hidden', lineHeight: '16px' }); }
      if (this.#hover >= 0 && state.cards[this.#hover] && !state.cardsMoving) { const card = state.cards[this.#hover]!, [x, y] = rules.cardPositions[this.#hover]!; this.#label(`攻击力 ${card.attack}`, x! + 10, y! + 60, 12, '#f00', '400'); this.#label(`防御力 ${card.defense}`, x! + 10, y! + 72, 12, '#f00', '400'); }
    }
  }
  #perform(command: Action): void {
    if (!this.#active) return; this.#actions.push(command); if (this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => { await this.#step; while (this.#active && this.#actions.length) { const result = await this.send(this.#actions.shift()!); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); } })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.hidden) return;
    this.#raf = requestAnimationFrame(now => { this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#render.pauseSounds(); this.#media.pause(true); this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = this.send({ type: 'battleTick', elapsedMs }).then(result => { if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }).catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`最终战暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
