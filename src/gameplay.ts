import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import { GameRenderer, type SceneImage } from './renderer';
import { TrackingView } from './tracking';
import { HammerView } from './hammer';
import { FistView } from './fist';
import { GasView } from './gas';
import { FireView } from './fire';
import { FirebeeView } from './firebee';
import { CompetitionView } from './competition';
import { EndingView } from './ending';
import { BattleView } from './battle';
import { FinalBattleView } from './final-battle';
import { InventoryView } from './inventory';
import { StatusView } from './status';
import { MapOverviewView } from './map-overview';
import { RelationshipsView } from './relationships';
import { SpiritSlotsView } from './spirit-slots';
import { SpiritsView } from './spirits';
import { MapDecorationsView } from './map-decorations';
import type { Command, DecodedBitmap, EngineResult, FirebeeState, FireState, FistState, GameMap, GasState, HammerState, OpenedFlic, Point, Snapshot, SpiritLottery, TrackingState } from './types';

const directions = [[-1, 1], [-1, -1], [1, -1], [1, 1]] as const;
const turns = [[0, 8, undefined, 4], [5, 1, 9, undefined], [undefined, 6, 2, 10], [11, undefined, 7, 3]] as const;
interface Movement { type: 'moved'; fromTileId: number; tiles: number[]; rolled?: number }

/** Original map projection, recovered from 0x00417d60. */
export function worldPoint(map: GameMap, tileId: number): Point {
  const tile = map.chips[tileId];
  if (!tile) throw new Error('原地图格坐标不存在');
  return {
    x: map.chipOrigin.x + Math.floor(map.chipSize.width * (tile.column + 1) / 2) - Math.floor((tile.column + 1) / 2),
    y: map.chipOrigin.y + Math.floor(map.chipSize.height * (tile.row + 1) / 2),
  };
}

function textAt(text: string, x: number, y: number, className = ''): HTMLSpanElement {
  const element = document.createElement('span');
  element.textContent = text; element.className = `game-label ${className}`;
  element.style.left = `${x}px`; element.style.top = `${y}px`;
  return element;
}
function controlAt(label: string, x: number, y: number, width: number, height: number, onClick: () => void): HTMLButtonElement {
  const button = document.createElement('button'); button.setAttribute('aria-label', label); button.className = 'game-hitbox';
  Object.assign(button.style, { left: `${x}px`, top: `${y}px`, width: `${width}px`, height: `${height}px` });
  button.onclick = onClick;
  return button;
}

export class GameplayView {
  #actor: OpenedFlic | undefined;
  #actorResource = '';
  #actorPose: DecodedBitmap | undefined;
  #actorFacing = 0;
  #facingMap = '';
  #walkFrames = new Map<string, DecodedBitmap[]>();
  #snapshot: Snapshot | undefined;
  #busy = false;
  #generation = 0;
  #music = gameAudio();
  #musicPath = '';
  #musicUrl = '';
  #musicEnabled = true;
  #tracking: TrackingView;
  #hammer: HammerView;
  #fist: FistView;
  #gas: GasView;
  #fire: FireView;
  #firebee: FirebeeView;
  #competition: CompetitionView;
  #ending: EndingView;
  #battle: BattleView;
  #finalBattle: FinalBattleView;
  #inventory: InventoryView;
  #parameters: StatusView;
  #mapOverview: MapOverviewView;
  #relationships: RelationshipsView;
  #spiritSlots: SpiritSlotsView;
  #spirits: SpiritsView;
  #decorations: MapDecorationsView;
  #eventSceneKey = '';
  #cue: OpenedFlic | undefined;
  #cueTimer = 0;
  #cueGeneration = 0;
  #cannon: OpenedFlic | undefined;
  #cannonTimer = 0;
  #cannonGeneration = 0;
  #cannonFrame = 0;
  #cannonAnimating = false;
  #cannonWrapped = false;
  #diceChoices: Snapshot | undefined;

  constructor(
    private renderer: GameRenderer,
    private assets: Assets,
    private engine: EngineClient,
    private ui: HTMLElement,
    private send: (command: Command) => Promise<EngineResult>,
    private inform: (text: string) => void,
    private saveMenu: () => void,
    private optionsMenu: () => void,
  ) {
    this.#tracking = new TrackingView(renderer, assets, engine, ui, send, inform);
    this.#hammer = new HammerView(renderer, assets, engine, ui, send, inform);
    this.#fist = new FistView(renderer, assets, engine, ui, send, inform);
    this.#gas = new GasView(renderer, assets, engine, ui, send, inform);
    this.#fire = new FireView(renderer, assets, engine, ui, send, inform);
    this.#firebee = new FirebeeView(renderer, assets, engine, ui, send, inform);
    this.#competition = new CompetitionView(renderer, assets, engine, ui, send, inform);
    this.#ending = new EndingView(renderer, assets, engine, ui, send, inform);
    this.#battle = new BattleView(renderer, assets, engine, ui, send, inform);
    this.#finalBattle = new FinalBattleView(renderer, assets, engine, ui, send, inform);
    this.#inventory = new InventoryView(renderer, assets, engine, ui, send, inform);
    this.#parameters = new StatusView(renderer, assets, engine, ui, send, inform);
    this.#mapOverview = new MapOverviewView(renderer, assets, engine, ui, async () => { this.#mapOverview.hide(); if (this.#snapshot) await this.show(this.#snapshot); }, inform);
    this.#relationships = new RelationshipsView(renderer, assets, engine, ui, send, inform);
    this.#spiritSlots = new SpiritSlotsView(renderer, assets, ui, send, inform);
    this.#spirits = new SpiritsView(renderer, assets, engine, inform);
    this.#decorations = new MapDecorationsView(renderer, assets, engine, inform);
    this.#music.loop = true;
    document.addEventListener('keydown', event => {
      if (this.#diceChoices && /^[1-8]$/.test(event.key) && !event.repeat && !document.querySelector('dialog[open]')) {
        event.preventDefault(); void this.#chooseDie(Number(event.key));
      }
    });
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'hidden') { this.#music.pause(); clearTimeout(this.#cueTimer); this.#cueTimer = 0; clearTimeout(this.#cannonTimer); this.#cannonTimer = 0; }
      else {
        this.#scheduleCue();
        this.#scheduleCannon();
        if (!this.ui.hidden && this.#musicEnabled && !this.#tracking.active && !this.#hammer.active && !this.#fist.active && !this.#gas.active && !this.#fire.active && !this.#firebee.active && !this.#competition.active && !this.#relationships.active && !this.#ending.active && !this.#battle.active && !this.#finalBattle.active) void this.#music.play().catch(() => {});
      }
    });
  }

  setMusic(enabled: boolean): void { this.#musicEnabled = enabled; this.#tracking.setSound(enabled); this.#hammer.setSound(enabled); this.#fist.setSound(enabled); this.#gas.setSound(enabled); this.#fire.setSound(enabled); this.#firebee.setSound(enabled); this.#competition.setSound(enabled); this.#relationships.setSound(enabled); this.#ending.setSound(enabled); this.#battle.setSound(enabled); this.#finalBattle.setSound(enabled); if (enabled && !this.ui.hidden && !this.#tracking.active && !this.#hammer.active && !this.#fist.active && !this.#gas.active && !this.#fire.active && !this.#firebee.active && !this.#competition.active && !this.#relationships.active && !this.#ending.active && !this.#battle.active && !this.#finalBattle.active) void this.#music.play().catch(() => {}); else this.#music.pause(); }
  async hide(): Promise<void> {
    this.#generation++;
    this.#diceChoices = undefined;
    this.#snapshot = undefined; this.#facingMap = ''; this.#actorPose = undefined;
    this.#eventSceneKey = '';
    await this.#stopCue();
    await this.#stopCannon();
    await this.#tracking.hide();
    await this.#hammer.hide();
    await this.#fist.hide();
    await this.#gas.hide();
    await this.#fire.hide();
    await this.#firebee.hide();
    await this.#competition.hide();
    await this.#ending.hide();
    await this.#battle.hide();
    await this.#finalBattle.hide();
    await this.#inventory.hide();
    await this.#parameters.hide();
    this.#mapOverview.hide();
    await this.#relationships.hide();
    this.#spiritSlots.hide();
    this.#spirits.hide();
    this.#decorations.hide();
    this.ui.hidden = true; this.ui.replaceChildren(); this.#music.pause();
    if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
    this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
  }

  async showMapDecorations(map: GameMap, snapshot?: Snapshot, paused = false): Promise<void> {
    await this.#decorations.show(map, snapshot?.mapDecorations);
    this.#decorations.pause(paused);
  }

  #characterIndex(snapshot: Snapshot): number {
    const life = snapshot.life;
    if (!life || !snapshot.player) throw new Error('人物资料尚未建立');
    return (snapshot.player.sex === 'female' ? 31 : 0) + life.stage + (life.profession === -1 ? 0 : life.profession);
  }
  #animationResource(snapshot: Snapshot, direction = 0): string {
    const directory = this.assets.rules.characters.directories[this.#characterIndex(snapshot)];
    const suffix = this.assets.rules.characters.animationSuffixes[direction];
    if (!directory || !suffix) throw new Error('原人物动画索引不存在');
    const model = directory.replace(/\/$/, '').split('/').at(-1)!;
    return this.assets.resolveLegacy(directory + suffix.replace(/^M00/i, model));
  }

  async #showActor(snapshot: Snapshot, point: Point, facing = this.#actorFacing): Promise<void> {
    const resource = this.#animationResource(snapshot, facing);
    if (!this.#actor || this.#actorResource !== resource) {
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = await this.engine.openFlic(await this.assets.bytes(resource));
      this.#actorResource = resource;
      this.#actorPose = this.#actor;
    }
    this.renderer.setActor(this.#actorPose ?? this.#actor, point, resource);
    this.renderer.actorPosition(point);
  }

  #initialFacing(map: GameMap): number {
    const chip = map.chips.find(candidate => candidate.row === map.start.row && candidate.column === map.start.column);
    const next = chip?.next.find(candidate => candidate.row !== -1 && candidate.column !== -1);
    return chip && next ? Math.max(0, directions.findIndex(([column, row]) => next.column - chip.column === column && next.row - chip.row === row)) : 0;
  }

  async #animationFrames(snapshot: Snapshot, index: number): Promise<DecodedBitmap[]> {
    const resource = this.#animationResource(snapshot, index);
    let frames = this.#walkFrames.get(resource);
    if (!frames) {
      const flic = await this.engine.openFlic(await this.assets.bytes(resource));
      try {
        frames = [flic];
        for (let frame = 1; frame < flic.frameCount; frame++) frames.push(await this.engine.nextFlic(flic.decoderId));
      } finally { await this.engine.closeFlic(flic.decoderId); }
      // Keep one character's twelve directional sequences resident.
      if (this.#walkFrames.size >= 12) this.#walkFrames.delete(this.#walkFrames.keys().next().value!);
      this.#walkFrames.set(resource, frames);
    }
    return frames;
  }

  /** FUN_00414be0: ten turning frames, then 24 cumulative integer movement steps. */
  async #walk(snapshot: Snapshot, movements: Movement[], generation: number): Promise<void> {
    const map = this.assets.maps.maps.find(candidate => candidate.id === snapshot.mapId);
    if (!map || !movements.length) return;
    this.#busy = true; await this.#stopCue(); await this.#stopCannon();
    if (this.renderer.map?.id !== map.id) await this.renderer.setMap(map, this.assets, () => {});
    await this.showMapDecorations(map, snapshot);
    if (generation !== this.#generation) return;
    if (this.#facingMap !== map.id) { this.#facingMap = map.id; this.#actorFacing = snapshot.mapFacing ?? this.#initialFacing(map); }
    this.#eventSceneKey = ''; this.#buildUi(snapshot);
    for (const control of this.ui.querySelectorAll<HTMLButtonElement>('button')) control.disabled = true;
    await this.renderer.setOverlays(this.#hud(snapshot), this.assets);
    for (const movement of movements) {
      let previous = movement.fromTileId;
      let position = worldPoint(map, previous);
      for (const tileId of movement.tiles) {
        if (generation !== this.#generation) return;
        const from = map.chips[previous], to = map.chips[tileId];
        if (!from || !to) throw new Error('原版行走路径坐标不存在');
        const column = to.column - from.column, row = to.row - from.row;
        const facing = directions.findIndex(([x, y]) => column === x && row === y);
        if (facing < 0) throw new Error('原版行走路径不是相邻斜向格');
        const turning = turns[facing]![this.#actorFacing];
        if (turning !== undefined && turning > 3) {
          const frames = await this.#animationFrames(snapshot, turning);
          await this.#showActor(snapshot, position, turning);
          for (let frame = 0; frame < 10; frame++) {
            if (generation !== this.#generation) return;
            this.#actorPose = frames[frame % frames.length]!;
            this.renderer.actorFrame(this.#actorPose);
            await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
          }
        }
        this.#actorFacing = facing;
        const frames = await this.#animationFrames(snapshot, facing);
        await this.#showActor(snapshot, position, facing);
        await this.#spirits.show(snapshot, map, facing); this.#spirits.pause(false);
        const origin = position;
        for (let frame = 0; frame < 24; frame++) {
          if (generation !== this.#generation) return;
          position = { x: origin.x + Math.sign(column) * Math.trunc(map.chipSize.width * (frame + 1) / 48), y: origin.y + Math.sign(row) * Math.trunc(map.chipSize.height * (frame + 1) / 48) };
          this.#actorPose = frames[frame % frames.length]!;
          this.renderer.actorFrame(this.#actorPose); this.renderer.actorPosition(position);
          await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
        }
        previous = tileId;
      }
    }
    this.#spirits.pause(true);
  }

  #hud(snapshot: Snapshot): SceneImage[] {
    const images: SceneImage[] = [
      { resourceId: 'install/bmp/pm06a011.bmp', x: 0, y: 0, width: 4, height: 480 },
      { resourceId: 'install/bmp/pm06a013.bmp', x: 0, y: 476, width: 640, height: 4 },
      { resourceId: 'install/bmp/pm06a012.bmp', x: 512, y: 0, width: 128, height: 480, transparentPaletteIndex: 1 },
      { resourceId: 'install/bmp/pm06a010.bmp', x: 0, y: 0, width: 640, height: 30, transparentPaletteIndex: 1 },
    ];
    const portrait = this.assets.rules.characters.portraits[this.#characterIndex(snapshot)];
    if (portrait) images.push({ resourceId: this.assets.resolveLegacy(portrait), x: 541, y: 35, width: 88, height: 112, transparentPaletteIndex: 1 });
    const hp = snapshot.stats.hp ?? 0, maxHp = snapshot.stats.maxHp ?? 1;
    const healthWidth = Math.floor(hp * 102 / Math.max(1, maxHp));
    const frame = this.assets.object('install/bmp/pm06a04.bmp').media!;
    images.push({ resourceId: 'install/bmp/pm06a04.bmp', x: 156, y: 4, width: frame.width!, height: frame.height!, transparentPaletteIndex: 1 });
    if (healthWidth > 0) images.push({ resourceId: 'install/bmp/pm06a05.bmp', x: 161, y: 7, width: healthWidth, height: 9, source: { x: 0, y: 0, width: healthWidth, height: 9 }, transparentPaletteIndex: 1 });
    for (const [index, y] of [247, 286, 325, 364].entries()) images.push({ resourceId: `install/bmp/pm06a${String(index + 9).padStart(2, '0')}.bmp`, x: 544, y, width: 52, height: 30, frames: 3 });
    return images;
  }

  async show(snapshot: Snapshot, events: unknown[] = []): Promise<void> {
    if (this.#mapOverview.active) this.#mapOverview.hide();
    const generation = ++this.#generation;
    const previous = this.#snapshot; this.#snapshot = snapshot;
    this.ui.hidden = false;
    this.#decorations.pause(!!snapshot.pending);
    const slots = snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'spiritSlots' ? snapshot.pending.state as SpiritLottery : undefined;
    if (this.#spiritSlots.active) {
      if (slots) { await this.#spiritSlots.show(slots); return; }
      this.#spiritSlots.hide();
    }
    if (this.#fist.active && snapshot.pending?.game !== 'fist') await this.#fist.hide();
    if (this.#gas.active && snapshot.pending?.game !== 'gas') await this.#gas.hide();
    if (this.#fire.active && snapshot.pending?.game !== 'fire') await this.#fire.hide();
    if (this.#firebee.active && snapshot.pending?.game !== 'firebee') await this.#firebee.hide();
    if (this.#competition.active && snapshot.pending?.kind !== 'competition') await this.#competition.hide();
    if (this.#ending.active && snapshot.pending?.kind !== 'ending') await this.#ending.hide();
    if (this.#battle.active && (snapshot.pending?.kind !== 'battle' || snapshot.pending.game !== 'ordinary')) await this.#battle.hide();
    if (this.#finalBattle.active && (snapshot.pending?.kind !== 'battle' || snapshot.pending.game !== 'final')) await this.#finalBattle.hide();
    if (this.#inventory.active && snapshot.pending?.kind !== 'inventory') await this.#inventory.hide();
    if (this.#parameters.active && snapshot.pending?.kind !== 'status') await this.#parameters.hide();
    const die = events.find((event): event is { type: 'diceRolled'; rolled: number } => !!event && typeof event === 'object' && (event as { type?: string }).type === 'diceRolled');
    if (die && this.renderer.map) {
      this.#spirits.pause(true);
      await this.#playDie(die.rolled, generation);
      if (generation !== this.#generation) return;
    }
    const movements = events.filter((event): event is Movement => !!event && typeof event === 'object' && (event as Movement).type === 'moved' && Array.isArray((event as Movement).tiles) && (event as Movement).tiles.length > 0);
    if (previous && movements.length) {
      await this.#walk(previous, movements, generation);
      if (generation !== this.#generation) return;
    }
    if (snapshot.pending && !slots && ['relationship', 'minigame', 'dialogue', 'choice', 'ending', 'battle', 'inventory', 'status'].includes(snapshot.pending.kind)) this.#spirits.hide();
    if (snapshot.pending && !slots && !(snapshot.pending.kind === 'competition' && snapshot.pending.state?.phase === 'arrival')) this.#decorations.hide();
    if (snapshot.pending?.kind === 'relationship' && snapshot.pending.state) {
      if (this.#tracking.active) await this.#tracking.hide();
      if (this.#hammer.active) await this.#hammer.hide();
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#relationships.show(snapshot);
      return;
    }
    if (this.#relationships.active) await this.#relationships.hide();
    if (snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'tracking' && snapshot.pending.state) {
      if (this.#hammer.active) await this.#hammer.hide();
      this.#eventSceneKey = '';
      await this.#stopCue();
      await this.#stopCannon();
      this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#tracking.show(snapshot.pending.state as TrackingState);
      return;
    }
    if (this.#tracking.active) await this.#tracking.hide();
    if (snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'hammer' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue();
      await this.#stopCannon();
      this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#hammer.show(snapshot.pending.state as HammerState);
      return;
    }
    if (this.#hammer.active) await this.#hammer.hide();
    if (snapshot.pending?.kind === 'status' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#parameters.show(snapshot); return;
    }
    if (snapshot.pending?.kind === 'inventory' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#inventory.show(snapshot); return;
    }
    if (snapshot.pending?.kind === 'battle' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      if (snapshot.pending.game === 'final') await this.#finalBattle.show(snapshot); else await this.#battle.show(snapshot);
      return;
    }
    if (snapshot.pending?.kind === 'ending' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#ending.show(snapshot);
      return;
    }
    if (snapshot.pending?.kind === 'competition' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon();
      if (snapshot.pending.state.phase !== 'arrival') {
        this.#music.pause(); this.#spirits.hide();
        if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
        this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      } else this.#spirits.pause(true);
      await this.#competition.show(snapshot);
      return;
    }
    if (snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'firebee' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#firebee.show(snapshot.pending.state as FirebeeState);
      return;
    }
    if (snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'fire' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#fire.show(snapshot.pending.state as FireState);
      return;
    }
    if (snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'gas' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#gas.show(snapshot.pending.state as GasState);
      return;
    }
    if (snapshot.pending?.kind === 'minigame' && snapshot.pending.game === 'fist' && snapshot.pending.state) {
      this.#eventSceneKey = '';
      await this.#stopCue(); await this.#stopCannon(); this.#music.pause();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#fist.show(snapshot.pending.state as FistState);
      return;
    }
    if (snapshot.pending?.kind === 'dialogue' || snapshot.pending?.kind === 'choice') {
      await this.#showEvent(snapshot, generation);
      return;
    }
    this.#eventSceneKey = '';
    await this.#stopCue();
    this.#busy = true;
    this.ui.replaceChildren();
    const map = this.assets.maps.maps.find(candidate => candidate.id === snapshot.mapId);
    if (!map || snapshot.tileId === null) throw new Error('当前游戏地图资料不完整');
    if (this.renderer.map?.id !== map.id) await this.renderer.setMap(map, this.assets, (loaded, total) => this.inform(`正在读取原版地图 ${loaded} / ${total}`));
    if (generation !== this.#generation) return;
    if (this.#facingMap !== map.id) { this.#facingMap = map.id; this.#actorFacing = snapshot.mapFacing ?? this.#initialFacing(map); }
    else if (snapshot.mapFacing !== undefined) this.#actorFacing = snapshot.mapFacing;
    await this.#showActor(snapshot, worldPoint(map, snapshot.tileId));
    await this.showMapDecorations(map, snapshot, !!snapshot.pending);
    await this.#spirits.show(snapshot, map, this.#actorFacing);
    const images = this.#hud(snapshot);
    await this.renderer.setOverlays(images, this.assets);
    if (generation !== this.#generation) return;
    this.#busy = false;
    if (slots) {
      await this.#stopCannon();
      this.#buildUi(snapshot);
      await this.#spiritSlots.show(slots);
      void this.#setMusic(snapshot).catch(error => this.inform(`音乐读取失败：${String(error)}`));
      return;
    }
    this.#buildUi(snapshot);
    if (snapshot.phase === 'playing' && !snapshot.pending) await this.#startCannon();
    else await this.#stopCannon();
    const moved = events.find((event): event is { type: string; rolled: number } => !!event && typeof event === 'object' && (event as { type?: string }).type === 'moved');
    this.inform(moved ? `骰子 ${moved.rolled} 点 · 第 ${snapshot.turn} 回合` : snapshot.message);
    void this.#setMusic(snapshot).catch(error => this.inform(`音乐读取失败：${error instanceof Error ? error.message : String(error)}`));
  }

  async #showEvent(snapshot: Snapshot, generation: number): Promise<void> {
    this.#busy = true;
    this.ui.replaceChildren();
    await this.#stopCannon();
    if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
    this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
    const scene = snapshot.scene;
    const pattern = scene?.patternResource ? this.assets.resolveLegacy(scene.patternResource) : undefined;
    const background = scene?.backgroundResource ? this.assets.resolveLegacy(scene.backgroundResource) : undefined;
    const patternMedia = pattern ? this.assets.object(pattern).media : undefined;
    const tiled = patternMedia?.width === 80 && patternMedia.height === 160;
    const key = `${pattern}:${scene?.patternScrollMode ?? 0}:${background}`;
    if (key !== this.#eventSceneKey) {
      await this.#stopCue();
      const images: SceneImage[] = [];
      if (pattern && !tiled) images.push({ resourceId: pattern, x: 0, y: 0, width: patternMedia?.width ?? 640, height: patternMedia?.height ?? 480 });
      images.push({ resourceId: 'install/bmp/frame.bmp', x: 0, y: 0, width: 640, height: 480, transparentPaletteIndex: 1 });
      if (background) {
        const media = this.assets.object(background).media!;
        const framed = media.width === 500 && media.height === 268;
        images.push({ resourceId: background, x: framed ? 70 : 0, y: framed ? 31 : 0, width: media.width!, height: media.height! });
      }
      await this.renderer.setScene(images, this.assets);
      if (generation !== this.#generation) return;
      if (tiled && pattern) await this.renderer.setPattern(pattern, scene?.patternScrollMode ?? 0, this.assets);
      if (generation !== this.#generation) return;
      this.#eventSceneKey = key;
    }
    if (generation !== this.#generation) return;
    const overlays: SceneImage[] = [];
    for (const [path, x] of [[scene?.leftPortraitResource, 70], [scene?.rightPortraitResource, 482]] as const) {
      if (!path) continue;
      const resourceId = this.assets.resolveLegacy(path);
      const media = this.assets.object(resourceId).media!;
      overlays.push({ resourceId, x, y: 304, width: media.width!, height: media.height!, transparentPaletteIndex: 1 });
    }
    overlays.push({ resourceId: 'install/bmp/talk.bmp', x: 166, y: 304, width: 308, height: 112, transparentPaletteIndex: 0 });
    await this.renderer.setOverlays(overlays, this.assets);
    if (generation !== this.#generation) return;
    this.#busy = false;
    this.#pending(snapshot);
    if (snapshot.pending?.kind === 'dialogue') await this.#startCue();
    else await this.#stopCue();
    this.inform(snapshot.message);
  }

  async #startCue(): Promise<void> {
    if (this.#cue) { this.#scheduleCue(); return; }
    const generation = ++this.#cueGeneration;
    const cue = await this.engine.openFlic(await this.assets.bytes('install/ani/3.flc'), 1);
    if (generation !== this.#cueGeneration || this.ui.hidden) { await this.engine.closeFlic(cue.decoderId); return; }
    this.#cue = cue;
    this.renderer.setAnimatedSprite('dialogue-next', cue, { x: 446, y: 396 }, 200);
    this.#scheduleCue();
  }

  #scheduleCue(): void {
    if (!this.#cue || this.#cueTimer || this.ui.hidden || document.visibilityState === 'hidden') return;
    this.#cueTimer = window.setTimeout(() => {
      this.#cueTimer = 0;
      const cue = this.#cue;
      if (!cue) return;
      void this.engine.nextFlic(cue.decoderId).then(frame => {
        if (this.#cue !== cue) return;
        this.renderer.animatedSpriteFrame('dialogue-next', frame); this.#scheduleCue();
      }).catch(error => this.inform(`提示动画读取失败：${String(error)}`));
    }, 251);
  }

  async #stopCue(): Promise<void> {
    this.#cueGeneration++;
    clearTimeout(this.#cueTimer); this.#cueTimer = 0;
    const cue = this.#cue; this.#cue = undefined;
    this.renderer.removeAnimatedSprite('dialogue-next');
    if (cue) await this.engine.closeFlic(cue.decoderId);
  }

  async #startCannon(): Promise<void> {
    if (this.#cannon) { this.#scheduleCannon(); return; }
    const generation = ++this.#cannonGeneration;
    const cannon = await this.engine.openFlic(await this.assets.bytes('install/ani/pm06a14.flc'));
    if (generation !== this.#cannonGeneration || this.ui.hidden) { await this.engine.closeFlic(cannon.decoderId); return; }
    this.#cannon = cannon; this.#cannonFrame = 0; this.#cannonAnimating = false; this.#cannonWrapped = false;
    this.renderer.setAnimatedSprite('dice-cannon', cannon, { x: 544, y: 373 }, 50);
    this.#scheduleCannon();
  }

  #scheduleCannon(): void {
    if (!this.#cannon || this.#cannonTimer || this.ui.hidden || document.visibilityState === 'hidden') return;
    // 0x414ee0: wait >5 seconds, then advance each frame after >100 ms.
    this.#cannonTimer = window.setTimeout(() => {
      this.#cannonTimer = 0;
      const cannon = this.#cannon;
      if (!cannon) return;
      if (!this.#cannonAnimating) { this.#cannonAnimating = true; this.#scheduleCannon(); return; }
      if (++this.#cannonFrame === cannon.frameCount) {
        this.#cannonFrame = 0; this.#cannonAnimating = false; this.#cannonWrapped = true; this.#scheduleCannon(); return;
      }
      void (async () => {
        // Native frame zero is a logical restart; the last image remains until frame one.
        if (this.#cannonWrapped) { await this.engine.nextFlic(cannon.decoderId); this.#cannonWrapped = false; }
        const frame = await this.engine.nextFlic(cannon.decoderId);
        if (this.#cannon !== cannon) return;
        this.renderer.animatedSpriteFrame('dice-cannon', frame); this.#scheduleCannon();
      })().catch(error => this.inform(`骰子动画读取失败：${String(error)}`));
    }, this.#cannonAnimating ? 101 : 5001);
  }

  async #stopCannon(): Promise<void> {
    this.#cannonGeneration++;
    clearTimeout(this.#cannonTimer); this.#cannonTimer = 0;
    const cannon = this.#cannon; this.#cannon = undefined;
    this.renderer.removeAnimatedSprite('dice-cannon');
    if (cannon) await this.engine.closeFlic(cannon.decoderId);
  }

  async #playDie(rolled: number, generation: number): Promise<void> {
    if (!Number.isInteger(rolled) || rolled < 1 || rolled > 8) throw new Error('原版骰子点数无效');
    this.#busy = true;
    await this.#stopCannon();
    const die = await this.engine.openFlic(await this.assets.bytes(`install/ani/pm06a${rolled + 14}.flc`));
    try {
      if (generation !== this.#generation) return;
      this.renderer.setAnimatedSprite('dice-result', die, { x: 270, y: 48 }, 50);
      // 0x414cb0 explicitly advances one frame for each DirectDraw flip.
      for (let frame = 1; frame < die.frameCount; frame++) {
        await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
        if (generation !== this.#generation) return;
        this.renderer.animatedSpriteFrame('dice-result', await this.engine.nextFlic(die.decoderId));
      }
    } finally {
      this.renderer.removeAnimatedSprite('dice-result');
      await this.engine.closeFlic(die.decoderId);
    }
  }

  #buildUi(snapshot: Snapshot): void {
    this.ui.replaceChildren();
    const values = [['money', 101], ['intelligence', 360], ['morality', 452], ['charm', 544], ['stamina', 636]] as const;
    for (const [key, right] of values) {
      const value = String(snapshot.stats[key] ?? '—');
      this.ui.append(textAt(value, right - value.length * 9, 5, 'hud-number'));
    }
    this.ui.append(textAt(snapshot.player?.name ?? '', 581, 159, 'hud-centered'));
    this.ui.append(textAt(`${snapshot.stats.age ?? ''}岁`, 575, 216, 'hud-centered'));
    const labels = ['参数一览', '道具一览', '地图浏览', '系统操作'];
    for (const [index, y] of [247, 286, 325, 364].entries()) {
      const control = controlAt(labels[index]!, 544, y, 52, 30, () => {
        if (index === 0) void this.#command({ type: 'openStatus' });
        if (index === 1) void this.#command({ type: 'openInventory' });
        if (index === 2) void this.#showMapOverview(snapshot);
        if (index === 3) this.optionsMenu();
      });
      const resourceId = `install/bmp/pm06a${String(index + 9).padStart(2, '0')}.bmp`;
      control.onpointerenter = control.onfocus = () => this.renderer.setSpriteFrame(resourceId, 1, 3);
      control.onpointerleave = control.onblur = () => this.renderer.setSpriteFrame(resourceId, 0, 3);
      control.onpointerdown = () => this.renderer.setSpriteFrame(resourceId, 2, 3);
      this.ui.append(control);
    }
    const roll = controlAt('掷骰子', 549, 402, 73, 68, () => {
      if (snapshot.spirits?.active === 0 && !snapshot.life?.traveling) void this.#showDiceChoices(snapshot);
      else void this.#command({ type: 'rollDice' });
    });
    roll.disabled = snapshot.phase !== 'playing' || !!snapshot.pending;
    this.ui.append(roll);
    if (snapshot.pending) this.#pending(snapshot);
  }

  async #showDiceChoices(snapshot: Snapshot): Promise<void> {
    if (this.#busy || this.#diceChoices) return;
    this.#busy = true;
    try {
      const images = Array.from({ length: 8 }, (_, index) => ({ resourceId: `install/bmp/zhan0${index + 1}.bmp`, x: 120 + index * 51, y: 407, width: 51, height: 63, frames: 3, transparentPaletteIndex: 1 }));
      await this.renderer.setOverlays([...this.#hud(snapshot), ...images], this.assets);
      this.#diceChoices = snapshot;
      const roll = this.ui.querySelector<HTMLButtonElement>('[aria-label="掷骰子"]'); if (roll) roll.disabled = true;
      for (const [index, image] of images.entries()) {
        const control = controlAt(`选择 ${index + 1} 点`, image.x, image.y, image.width, image.height, () => { void this.#chooseDie(index + 1); });
        control.onpointerenter = control.onfocus = () => this.renderer.setSpriteFrame(image.resourceId, 1, 3);
        control.onpointerleave = control.onblur = () => this.renderer.setSpriteFrame(image.resourceId, 0, 3);
        control.onpointerdown = () => this.renderer.setSpriteFrame(image.resourceId, 2, 3);
        this.ui.append(control);
      }
      this.inform('点击原版数字按钮或按 1–8 选择骰子点数。');
    } catch (error) { this.inform(String(error)); }
    finally { this.#busy = false; }
  }

  async #showMapOverview(snapshot: Snapshot): Promise<void> {
    if (this.#busy || this.#mapOverview.active) return;
    this.#busy = true; const camera = { ...this.renderer.camera };
    try {
      await this.#stopCue(); await this.#stopCannon(); this.#spirits.hide(); this.#decorations.hide();
      if (this.#actor) await this.engine.closeFlic(this.#actor.decoderId);
      this.#actor = undefined; this.#actorResource = ''; this.renderer.clearActor();
      await this.#mapOverview.show(snapshot, camera);
    } catch (error) { this.inform(String(error)); }
    finally { this.#busy = false; }
  }

  async #chooseDie(value: number): Promise<void> {
    const snapshot = this.#diceChoices;
    if (!snapshot || this.#busy) return;
    this.#diceChoices = undefined;
    await this.renderer.setOverlays(this.#hud(snapshot), this.assets);
    this.ui.querySelectorAll<HTMLButtonElement>('[aria-label^="选择 "]').forEach(button => button.remove());
    await this.#command({ type: 'rollDice', value });
  }

  #pending(snapshot: Snapshot): void {
    const pending = snapshot.pending!;
    if (pending.kind === 'minigame') return;
    if (pending.kind === 'allocation') {
      const panel = document.createElement('form'); panel.className = 'allocation-panel';
      const title = document.createElement('h2'); title.textContent = pending.text ?? '分配成长点数';
      const summary = document.createElement('p'); summary.textContent = `可分配点数：${pending.points ?? 0}`;
      panel.append(title, summary);
      const fields = [['intelligence', '智力'], ['morality', '道德'], ['charm', '魅力'], ['stamina', '体能']] as const;
      for (const [key, labelText] of fields) {
        const label = document.createElement('label'); label.textContent = labelText;
        const input = document.createElement('input'); input.name = key; input.type = 'number'; input.min = '0'; input.step = '1'; input.max = String(pending.points ?? 0); input.value = String(pending.defaults?.[key] ?? 0); input.required = true;
        label.append(input); panel.append(label);
      }
      const confirm = document.createElement('button'); confirm.type = 'submit'; confirm.textContent = '确定'; panel.append(confirm);
      panel.onsubmit = event => {
        event.preventDefault(); const data = new FormData(panel);
        const allocations = { intelligence: Number(data.get('intelligence')), morality: Number(data.get('morality')), charm: Number(data.get('charm')), stamina: Number(data.get('stamina')) };
        void this.#command({ type: 'allocateStats', allocations });
      };
      this.ui.append(panel);
      return;
    }
    if (pending.kind === 'blocked') {
      const panel = document.createElement('div'); panel.className = 'event-blocked'; panel.setAttribute('role', 'status');
      const paragraph = document.createElement('p'); paragraph.textContent = pending.text ?? '此处的原版规则仍需核实。';
      const save = document.createElement('button'); save.textContent = '保存进度'; save.onclick = this.saveMenu;
      panel.append(paragraph, save); this.ui.append(panel); return;
    }
    const text = document.createElement('p'); text.className = pending.kind === 'choice' ? 'event-text event-choice-prompt' : 'event-text'; text.textContent = pending.text ?? '';
    this.ui.append(text);
    if (pending.kind === 'dialogue') {
      const next = controlAt('继续对话', 166, 304, 308, 112, () => { void this.#command({ type: 'advanceEvent' }); });
      this.ui.append(next);
    } else {
      const choices = document.createElement('div'); choices.className = 'event-choices';
      for (const option of pending.options ?? []) {
        const control = document.createElement('button'); control.textContent = option.text;
        control.disabled = option.disabled ?? false;
        control.onclick = () => { void this.#command({ type: 'chooseOption', index: option.index }); };
        choices.append(control);
      }
      this.ui.append(choices);
    }
  }

  async #command(command: Command): Promise<void> {
    if (this.#busy) return;
    this.#busy = true;
    try {
      const result = await this.send(command);
      if (!result.ok) this.inform(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
    } catch (error) { this.inform(error instanceof Error ? error.message : String(error)); }
    finally { this.#busy = false; }
  }

  async #setMusic(snapshot: Snapshot): Promise<void> {
    const path = this.assets.rules.stages.find(stage => stage.id === snapshot.life?.stage)?.musicPath;
    if (!path) return;
    if (path === this.#musicPath) {
      if (this.#musicEnabled && !this.ui.hidden) void this.#music.play().catch(() => {});
      return;
    }
    this.#musicPath = path;
    const resource = this.assets.resolveLegacy(path);
    const bytes = await this.assets.bytes(resource);
    if (this.#musicPath !== path) return;
    if (this.#musicUrl) URL.revokeObjectURL(this.#musicUrl);
    this.#musicUrl = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }));
    this.#music.src = this.#musicUrl;
    if (this.#musicEnabled && !this.ui.hidden) void this.#music.play().catch(() => this.inform('点按游戏选项启用声音。'));
  }
}
