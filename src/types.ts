export interface Point { x: number; y: number }
export interface Cell { row: number; column: number }
export interface MapChip extends Cell { eventId: number; next: Cell[]; backgroundCells: Cell[] }
export interface MapBackground extends Cell { x: number; y: number; width: number; height: number; resourceId: string }
export interface GameMap {
  id: string;
  dimensions: { width: number; height: number };
  chipOrigin: Point;
  chipSize: { width: number; height: number };
  start: Cell;
  chips: MapChip[];
  backgroundTiles: MapBackground[];
  sourceResource: string;
  legacySections: Record<string, Record<string, string | { signed?: number[]; bytes?: number[] }>>;
}
export interface MapCatalog { formatVersion: number; maps: GameMap[] }
export interface EngineContent extends MapCatalog { rules: RulesCatalog; events: unknown; questionnaires: unknown }
export interface RulesCatalog {
  formatVersion: number;
  stages: { id: number; mapPath: string; musicPath: string; stageTurns: number }[];
  characters: { directories: string[]; animationSuffixes: string[]; portraits: string[] };
  relationships?: { playerPortraits: string[][]; answerPrefixes: string[] };
  competition?: { menu: string[][]; beautyOpponents: string[][]; runningOpponents: string[][]; openingAnimation: CompetitionAnimation; rendering: CompetitionRendering; running: { rendering: RunningRendering } };
  endingSummary?: EndingSummaryRendering;
  epilogue?: EpilogueRendering;
  battle?: BattleRules;
  finalBattle?: FinalBattleRules;
  inventory?: InventoryRules;
  statusRendering?: StatusRendering;
  mapOverview?: { maps: { mapId: string; special: boolean; index: number; columns: number; rows: number; tileWidth: number; tileHeight: number; tiles: import('./native-rendering').NativeImage[] }[]; markers: import('./native-rendering').NativeFrames[] };
  optionsRendering?: OptionsRendering;
}
export interface OptionsRendering {
  background: import('./native-rendering').NativeImage; back: import('./native-rendering').NativeFrames[];
  buttons: (import('./native-rendering').NativeImage & { action: 'volume' | 'save' | 'load' | 'exit' })[];
  volume: { background: import('./native-rendering').NativeImage; back: import('./native-rendering').NativeFrames[]; track: import('./native-rendering').NativeImage; thumb: import('./native-rendering').NativeImage };
  exit: { background: import('./native-rendering').NativeImage; back: import('./native-rendering').NativeFrames[]; yes: import('./native-rendering').NativeImage; no: import('./native-rendering').NativeImage };
}
export type StatusPage = 'overview' | 'house' | 'investment' | 'careerHistory' | 'skills';
export interface NativeTransition { kind: 'rain' | 'mosaic'; unitSize: number; steps: number; finished: boolean; columns: { y: number; finished: boolean }[] }
export interface StatusPanel { phase?: never; page: StatusPage; scroll: number; clockMs: number; backTracks: { frame: number }[]; transition?: NativeTransition }
export interface StatusOverview { fields: Record<string, string | number>; portraitIndex: number; hpWidth: number; hpText: string; missionCount: number; missionRank: string; mission: string; hasMission: boolean; canInspectHouse: boolean; canInspectInvestment: boolean }
export interface StatusListRendering {
  background: import('./native-rendering').NativeImage; scrollThumb: import('./native-rendering').NativeImage;
  scrollButtons: import('./native-rendering').NativeImage[]; back: import('./native-rendering').NativeFrames[];
  tabs?: import('./native-rendering').NativeImage[]; scrollRect: number[]; scrollStep?: number; scrollTravel?: number;
  visibleRows: number; maxScroll?: number; columns: number[]; widths?: number[]; rowY: number; rowHeight: number;
  nameX?: number; nameWidth?: number; columnWidth?: number; firstLineY?: number; secondLineY?: number;
}
export interface StatusRendering {
  background: import('./native-rendering').NativeImage; portraits: import('./native-rendering').NativeImage[];
  hpLabel: import('./native-rendering').NativeImage; hpFrame: import('./native-rendering').NativeImage; hpFill: import('./native-rendering').NativeImage;
  back: import('./native-rendering').NativeFrames[]; buttons: import('./native-rendering').NativeImage[]; disabledButtons: import('./native-rendering').NativeImage[];
  texts: { field: string; x: number; y: number; color: number; width: number }[];
  hpText: { x: number; y: number; shadowX: number; shadowY: number }; missionRank: { valueX: number; valueY: number; textX: number; textY: number }; mission: { x: number; y: number; emptyY: number };
  skills: StatusListRendering; careerHistory: StatusListRendering;
  house: { sound: string; styles: { style: number; layers: import('./native-rendering').NativeImage[] }[] };
  investment: { sound: string; background: import('./native-rendering').NativeImage; stages: (import('./native-rendering').NativeImage & { investmentStage: number })[] };
}
export type InventoryTab = 'ordinary' | 'special' | 'equipment';
export interface InventoryItem { id: number; name: string; animation: string; frameCount: number; frameDurationMs: number; descriptions: string[]; healingPercent?: number }
export interface InventoryRules {
  ordinary: InventoryItem[]; special: InventoryItem[]; equipment: InventoryItem[];
  rendering: { backgrounds: import('./native-rendering').NativeImage[]; rows: import('./native-rendering').NativeImage; buttons: import('./native-rendering').NativeImage[]; equipmentButtons: import('./native-rendering').NativeImage[]; hpLabel: import('./native-rendering').NativeImage; hpFrame: import('./native-rendering').NativeImage; hpFill: import('./native-rendering').NativeImage; confirmation: import('./native-rendering').NativeImage; confirmYes: import('./native-rendering').NativeImage; confirmNo: import('./native-rendering').NativeImage; back: import('./native-rendering').NativeFrames[]; emptyEquipmentLabel: string; asciiFont: string; chineseFont: string };
}
export interface InventoryState { phase?: never; tab: InventoryTab; selectedIndex: number | null; confirmDrop: boolean; clockMs: number; entries: { id: number; slot?: number; equipped?: boolean; frame: number }[]; backTracks?: { frame: number }[]; previewOrigin?: { x: number; y: number } }
export interface FinalResource { resource: string; assetPath: string; kind: string; width?: number; height?: number; frameCount?: number; frameDurationMs?: number }
export interface FinalBattleRules {
  careerHpMultipliers: number[]; careerPower: number[]; cardPositions: number[][]; selectedCardPositions: number[][];
  shopCardPositions: number[][]; heldShopPositions: number[][]; heldBattlePositions: number[][];
  itemPrices: number[]; itemDescriptions: string[]; introTexts: string[]; shopTexts: Record<string, string>;
  valuationLabels: string[]; valuationLabelPositions: number[][]; valuationValuePositions: number[][];
  battleIndexLabels: string[]; totalValuationLabel: string; careerNames: string[]; unemployedCareerName: string;
  playerAnimations: FinalResource[]; enemyAnimations: FinalResource[]; introAnimations: FinalResource[];
  recoveryAnimation: FinalResource; shieldAnimations: FinalResource[]; animations: { enemyLeft: FinalResource[]; enemyRight: FinalResource[] };
  openingVideo: string; openingAudio: string; openingDurationMs: number; resourcesByAddress: Record<string, FinalResource>;
  runtimeObjects?: Record<string, FinalResource & { x: number; y: number; objectType: string; transparentIndex?: number | null; firstPixelTransparent?: boolean }>;
}
export interface FinalBattleState {
  layers?: { resource?: string; assetPath?: string; frame: number; x: number; y: number; transparentIndex?: number | null; firstPixelTransparent?: boolean }[];
  phase: 'intro' | 'shop' | 'opening' | 'playing' | 'finished'; clockMs: number; introStep: number; introStartedMs: number; introDurationMs: number;
  introDisplayStep?: number; introDisplayedMessage?: string; shopDisplayedPanelY?: number; shopDisplayedItemCount?: number; shopVisibleItems?: number[];
  valuation: number[]; money: number; hp: number; enemyHp: number; power: number; items: number[];
  cards: { kind: number; attack: number; defense: number; selected: boolean; x: number }[]; cardsMoving: boolean;
  selectedItem?: number | null; activeItem: number; itemUsed: boolean; selectionChanged: boolean;
  attack: number; defense: number; enemyDefense: number; shield: number; round: number; combatState: number; enemyAnimation: number; playerAnimation: number;
  displayedHp: number; displayedEnemyHp: number; hpDelta: number; enemyHpDelta: number; hpTextY: number; enemyTextY: number;
  enemyTracks: { frame: number }[]; playerTracks: { frame: number }[]; introTracks: { frame: number }[]; recoveryTrack: { frame: number }; shieldTracks: { frame: number }[];
  shopAnimation?: { item: number; phase: number; x: number; y: number; originalX: number; originalY: number } | null;
  shopPanelY: number; shopMessage: string; itemScroll: number; shopSelectedBuy?: number | null; shopSelectedSell?: number | null;
  openingStartedMs: number; attackSerial: number; soundSerial: number; sounds: string[]; soundStops?: string[]; finished: boolean;
}
export interface BattleRules {
  cardPositions: number[][]; enemyNames: string[]; enemyDescriptions: string[]; rendering: BattleRendering;
}
export interface BattleRendering {
  versusDoors: import('./native-rendering').NativeImage[]; versusEnemyPortraits: import('./native-rendering').NativeImage[]; versusPlayerPortraits: import('./native-rendering').NativeImage[][]; classNames: string[][];
  actors: import('./native-rendering').NativeFrames[][][]; enemyActors: import('./native-rendering').NativeFrames[][];
  playerStrongEffects: import('./native-rendering').NativeFrames[][]; enemyStrongEffects: import('./native-rendering').NativeFrames[][];
  elementEffects: import('./native-rendering').NativeFrames[]; cardCombo: import('./native-rendering').NativeFrames[];
  godEffects: import('./native-rendering').NativeFrames[]; playerCardFlips: import('./native-rendering').NativeFrames[]; enemyCardFlips: import('./native-rendering').NativeFrames[];
  healthFlares: import('./native-rendering').NativeFrames[]; shields: import('./native-rendering').NativeFrames[]; effectOverlays: import('./native-rendering').NativeFrames[];
  backgrounds: import('./native-rendering').NativeImage[]; backgroundOverlays: import('./native-rendering').NativeImage[];
  panels: import('./native-rendering').NativeImage[]; selectedCardPositions: number[][]; comboCardPositions: number[][][];
  timeDigits: import('./native-rendering').NativeImage; cardDescriptions: string[][]; cardAttackLabel: string; cardDefenseLabel: string; damagePrefix: string; damageSuffix: string;
  bonus: { money: import('./native-rendering').NativeImage; experience: import('./native-rendering').NativeImage; digits: import('./native-rendering').NativeImage; gain: import('./native-rendering').NativeFrames; loss: import('./native-rendering').NativeFrames };
  cards: import('./native-rendering').NativeImage[]; selectedCards: import('./native-rendering').NativeImage[];
  attackButton: import('./native-rendering').NativeImage; words: import('./native-rendering').NativeImage[]; healthBar: import('./native-rendering').NativeImage; cross: import('./native-rendering').NativeImage;
  hpDigits: import('./native-rendering').NativeImage[]; enemyPortraits: import('./native-rendering').NativeImage[][]; careerPortraits: import('./native-rendering').NativeImage[][][]; studentPortraits: import('./native-rendering').NativeImage[][][];
  sounds: { music: string; closeDoor: string; cardSelect: string[]; cardCombo: string[]; cooldownReady: string; shield: string; card: string; showCards: string; enemyDamage: string[]; enemyAttack: string[][]; enemyDefence: string[]; playerAttack: string[]; playerVoices: string[][]; playerCareerVoice: string[][] };
}
export interface BattleCard { kind: number; attack: number; defense: number }
export interface BattleState {
  phase: 'opening' | 'intro' | 'playing' | 'result' | 'reward' | 'finished'; clockMs: number; phaseSince: number; openingLeftX: number; openingRightX: number; showOpening: boolean;
  input: { sex: number; stage: number; career: number; lifeLevel: number }; enemy: number; background: number; playerClass: number; persistent: { enemies: { rank: number }[] };
  hp: number; maxHp: number; enemyHp: number; enemyMaxHp: number; displayedHp: number; displayedEnemyHp: number;
  playerDamage: number; enemyDamage: number; playerDamageFrames: number; enemyDamageFrames: number; shownPlayerDamage: boolean; shownEnemyDamage: boolean; hoveredCard?: number | null; paletteResource: string;
  cardCapacity: number; cards: BattleCard[]; cardCount: number; selected: boolean[]; cardX: number[]; dealing: boolean;
  playerCooldown: number; enemyCooldown: number; playerTenths: number; enemyTenths: number; playerSeconds: number; enemySeconds: number;
  remaining: number; attackPower: number; defensePower: number; attackPhase: number; playerAttacking: boolean; enemyAttacking: boolean;
  playerActor: number; enemyActor: number; enemyEffect: number; comboCount: number; attackKind: number; reflection: number; strongAttack: boolean; strongEnemy: boolean; godEffect: number;
  usedCards: BattleCard[]; usedSelected: boolean[]; playerPortraitTimers: number[]; enemyPortraitTimers: number[]; lossY: number;
  won: boolean; lost: boolean; resultStarted: boolean; text: string; tracks: Record<string, import('./native-rendering').NativeFrames>; layers: import('./native-rendering').NativeFrames[]; sounds: string[]; soundStops: string[];
  bonus?: BonusState | null; bonusKind: number; help: boolean; helpPage: number; helpSince: number;
}
export interface EndingSummaryRendering {
  textX: number; initialY: number; clip: [number, number, number, number];
  backgrounds: string[][]; portraits: string[][]; actors: { resource: string; frameCount: number; interval: number }[];
  music: string; fontSize: number; lineSpacing: number; lineHeight: number; byteWrap: number; characterSpacing: number;
}
export interface EpilogueRendering {
  subtitles: { text: string; segment: number; durationMs: number }[][];
  segments: { startMs: number; endMs: number; color: string }[];
  font: string; fontSize: number; characterSpacing: number; subtitleRect: [number, number, number, number];
  movie: string; outro: string; narration: string[]; music: string;
  video: { x: number; y: number; width: number; height: number; sourceHeight: number; scanlines: boolean };
  credits: { paragraphs: { text: string; y: number; height: number }[]; rect: [number, number, number, number]; font: string; fontSize: number; characterSpacing: number; color: string; end: { resource: string; width: number; height: number } };
  choice: { background: { resource: string; width: number; height: number }; buttons: { resource: string; width: number; height: number; x: number; y: number }[]; cursor: { resource: string; width: number; height: number } };
}
export interface EndingMovie {
  phase: 'subtitles' | 'outro' | 'finished'; victory: boolean; clockMs: number;
  subtitleIndex: number; displayedSubtitle: number | null; segment: number; videoTimeMs: number; displayedVideoTimeMs: number;
  narrationStarted: boolean; narrationStartedMs: number; seekSerial: number;
}
export interface EndingCredits { clockMs: number; offset: number; brightness: number; finished: boolean }
export interface EndingSummaryState {
  finished: boolean; clockMs: number; sex: number; career: number; victory: boolean;
  background: string; portrait: string; actor: { resource: string; frame: number };
  paragraphs: { text: string; bytes: number[]; x: number; y: number; height: number }[];
  scrollY: number; actorY: number; showText: boolean; showActor: boolean; scrolling: boolean;
  palette: number[][]; originalPalette: number[][];
}
export interface EndingState {
  victory: boolean; phase: 'summary' | 'movie' | 'credits' | 'endImage' | 'reincarnation' | 'finished';
  summary?: EndingSummaryState; movie?: EndingMovie; credits?: EndingCredits;
}
export interface AssetObject {
  sha256: string;
  path: string;
  bytes: number;
  media?: { kind: string; width?: number; height?: number; bitsPerPixel?: number };
}
export interface AssetManifest {
  formatVersion: number;
  objects: AssetObject[];
  resources: Record<string, { sha256: string }>;
}
export interface Profile { name: string; sex: 'male' | 'female'; birthMonth: number; birthDay: number; bloodType: 'A' | 'B' | 'O' | 'AB' }
export interface BonusState {
  value: number;
  increment: number;
  digitCount: number;
  valueUnits: number | null;
  incrementUnits: number | null;
  visible: boolean;
  finished: boolean;
}
export interface TrackingState {
  phase: 'reveal' | 'memorize' | 'shuffle' | 'settle' | 'choose' | 'roundResult' | 'reward' | 'finished';
  round: number;
  correct: number;
  outcomes: boolean[];
  objects: { x: number; y: number; sprite: number; verticalDirection: number; settled: boolean; destination: number }[];
  revealedTarget: number;
  target: number;
  selected: number | null;
  phaseElapsedMs: number;
  idleElapsedMs: number;
  idleAnimation: [number, number] | null;
  exitDirection: number;
  exitVerticalDirection: number;
  reward?: BonusState | null;
}
export interface HammerState {
  phase: 'ready' | 'playing' | 'won' | 'lost' | 'reward' | 'finished';
  targets: { x: number; y: number; kind: number; phase: number; hits: number; frame: number; lastTick: number; spriteY: number; impactUntil: number | null; impactPosition: [number, number]; scoreFrame: number | null; scoreLastTick: number }[];
  score: number;
  clockMs: number;
  startedAt: number;
  resultAt: number;
  remainingSeconds: number;
  lossFrame: number;
  lossLastTick: number;
  pointer: [number, number];
  reward?: BonusState | null;
}
export interface FistState {
  phase: 'ready' | 'playing' | 'help' | 'resultPause' | 'won' | 'lost' | 'lostShake' | 'lostWait' | 'reward' | 'finished';
  playerX: number; playerState: number; strikeFrame: number;
  targetX: number; targetState: number; targetFrame: number;
  score: number; remainingSeconds: number; winFrame: number;
  lossY: number; lossShakeFrame: number;
  left: boolean; right: boolean; fire: boolean;
  reward?: BonusState | null;
}
export interface NativeAnimation { resource: string; x: number; y: number; frame: number; frameCount: number }
export interface GasState {
  phase: 'ready' | 'playing' | 'help' | 'hit' | 'miss' | 'won' | 'wonWait' | 'lost' | 'lostFall' | 'lostShake' | 'reward' | 'finished';
  clockMs: number; phaseLastTick: number;
  attempts: number; successes: number; results: boolean[];
  targetState: number; targets: NativeAnimation[];
  playerState: number; animations: NativeAnimation[];
  fallY: number; shakeFrame: number;
  reward?: BonusState | null;
}
export interface FireState {
  phase: 'ready' | 'playing' | 'finished'; clockMs: number; failed: boolean;
  duration: number; elapsed: number; wonAtElapsed: number; fadeAt: number; gauge: number;
  hand: boolean; bodyState: number; bodyFrame: number; sparksState: number; sparksFrame: number;
  actorX: number; actorY: number; actorType: number; winStage: number;
  lightningActive: boolean; lightningFrame: number; lightningSound?: number; fireFlash: boolean; shakeActive: boolean; shakeOffset: number;
  helpState: number; helpX: number; helpY: number;
  intro: boolean; randomOverlay: boolean; lossAnimation: boolean; lossBanner: boolean; lossFrame: number;
  winBanner: boolean; rewardStarted: boolean; tracks: { frame: number }[];
  reward?: BonusState | null;
}
export interface FirebeeState {
  phase: 'ready' | 'playing' | 'help' | 'reward' | 'finished'; clockMs: number;
  playerState: number; playerX: number; failed: boolean; score: number;
  left: boolean; right: boolean; fire: boolean;
  drawnEnemies: { row: number; x: number; y: number; frame: number }[];
  drawnEnemyShots: [number, number][]; drawnPlayerShots: [number, number][];
  drawnExplosions: { x: number; y: number; frame: number }[]; death: { x: number; y: number; frame: number };
  sounds: number[]; reward?: BonusState | null;
}
export interface CompetitionAnimation extends Point { resource: string; frameCount: number; interval: number; frame?: number }
interface CompetitionBitmap extends Point { resource: string; transparent?: boolean }
interface CompetitionTextArea extends Point { width: number; height: number; color: string; fontSize: number }
export interface CompetitionRendering {
  background: CompetitionBitmap; menuHost: CompetitionBitmap; hostAnimation: CompetitionAnimation;
  talkBackground: CompetitionBitmap; talkText: CompetitionTextArea; menuTitle: CompetitionBitmap; menuText: CompetitionTextArea;
  menuButtons: (CompetitionBitmap & { width: number; height: number })[]; introBackgrounds: string[][];
  scorePanelResource: string; scorePanelPositions: [number, number][]; scoreNamePositions: [number, number][]; scoreValuePositions: [number, number][];
  scoreLabels: string[];
  portraitPositions: [number, number][]; beautyPlayerPortraits: string[][][]; beautyOtherPortraits: string[][];
  judgeScorePositions: [number, number][]; podiumNamePositions: [number, number][];
  quizText: CompetitionTextArea; quizScoreBackground: CompetitionBitmap; quizScoreResources: string[];
  quizScorePosition: { x: number; baseY: number; stepY: number }; quizSelection: CompetitionAnimation; music: string;
  sounds: string[]; quizSounds: string[];
}
export interface RunningRendering {
  background: string; playerPortraits: string[][][]; otherPortraits: string[][]; scoreLabels: string[];
  scorePauseText: string; sounds: Record<string, string>;
}
export interface RunningState {
  phase: 'preparing' | 'running' | 'finishing' | 'resultPause' | 'podium' | 'finished';
  contestant: number; times: number[]; place: number; prize: number; podiumPlace: number; winners: number[];
  displayedContestant: number; displayedTimes: number[]; showScorePanels: boolean; clearedFrame: boolean;
  layers: CompetitionAnimation[]; sounds: string[];
}
export interface CompetitionState {
  phase: 'arrival' | 'opening' | 'menu' | 'intro' | 'beauty' | 'running' | 'quizIntro' | 'quizQuestion' | 'quizFeedback' | 'quizResult' | 'rankPrize' | 'moneyPrize' | 'finished';
  clockMs: number; stage: number; sex: number; name: string; selected: number | null; introPage: number; place: number; prize: number;
  animation?: CompetitionAnimation | null; feedback: CompetitionAnimation[]; lastCorrect: boolean;
  exam?: { current: { question: string; options: string[] }; answered: number; score: number } | null;
  answeredQuestion?: { question: string; options: string[] } | null;
  beauty?: { phase: string; contestant: number; scores: number[][]; totals: number[]; winners: number[]; podiumPlace: number; layers: CompetitionAnimation[] } | null;
  running?: RunningState | null;
}
export interface RelationshipPerson {
  id: number; playerSex: number; cost: number; charmRequired: number; romanceRequired: number;
  selectedPortrait: string; portrait: string; introduction: string;
}
export interface RelationshipHouse {
  phase: 'main' | 'meet' | 'question' | 'notice' | 'dates' | 'dateConfirm' | 'marriages' | 'marriageConfirm' | 'finished';
  state: { sex: number; stage: number; money: number; charm: number; romance: number; contacts: number[]; spouse: number; giftLevel: number };
  people: RelationshipPerson[];
  offers: number[];
  marriageCandidates: number[];
  hoveredOffer: number | null;
  selectedPerson: number | null;
  questionIndex: number;
  attempts: number;
  message: string;
  noticeKind: number;
  clockMs: number;
}
export interface SpiritLottery {
  phase: 'spinning' | 'stopping' | 'result' | 'finished';
  spirit: number;
  positiveMoney: boolean;
  digits: { value: number }[];
  attribute: { value: number };
}
export interface Snapshot {
  mapFacing?: number;
  mapDecorations?: { mapId: string; loadSerial: number; delaysMs: number[] };
  schemaVersion: 1;
  revision: number;
  lastSeq: number;
  phase: 'ready' | 'exploring' | 'playing' | 'event' | 'blocked' | 'completed';
  mapId: string | null;
  tileId: number | null;
  turn: number;
  player: Profile | null;
  stats: Record<string, number | null>;
  blockers: string[];
  message: string;
  selectedTile: MapChip | null;
  life?: { stage: number; stageTurn: number; year: number; lifeLevel: number; profession: number; professionLevel: number; affection: number; traveling?: boolean };
  statusOverview?: StatusOverview;
  statusDetails?: { skills: { name: string; columns: { first: string; second?: string; color: number }[] }[]; careerHistory: { year: number; name: string; description: string }[] };
  spirits?: { positions: Cell[]; active: number | null; expiresTurn: number };
  pending?: { kind: 'dialogue' | 'choice' | 'blocked' | 'allocation' | 'minigame' | 'relationship' | 'competition' | 'ending' | 'battle' | 'inventory' | 'status'; text?: string; options?: { index: number; text: string; disabled?: boolean }[]; points?: number; money?: number; defaults?: Record<string, number>; game?: string; state?: TrackingState | HammerState | FistState | GasState | FireState | FirebeeState | RelationshipHouse | SpiritLottery | CompetitionState | EndingState | BattleState | FinalBattleState | InventoryState | StatusPanel; question?: { prompt: string; answers: string[] }; [key: string]: unknown } | null;
  scene?: { patternResource?: string | null; patternScrollMode?: number; backgroundResource?: string | null; leftPortraitResource?: string | null; rightPortraitResource?: string | null; leftPortraitId?: number; rightPortraitId?: number };
}
export type Command =
  | { type: 'exploreMap'; mapId: string }
  | { type: 'inspectTile'; tileId: number }
  | { type: 'newGame'; profile: Profile; seed: number }
  | { type: 'rollDice'; value?: number }
  | { type: 'advanceEvent' }
  | { type: 'chooseOption'; index: number }
  | { type: 'minigameTick'; elapsedMs: number }
  | { type: 'minigameAction'; action: 'startShuffle' | 'choose' | 'nextRound' | 'finishIdleAnimation' | 'start' | 'strike' | 'strikeIndex' | 'finishRewardAnimation' | 'stop' | 'input' | 'help' | 'throw' | 'left' | 'right' | 'releaseLeft' | 'releaseRight'; index?: number; x?: number; y?: number; left?: boolean; right?: boolean; fire?: boolean }
  | { type: 'relationshipTick'; elapsedMs: number; hoverIndex?: number }
  | { type: 'competitionTick'; elapsedMs: number }
  | { type: 'endingTick'; elapsedMs: number }
  | { type: 'battleTick'; elapsedMs: number }
  | { type: 'openInventory'; tab?: InventoryTab }
  | { type: 'openStatus' }
  | { type: 'statusTick'; elapsedMs: number }
  | { type: 'statusAction'; action: 'page' | 'scroll' | 'close'; page?: StatusPage; delta?: number; index?: number }
  | { type: 'inventoryTick'; elapsedMs: number }
  | { type: 'inventoryAction'; action: 'tab' | 'select' | 'use' | 'wear' | 'unequip' | 'drop' | 'confirmDrop' | 'cancelDrop' | 'close'; index?: number; tab?: InventoryTab }
  | { type: 'battleAction'; action: 'start' | 'selectCard' | 'attack' | 'showHelp' | 'skipReward' | 'hoverCard' | 'continue' | 'buy' | 'sell' | 'inspectBuy' | 'inspectSell' | 'cancelShop' | 'selectItem' | 'useItem' | 'scrollLeft' | 'scrollRight'; index?: number }
  | { type: 'endingAction'; action: 'escape' | 'sDown' | 'sUp' | 'continue' | 'choose'; index?: number }
  | { type: 'competitionAction'; action: 'choose' | 'continue' | 'answer' | 'left' | 'right'; index?: number }
  | { type: 'relationshipAction'; action: 'meet' | 'dates' | 'marriages' | 'selectOffer' | 'answer' | 'advanceNotice' | 'selectDate' | 'selectMarriage' | 'reject' | 'confirm' | 'close'; index?: number }
  | { type: 'allocateStats'; allocations: { intelligence: number; morality: number; charm: number; stamina: number } };
export interface EngineResult {
  ok: boolean;
  snapshot: Snapshot;
  events: unknown[];
  error?: { code: string; message: string } | string;
}
export interface DecodedBitmap { width: number; height: number; pixels: Uint8Array<ArrayBuffer>; paletteIndices?: Uint8Array<ArrayBuffer> }
export interface OpenedFlic extends DecodedBitmap { decoderId: number; frameCount: number; frameDelayMs: number }
export interface WorkerRequest { id: number; type: 'init' | 'dispatch' | 'snapshot' | 'serialize' | 'restore' | 'decodeBmp' | 'openFlic' | 'nextFlic' | 'closeFlic'; payload?: unknown }
export interface WorkerReply { id: number; ok: boolean; value?: unknown; error?: string }
