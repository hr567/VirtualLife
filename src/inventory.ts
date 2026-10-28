import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { NativeRendering, type NativeFrames, type NativeImage } from './native-rendering';
import { indexedText } from './bitmap-font';
import type { Command, EngineResult, InventoryState, InventoryTab, Snapshot } from './types';

type Action = Extract<Command, { type: 'inventoryAction' }>;
type Control = { element: HTMLButtonElement; image?: NativeImage; frame: number; selected?: boolean };
const tabs: InventoryTab[] = ['ordinary', 'special', 'equipment'];

/** The original three inventory pages, including the original bitmap fonts. */
export class InventoryView {
  #active = false; #generation = 0;
  #render: NativeRendering; #fonts: Promise<ArrayBuffer[]>;
  #palette = new Uint8Array(1024); #scene = ''; #uiKey = ''; #controlsList: Control[] = []; #backHover = false;
  #raf = 0; #lastTime = 0; #step: Promise<unknown> | undefined; #acting = false; #actions: Action[] = [];
  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#render = new NativeRendering(renderer, assets, engine, 'inventory');
    const r = assets.rules.inventory!.rendering; this.#fonts = Promise.all([assets.bytes(r.asciiFont), assets.bytes(r.chineseFont)]);
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); });
  }
  get active(): boolean { return this.#active; }
  async hide(): Promise<void> { this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#scene = this.#uiKey = ''; this.#actions = []; this.#render.hide(); this.renderer.paletteColors(); this.ui.classList.remove('inventory-active'); }
  async show(snapshot: Snapshot): Promise<void> {
    const state = snapshot.pending!.state as InventoryState, rules = this.assets.rules.inventory!, r = rules.rendering;
    if (!this.#active) { this.#generation++; this.#lastTime = 0; }
    this.#active = true; const generation = this.#generation, equipment = state.tab === 'equipment';
    this.ui.classList.add('inventory-active'); this.ui.dataset.nativePhase = state.tab; this.ui.dataset.nativeClock = String(state.clockMs); this.ui.dataset.nativeItems = String(state.entries.length); this.ui.dataset.nativeHp = String(snapshot.stats.hp);
    const equipped = (snapshot.pending!.equipped ?? []) as number[]; this.ui.dataset.nativeEquipped = equipped.join(',');
    if (this.#scene !== state.tab) {
      this.ui.dataset.nativeReady = 'false'; const background = r.backgrounds[tabs.indexOf(state.tab)]!;
      await this.renderer.setScene([{ resourceId: background.resource, x: 0, y: 0, width: 640, height: 480 }], this.assets); this.#render.clear(); this.#scene = state.tab;
      this.#palette = new Uint8Array(await this.#render.palette(background.resource));
      const firstItem = state.entries[0] && rules[state.tab][state.entries[0].id];
      for (const [resource, first, last] of [[r.confirmation.resource, 191, 201], [r.back[0]!.resource, 162, 190], ...(firstItem ? [[firstItem.animation, 2, 161] as const] : []), ...(state.tab === 'ordinary' ? [[r.hpFill.resource, 142, 161] as const] : [])] as const) {
        const palette = await this.#render.palette(resource); this.#palette.set(palette.subarray(first * 4, (last + 1) * 4), first * 4);
      }
      this.renderer.paletteColors(this.#palette);
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.begin(); this.#controls(state);
    if (!equipment) for (let index = 0; index < 8; index++) await this.#render.draw(`row-${index}`, r.rows, 1, { x: 43, y: 80 + index * 38 });
    if (state.tab === 'ordinary') {
      await this.#render.draw('hp-label', r.hpLabel, 2); await this.#render.draw('hp-frame', r.hpFrame, 2);
      const width = Math.max(0, Math.min(260, Math.trunc(Number(snapshot.stats.hp) * 260 / Number(snapshot.stats.maxHp))));
      if (width) await this.#render.draw('hp-fill', r.hpFill, 3, r.hpFill, { x: 0, y: 0, width, height: 12 });
    }
    const entry = state.selectedIndex === null ? undefined : state.entries[state.selectedIndex], selected = entry && rules[state.tab][entry.id];
    if (selected) {
      const media = this.assets.object(selected.animation).media!;
      const animation: NativeFrames = { resource: selected.animation, width: media.width!, height: media.height!, x: state.previewOrigin?.x ?? (equipment ? 312 : 302), y: state.previewOrigin?.y ?? 47, frameCount: selected.frameCount, interval: 100, transparent: 1, frame: entry.frame };
      await this.#render.animation('preview', animation, 4);
    }
    const back = r.back[this.#backHover ? 1 : 0]!;
    await this.#render.animation('back', { ...back, x: equipment ? 553 : 543, frame: state.backTracks?.[this.#backHover ? 1 : 0]?.frame ?? 0 }, 5);
    const [ascii, chinese] = await this.#fonts;
    const label = (id: string, text: string, x: number, y: number, color = 255): void => this.#render.image(id, indexedText(ascii!, chinese!, text, color, this.#palette), { x, y }, 10, `${state.tab}:${text}:${color}`);
    for (const [index, item] of state.entries.entries()) {
      const definition = rules[state.tab][item.id]!, color = index === state.selectedIndex ? 1 : equipment && (item.equipped || equipped.includes(item.id)) ? 211 : 255;
      label(`item-${index}`, definition.name, equipment ? 49 + Math.floor(index / 5) * 114 : 50, equipment ? 100 + index % 5 * 29 : 86 + index * 38, color);
    }
    if (equipment) for (let slot = 0; slot < 6; slot++) { const id = equipped[slot] ?? -1; label(`equipped-${slot}`, id < 0 ? r.emptyEquipmentLabel : rules.equipment[id]!.name, 49 + Math.floor(slot / 3) * 114, 316 + slot % 3 * 29); }
    if (selected) selected.descriptions.forEach((description, index) => label(`description-${index}`, description, equipment ? 320 : 312, (equipment ? 288 : 291) + index * 20));
    if (state.confirmDrop) await this.#render.draw('confirm-panel', r.confirmation, 30);
    for (const [index, control] of this.#controlsList.entries()) if (control.image) {
      const image = control.image, frame = control.selected ? 2 : control.frame, height = image.height / 3;
      await this.#render.draw(`button-${index}`, image, control.element.dataset.confirm ? 32 : 6, image, { x: 0, y: frame * height, width: image.width, height });
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.end(); this.ui.dataset.nativeReady = 'true'; this.#schedule();
  }
  #button(label: string, image: NativeImage | undefined, x: number, y: number, width: number, height: number, command: Action, selected = false): HTMLButtonElement {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label); Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` }); button.onclick = () => this.#perform(command);
    const control: Control = { element: button, image, frame: 0, selected }; button.onpointerenter = () => { control.frame = 1; }; button.onpointerleave = () => { control.frame = 0; }; button.onpointerdown = () => { control.frame = 2; }; button.onpointerup = () => { control.frame = 1; };
    this.#controlsList.push(control); this.ui.append(button); return button;
  }
  #controls(state: InventoryState): void {
    const r = this.assets.rules.inventory!.rendering, equipment = state.tab === 'equipment', key = `${state.tab}:${state.confirmDrop}:${state.entries.map(item => item.id).join(',')}`;
    if (this.#uiKey !== key) {
      this.#uiKey = key; this.ui.replaceChildren(); this.#controlsList = [];
      const imageButton = (label: string, image: NativeImage, command: Action, selected = false): HTMLButtonElement => this.#button(label, image, image.x, image.y, image.width, image.height / 3, command, selected);
      for (const [index, tab] of tabs.entries()) imageButton(['一般道具', '特殊道具', '装备道具'][index]!, r.buttons[index]!, { type: 'inventoryAction', action: 'tab', tab }, tab === state.tab);
      if (equipment) {
        imageButton('装备选中道具', r.equipmentButtons[0]!, { type: 'inventoryAction', action: 'wear' }).dataset.requiresItem = 'true';
        imageButton('卸下选中装备', r.equipmentButtons[1]!, { type: 'inventoryAction', action: 'unequip' }).dataset.requiresItem = 'true';
        imageButton('丢弃选中道具', r.equipmentButtons[2]!, { type: 'inventoryAction', action: 'drop' }).dataset.requiresItem = 'true';
      } else {
        if (state.tab === 'ordinary') imageButton('使用选中道具', r.buttons[3]!, { type: 'inventoryAction', action: 'use' }).dataset.requiresItem = 'true';
        imageButton('丢弃选中道具', { ...r.buttons[4]!, x: state.tab === 'special' ? 316 : 403 }, { type: 'inventoryAction', action: 'drop' }).dataset.requiresItem = 'true';
      }
      for (const [index, entry] of state.entries.entries()) {
        const definition = this.assets.rules.inventory![state.tab][entry.id]!, x = equipment ? 46 + Math.floor(index / 5) * 114 : 43, y = equipment ? 97 + index % 5 * 29 : 80 + index * 38;
        this.#button(`选择道具 ${definition.name} ${index + 1}`, undefined, x, y, equipment ? 100 : 178, equipment ? 21 : 27, { type: 'inventoryAction', action: 'select', index }).dataset.entry = String(index);
      }
      const back = this.#button('返回地图', undefined, equipment ? 553 : 543, 383, 62, 62, { type: 'inventoryAction', action: 'close' }); back.onpointerenter = back.onfocus = () => { this.#backHover = true; }; back.onpointerleave = back.onblur = () => { this.#backHover = false; };
      if (state.confirmDrop) {
        const shield = document.createElement('div'); shield.className = 'inventory-confirm-shield'; Object.assign(shield.style, { position: 'absolute', inset: '0', pointerEvents: 'auto' }); this.ui.append(shield);
        imageButton('确定丢弃', r.confirmYes, { type: 'inventoryAction', action: 'confirmDrop' }).dataset.confirm = 'true';
        imageButton('取消丢弃', r.confirmNo, { type: 'inventoryAction', action: 'cancelDrop' }).dataset.confirm = 'true';
      }
      const description = document.createElement('p'); description.className = 'sr-only inventory-description'; this.ui.append(description);
    }
    for (const control of this.#controlsList) control.element.disabled = state.confirmDrop ? !control.element.dataset.confirm : !!control.selected || !!control.element.dataset.requiresItem && !state.entries.length;
    for (const button of this.ui.querySelectorAll<HTMLButtonElement>('[data-entry]')) button.setAttribute('aria-pressed', String(Number(button.dataset.entry) === state.selectedIndex));
    const selected = state.selectedIndex === null ? undefined : state.entries[state.selectedIndex]; this.ui.querySelector('.inventory-description')!.textContent = selected ? this.assets.rules.inventory![state.tab][selected.id]!.descriptions.join('。') : '当前没有道具';
  }
  #perform(command: Action): void {
    if (!this.#active) return; this.#actions.push(command); if (this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => { await this.#step; while (this.#active && this.#actions.length) { const result = await this.send(this.#actions.shift()!); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); } })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.hidden) return;
    this.#raf = requestAnimationFrame(now => { this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = this.send({ type: 'inventoryTick', elapsedMs }).then(result => { if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }).catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`道具界面暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
