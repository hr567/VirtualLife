import type { Assets } from './assets';
import type { BattleState, EndingState, GameMap, InventoryState, Snapshot, StatusPanel } from './types';

/** Map artwork is kept compressed; decoding and GPU uploads still happen on demand. */
export function mapResources(map: GameMap): string[] {
  const resources = new Set(map.backgroundTiles.map(tile => tile.resourceId));
  const directory = map.sourceResource.slice(0, map.sourceResource.lastIndexOf('/') + 1);
  for (const [name, fields] of Object.entries(map.legacySections)) {
    if (/^(Spirit|Floater)_\d+$/.test(name) && typeof fields.File === 'string') {
      resources.add(directory + fields.File.replaceAll('\\', '/').toLowerCase());
    }
  }
  return [...resources];
}

/** Ordered by the next likely use; no plan expands the entire disc into memory. */
export function resourcePlan(assets: Assets, snapshot?: Snapshot): string[] {
  const resources = new Set<string>();
  const add = (path: string | undefined | null): void => {
    if (!path) return;
    try { resources.add(assets.resolveLegacy(path)); }
    catch { /* Catalog text and optional legacy references are not downloadable objects. */ }
  };
  const collect = (value: unknown): void => {
    if (typeof value === 'string') { if (/[\\/]/.test(value)) add(value); }
    else if (Array.isArray(value)) value.forEach(collect);
    else if (value && typeof value === 'object') Object.values(value).forEach(collect);
  };
  const group = (prefix: string): void => {
    for (const resource of Object.keys(assets.manifest.resources)) if (resource.startsWith(prefix)) add(resource);
  };
  const map = (id: string | null | undefined): void => {
    const selected = assets.maps.maps.find(candidate => candidate.id === id);
    if (selected) mapResources(selected).forEach(add);
  };
  const rules = assets.rules, pending = snapshot?.pending;

  // Creating a profile can be the first click, before a map finishes downloading.
  add('install/bmp/pm0301.bmp');
  // A command can expose the next scene before the dice and walking finish.
  collect(snapshot?.scene);
  collect(pending?.state);
  if (pending?.kind === 'minigame') {
    const directories: Record<string, string> = { tracking: 'gbs', hammer: 'hammer', fist: 'fistgame', gas: 'gas', fire: 'fire', firebee: 'firebee', spiritSlots: 'spirit' };
    const directory = directories[pending.game ?? ''];
    if (directory) group(`install/${directory}/`);
    group('install/public/bonus/');
    group('install/sound/gb/');
  } else if (pending?.kind === 'battle' && pending.game === 'ordinary' && rules.battle) {
    const state = pending.state as BattleState, r = rules.battle.rendering;
    if (state) {
      const { sex, stage, career } = state.input;
      const portrait = stage < 2 ? 25 : stage < 4 ? 26 : stage === 4 ? 27 : career < 0 ? 28 : career;
      collect([r.backgrounds[state.background], r.backgroundOverlays[state.background], r.versusDoors,
        r.versusEnemyPortraits[state.enemy], r.versusPlayerPortraits[sex]?.[portrait],
        r.actors[sex]?.[state.playerClass], r.enemyActors[state.enemy], r.panels, r.cards, r.selectedCards,
        r.timeDigits, r.healthBar, r.hpDigits, r.attackButton, r.words, r.bonus, r.sounds]);
    }
  } else if (pending?.kind === 'battle' && pending.game === 'final' && rules.finalBattle) {
    // The introduction/shop gives the large opening movie time to arrive.
    collect([rules.finalBattle.openingVideo, rules.finalBattle.openingAudio]);
    collect(rules.finalBattle);
  } else if (pending?.kind === 'ending' && rules.epilogue) {
    const state = pending.state as EndingState, epilogue = rules.epilogue;
    collect(rules.endingSummary?.music);
    collect([epilogue.movie, epilogue.outro, epilogue.narration[Number(state?.victory)], epilogue.music,
      epilogue.credits, epilogue.choice]);
  } else if (pending?.kind === 'competition') {
    collect(rules.competition);
  } else if (pending?.kind === 'relationship') {
    group('install/pm0l/');
    add('install/public/button/exita.flc');
    add('sound/music/n2love.wav');
    collect(rules.relationships);
  } else if (pending?.kind === 'inventory' && rules.inventory) {
    const state = pending.state as InventoryState;
    collect(rules.inventory.rendering);
    if (state) for (const entry of state.entries) collect(rules.inventory[state.tab][entry.id]?.animation);
  } else if (pending?.kind === 'status' && rules.statusRendering) {
    const state = pending.state as StatusPanel;
    if (state?.page === 'house') {
      const housing = pending.housing as { style: number } | undefined;
      collect(rules.statusRendering.house.styles.find(style => style.style === housing?.style));
      add(rules.statusRendering.house.sound);
    } else if (state?.page === 'investment') collect(rules.statusRendering.investment);
    else if (state?.page === 'skills' || state?.page === 'careerHistory') collect(rules.statusRendering[state.page]);
  }

  map(snapshot?.mapId ?? 'park');
  // LegacyPlayer::initialize starts a new life at stage 2 (the park schoolchild).
  const stage = snapshot?.life?.stage ?? 2;
  const currentStage = rules.stages.find(candidate => candidate.id === stage);
  add(currentStage?.musicPath);
  const character = (snapshot?.player?.sex === 'female' ? 31 : 0) + stage +
    ((snapshot?.life?.profession ?? -1) === -1 ? 0 : snapshot!.life!.profession);
  const directory = rules.characters.directories[character];
  if (directory) {
    const model = directory.replace(/\/$/, '').split('/').at(-1)!;
    for (const suffix of rules.characters.animationSuffixes) add(directory + suffix.replace(/^M00/i, model));
  }
  add(rules.characters.portraits[character]);
  if (!snapshot?.player) {
    const femaleDirectory = rules.characters.directories[31 + stage];
    if (femaleDirectory) {
      const model = femaleDirectory.replace(/\/$/, '').split('/').at(-1)!;
      for (const suffix of rules.characters.animationSuffixes) add(femaleDirectory + suffix.replace(/^M00/i, model));
    }
    add(rules.characters.portraits[31 + stage]);
  }
  if (snapshot?.spirits && stage !== 2 && !snapshot.life?.traveling) {
    const age = [1, 1, 1, 2, 2, 4, 3][stage];
    if (age !== undefined) for (const spirit of ['xr', 'dl', 'sa', 'fd', 'yb']) {
      for (const direction of ['3a', '3b', '3c', '3d']) add(`spirit/${age}${spirit}${direction}.flc`);
    }
  }

  // Shared controls and all dice outcomes are small and used throughout a life.
  for (let index = 1; index <= 5; index++) add(`install/bmp/pm020${index}.bmp`);
  add('install/bmp/pm0301.bmp');
  group('install/bmp/pm06a');
  group('install/ani/pm06a');
  for (let index = 1; index <= 8; index++) add(`install/bmp/zhan0${index}.bmp`);
  ['install/bmp/frame.bmp', 'install/bmp/talk.bmp', 'install/ani/3.flc'].forEach(add);
  collect(rules.inventory?.rendering);
  collect(rules.optionsRendering);
  const status = rules.statusRendering;
  if (status) {
    const portrait = snapshot?.statusOverview?.portraitIndex ?? character;
    collect([status.background, status.portraits[portrait], status.hpLabel, status.hpFrame, status.hpFill,
      status.hpGdi.variants[status.hpGdi.portraitVariants[portrait] ?? 0], status.back, status.buttons, status.disabledButtons]);
  }

  // Graduation can enter study, unemployment or adult life; university goes
  // straight to adult life. The unemployment stage has no timed transition.
  const nextStages: Record<number, number[]> = { 0: [1], 1: [2], 2: [3], 3: [4, 5, 6], 4: [6] };
  if (currentStage && snapshot?.life && currentStage.stageTurns > 0 && currentStage.stageTurns - snapshot.life.stageTurn <= 5) {
    for (const id of nextStages[stage] ?? []) {
      const next = rules.stages.find(candidate => candidate.id === id);
      if (!next) continue;
      const nextMap = assets.maps.maps.find(candidate => candidate.sourceResource === `cd/data/${next.mapPath}.mmi`);
      if (nextMap) mapResources(nextMap).forEach(add);
      add(next.musicPath);
      // A new adult career is chosen later, so prepare its fallback character
      // rather than downloading every possible profession's animation set.
      const index = (snapshot.player?.sex === 'female' ? 31 : 0) + id;
      const nextDirectory = rules.characters.directories[index];
      if (nextDirectory) {
        const model = nextDirectory.replace(/\/$/, '').split('/').at(-1)!;
        for (const suffix of rules.characters.animationSuffixes) add(nextDirectory + suffix.replace(/^M00/i, model));
      }
      add(rules.characters.portraits[index]);
    }
  }

  // Any of the six small games can be selected by an event. Their compressed art
  // costs far less than the large battle/film catalogs and prevents mid-game stalls.
  for (const directory of ['gbs', 'hammer', 'fistgame', 'gas', 'fire', 'firebee', 'public/bonus']) group(`install/${directory}/`);
  group('install/sound/gb/');
  return [...resources];
}
