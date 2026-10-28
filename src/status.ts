import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import { NativeRendering, type NativeImage } from './native-rendering';
import { indexedText } from './bitmap-font';
import type { Command, DecodedBitmap, EngineResult, NativeTransition, Snapshot, StatusPanel } from './types';

type Action = Extract<Command, { type: 'statusAction' }>;
type Control = { element: HTMLButtonElement; image?: NativeImage; frame: number; fixedFrame?: number };

/** Original parameter pages preserve player values; property transitions consume the shared native RNG. */
export class StatusView {
  #active = false; #generation = 0; #state: StatusPanel | undefined;
  #render: NativeRendering; #fonts: Promise<ArrayBuffer[]>;
  #palette = new Uint8Array(1024); #scene = ''; #uiKey = ''; #controlsList: Control[] = []; #backHover = false;
  #rainMask: { step: number; image: DecodedBitmap } | undefined;
  #raf = 0; #lastTime = 0; #step: Promise<unknown> | undefined; #acting = false; #actions: Action[] = [];
  constructor(private renderer: GameRenderer, private assets: Assets, engine: EngineClient, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    this.#render = new NativeRendering(renderer, assets, engine, 'parameters');
    const r = assets.rules.inventory!.rendering; this.#fonts = Promise.all([assets.bytes(r.asciiFont), assets.bytes(r.chineseFont)]);
    document.addEventListener('visibilitychange', () => { this.#lastTime = 0; if (document.hidden) { cancelAnimationFrame(this.#raf); this.#raf = 0; } else this.#schedule(); });
    for (const type of ['keydown', 'keyup'] as const) document.addEventListener(type, event => {
      if (!this.#active || this.#state?.transition || !['house', 'investment'].includes(this.#state!.page) || event.repeat || document.querySelector('dialog[open]')) return;
      if (type === 'keydown' && event.key === 'Enter' || type === 'keyup' && event.key === 'Escape') { event.preventDefault(); this.#perform({ type: 'statusAction', action: 'close' }); }
    });
  }
  get active(): boolean { return this.#active; }
  setSound(enabled: boolean): void { this.#render.setSound(enabled); }
  async hide(): Promise<void> { this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0; this.#scene = this.#uiKey = ''; this.#actions = []; this.#render.hide(); this.renderer.paletteColors(); this.ui.classList.remove('parameters-active', 'property-preview'); }
  async show(snapshot: Snapshot): Promise<void> {
    const state = snapshot.pending!.state as StatusPanel, rules = this.assets.rules.statusRendering!, list = state.page === 'skills' || state.page === 'careerHistory' ? rules[state.page] : undefined;
    if (!this.#active) { this.#generation++; this.#lastTime = 0; }
    this.#active = true; this.#state = state; const generation = this.#generation;
    this.ui.classList.add('parameters-active'); this.ui.dataset.nativePhase = state.page; this.ui.dataset.nativeClock = String(state.clockMs); this.ui.dataset.nativeScroll = String(state.scroll); this.ui.dataset.nativeTransition = state.transition?.kind ?? 'none';
    const property = state.page === 'house' || state.page === 'investment', housing = snapshot.pending!.housing as { style: number; furniture: number; investmentStage: number };
    this.ui.classList.toggle('property-preview', property);
    if (this.#scene !== state.page) {
      this.ui.dataset.nativeReady = 'false'; const background = state.page === 'house' ? rules.house.styles[housing.style]!.layers[0]! : state.page === 'investment' ? rules.investment.background : list?.background ?? rules.background;
      await this.renderer.setScene(state.page === 'house' ? [] : [{ resourceId: background.resource, x: 0, y: 0, width: 640, height: 480 }], this.assets); this.#render.clear(); this.#scene = state.page; this.#rainMask = undefined;
      this.#palette = new Uint8Array(await this.#render.palette(background.resource));
      if (state.page === 'overview') { const portraitPalette = await this.#render.palette(rules.portraits[snapshot.statusOverview!.portraitIndex]!.resource); this.#palette.set(portraitPalette.subarray(202 * 4, 222 * 4), 202 * 4); }
      if (!property) { const backPalette = await this.#render.palette((list?.back ?? rules.back)[0]!.resource); this.#palette.set(backPalette.subarray(162 * 4, 191 * 4), 162 * 4); }
      this.renderer.paletteColors(this.#palette);
      if (property) { this.#render.begin(); void this.#render.sound(rules[state.page as 'house' | 'investment'].sound, true); }
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.begin(); this.#controls(snapshot, state);
    const [ascii, chinese] = await this.#fonts;
    const label = (id: string, text: unknown, x: number, y: number, color = 255, width?: number): void => {
      const content = String(text ?? ''), image = indexedText(ascii!, chinese!, content, color, this.#palette);
      this.#render.image(id, image, { x: x + (width === undefined ? 0 : (width * 8 - image.width) / 2), y }, 10, `${state.page}:${content}:${color}`);
    };
    if (state.page === 'overview') {
      const overview = snapshot.statusOverview!;
      await this.#render.draw('portrait', rules.portraits[overview.portraitIndex]!, 2);
      await this.#render.draw('hp-label', rules.hpLabel, 2); await this.#render.draw('hp-frame', rules.hpFrame, 2);
      if (overview.hpWidth > 0) await this.#render.draw('hp-fill', rules.hpFill, 3, rules.hpFill, { x: 0, y: 0, width: overview.hpWidth, height: 9 });
      const values = { ...snapshot.stats, ...snapshot.life, ...overview.fields, name: snapshot.player?.name };
      for (const entry of rules.texts) label(`field-${entry.field}`, (values as Record<string, unknown>)[entry.field], entry.x, entry.y, entry.color, entry.width);
      label('hp-shadow', overview.hpText, rules.hpText.shadowX, rules.hpText.shadowY, 0);
      label('hp-text', overview.hpText, rules.hpText.x, rules.hpText.y);
      label('mission-count', overview.missionCount, rules.missionRank.valueX, rules.missionRank.valueY, 0);
      label('mission-rank', overview.missionRank, rules.missionRank.textX, rules.missionRank.textY, 255, 10);
      if (!overview.hasMission) label('mission-empty', overview.mission, rules.mission.x, rules.mission.emptyY, 255, 30);
      else {
        let line = '', bytes = 0, row = 0;
        for (const character of overview.mission) { const count = character.charCodeAt(0) < 128 ? 1 : 2; if (bytes + count > 30) { label(`mission-${row}`, line, rules.mission.x, rules.mission.y + row++ * 18); line = ''; bytes = 0; } line += character; bytes += count; }
        if (line) label(`mission-${row}`, line, rules.mission.x, rules.mission.y + row * 18);
      }
    } else if (property) {
      if (state.page === 'house') {
        const layers = rules.house.styles[housing.style]!.layers.slice(0, housing.furniture + (state.transition ? 0 : 1));
        for (const [index, layer] of layers.entries()) await this.#render.draw(`furniture-${index}`, layer, index + 1);
        if (state.transition) this.#render.image('rain-mask', this.#mask(state.transition), { x: 0, y: 0 }, 20, 'rain-mask', state.transition.steps);
      }
      else { const building = rules.investment.stages.find(item => item.investmentStage === housing.investmentStage); if (building) await this.#render.draw('investment-building', building, 1); }
    } else if (list) {
      const history = snapshot.statusDetails?.careerHistory ?? [], skills = snapshot.statusDetails?.skills ?? [];
      const offset = state.page === 'skills' ? state.scroll * 9 : history.length > 10 ? Math.trunc(171 / (history.length - 10)) * state.scroll : 0;
      await this.#render.draw('scroll-thumb', list.scrollThumb, 2, { x: list.scrollThumb.x, y: list.scrollThumb.y + offset });
      if (state.page === 'skills') {
        for (const [index, row] of skills.slice(state.scroll, state.scroll + 4).entries()) {
          label(`skill-name-${index}`, row.name, list.nameX!, list.rowY + index * list.rowHeight, 255, list.nameWidth);
          for (const [column, item] of row.columns.entries()) {
            if (item.second) { label(`skill-first-${index}-${column}`, item.first, list.columns[column]!, list.firstLineY! + index * list.rowHeight, item.color, list.columnWidth); label(`skill-second-${index}-${column}`, item.second, list.columns[column]!, list.secondLineY! + index * list.rowHeight, item.color, list.columnWidth); }
            else label(`skill-empty-${index}-${column}`, item.first, list.columns[column]!, list.rowY + index * list.rowHeight, item.color, list.columnWidth);
          }
        }
      } else for (const [index, row] of history.slice(state.scroll, state.scroll + 10).entries()) {
        [row.year, row.name, row.description].forEach((value, column) => label(`history-${index}-${column}`, value, list.columns[column]!, list.rowY + index * list.rowHeight, 255, list.widths![column]));
      }
    }
    if (!property) { const back = (list?.back ?? rules.back)[this.#backHover ? 1 : 0]!;
      await this.#render.animation('back', { ...back, frame: state.backTracks[this.#backHover ? 1 : 0]?.frame ?? 0 }, 5); }
    for (const [index, control] of this.#controlsList.entries()) if (control.image) {
      const image = control.image, height = image.height / 3;
      await this.#render.draw(`button-${index}`, image, 6, image, { x: 0, y: (control.fixedFrame ?? control.frame) * height, width: image.width, height });
    }
    if (!this.#active || generation !== this.#generation) return;
    this.#render.end(); this.ui.dataset.nativeReady = 'true'; this.#schedule();
  }
  #mask(transition: NativeTransition): DecodedBitmap {
    if (this.#rainMask?.step === transition.steps) return this.#rainMask.image;
    const pixels = new Uint8Array(640 * 480 * 4);
    for (const [column, state] of transition.columns.entries()) {
      const revealed = state.finished ? 480 : state.y;
      for (let y = revealed; y < 480; y++) for (let x = column * transition.unitSize; x < (column + 1) * transition.unitSize; x++) pixels[(y * 640 + x) * 4 + 3] = 255;
    }
    const image = { width: 640, height: 480, pixels }; this.#rainMask = { step: transition.steps, image }; return image;
  }
  #button(label: string, image: NativeImage | undefined, x: number, y: number, width: number, height: number, command: Action): HTMLButtonElement {
    const button = document.createElement('button'); button.className = 'game-hitbox'; button.setAttribute('aria-label', label); Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` }); button.onclick = () => this.#perform(command);
    const control: Control = { element: button, image, frame: 0 }; button.onpointerenter = () => { control.frame = 1; }; button.onpointerleave = () => { control.frame = 0; }; button.onpointerdown = () => { control.frame = 2; }; button.onpointerup = () => { control.frame = 1; };
    this.#controlsList.push(control); this.ui.append(button); return button;
  }
  #controls(snapshot: Snapshot, state: StatusPanel): void {
    const r = this.assets.rules.statusRendering!, list = state.page === 'skills' || state.page === 'careerHistory' ? r[state.page] : undefined;
    if (this.#uiKey !== state.page) {
      this.#uiKey = state.page; this.ui.replaceChildren(); this.#controlsList = []; this.#backHover = false;
      const imageButton = (label: string, image: NativeImage, command: Action): HTMLButtonElement => this.#button(label, image, image.x, image.y, image.width, image.height / 3, command);
      if (state.page === 'house' || state.page === 'investment') this.#button(state.page === 'house' ? '关闭住宅预览' : '关闭投资预览', undefined, 0, 0, 640, 480, { type: 'statusAction', action: 'close' });
      else {
      if (state.page === 'overview') for (const [index, page] of (['house', 'investment', 'careerHistory', 'skills'] as const).entries()) {
        const enabled = index > 1 || (index === 0 ? snapshot.statusOverview!.canInspectHouse : snapshot.statusOverview!.canInspectInvestment);
        const button = imageButton(['住宅一览', '投资一览', '职业记录', '技能一览'][index]!, r.buttons[index]!, { type: 'statusAction', action: 'page', page }); button.disabled = !enabled;
        if (!enabled) { const control = this.#controlsList.at(-1)!; control.image = { ...r.disabledButtons[index]!, height: r.disabledButtons[index]!.height * 3 }; control.fixedFrame = 0; }
      }
      if (list) {
        list.scrollButtons.forEach((image, index) => imageButton(index ? '参数列表向下' : '参数列表向上', image, { type: 'statusAction', action: 'scroll', delta: index ? 1 : -1 }));
        if (list.tabs?.[0]) { imageButton('职业技能', list.tabs[0], { type: 'statusAction', action: 'page', page: 'skills' }).disabled = true; this.#controlsList.at(-1)!.fixedFrame = 2; }
        const [left, top, right, bottom] = list.scrollRect, track = this.#button('参数列表滚动条', undefined, left!, top!, right! - left!, bottom! - top!, { type: 'statusAction', action: 'scroll' });
        track.onclick = event => { const rect = this.ui.getBoundingClientRect(), y = (event.clientY - rect.top) * 480 / rect.height - top!, length = snapshot.statusDetails?.careerHistory.length ?? 0;
          this.#perform({ type: 'statusAction', action: 'scroll', index: state.page === 'skills' ? Math.max(0, Math.trunc(y / 9)) : length > 10 ? Math.max(0, Math.trunc(y / Math.max(1, Math.trunc(171 / (length - 10))))) : 0 });
        };
      }
      const back = (list?.back ?? r.back)[0]!, control = this.#button(state.page === 'overview' ? '返回地图' : '返回参数一览', undefined, back.x, back.y, back.width, back.height, { type: 'statusAction', action: 'close' });
      control.onpointerenter = control.onfocus = () => { this.#backHover = true; }; control.onpointerleave = control.onblur = () => { this.#backHover = false; };
      const description = document.createElement('p'); description.className = 'sr-only parameters-description'; this.ui.append(description);
      }
    }
    if (state.page === 'house' || state.page === 'investment') { this.#controlsList[0]!.element.disabled = !!state.transition; return; }
    const description = this.ui.querySelector('.parameters-description'); if (description) description.textContent = state.page === 'overview' ? `${snapshot.player?.name}，${snapshot.stats.age}岁，生命值${snapshot.statusOverview?.hpText}，${snapshot.statusOverview?.missionRank}` : state.page === 'skills' ? snapshot.statusDetails?.skills.slice(state.scroll, state.scroll + 4).map(row => row.name).join('，') ?? '' : snapshot.statusDetails?.careerHistory.slice(state.scroll, state.scroll + 10).map(row => `${row.year} ${row.name} ${row.description}`).join('，') ?? '';
  }
  #perform(command: Action): void {
    if (!this.#active || this.#state?.transition) return; this.#actions.push(command); if (this.#acting) return;
    this.#acting = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => { await this.#step; while (this.#active && this.#actions.length) { const result = await this.send(this.#actions.shift()!); if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); } })().catch(error => this.inform(String(error))).finally(() => { this.#acting = false; this.#schedule(); });
  }
  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#acting || document.hidden) return;
    this.#raf = requestAnimationFrame(now => { this.#raf = 0; if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now; const generation = this.#generation;
      this.#step = this.send({ type: 'statusTick', elapsedMs }).then(result => { if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message); }).catch(async error => { if (this.#active && generation === this.#generation) { this.inform(`参数界面暂停：${String(error)}`); await this.hide(); } }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
