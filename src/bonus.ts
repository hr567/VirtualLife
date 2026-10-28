import { gameAudio } from './audio';
import type { Assets } from './assets';
import type { EngineClient } from './engine-client';
import type { GameRenderer } from './renderer';
import type { BonusState, DecodedBitmap, OpenedFlic, Point } from './types';

/** The original shared statistic-counter artwork; RNG and displayed digits come from Rust. */
export class BonusView {
  #kind = '';
  #generation = 0;
  #description: DecodedBitmap | undefined;
  #digits: DecodedBitmap | undefined;
  #animation: OpenedFlic | undefined;
  #frame = 0;
  #elapsed = 0;
  #visible = false;
  #position: Point = { x: 0, y: 0 };
  #digitIds = new Set<string>();
  #increment: number | undefined;
  #sound: HTMLAudioElement | undefined;
  #soundEnabled = true;

  constructor(private renderer: GameRenderer, private assets: Assets, private engine: EngineClient) {}

  setSound(enabled: boolean): void { this.#soundEnabled = enabled; if (!enabled) this.#sound?.pause(); }

  async hide(): Promise<void> {
    this.#generation++; this.#kind = ''; this.#visible = false; this.#increment = undefined;
    for (const id of ['bonus-description', 'bonus-animation', ...this.#digitIds]) this.renderer.removeAnimatedSprite(id);
    this.#digitIds.clear();
    const animation = this.#animation; this.#animation = undefined;
    this.#sound?.pause();
    if (animation) await this.engine.closeFlic(animation.decoderId);
  }

  async show(kind: 'ham' | 'alie' | 'targ' | 'bear' | 'fire' | 'fly', state: BonusState): Promise<void> {
    if (kind !== this.#kind) {
      await this.hide(); this.#kind = kind;
      const generation = this.#generation;
      const path = `install/public/bonus/${kind}`;
      const [description, digits, bytes] = await Promise.all([this.assets.bitmap(`${path}desc.bmp`, 1), this.assets.bitmap(`${path}fig.bmp`, 1), this.assets.bytes(`${path}bns.flc`)]);
      const animation = await this.engine.openFlic(bytes, 1);
      if (generation !== this.#generation) { await this.engine.closeFlic(animation.decoderId); return; }
      this.#description = description; this.#digits = digits; this.#animation = animation;
      this.#position = kind === 'ham' || kind === 'fire' ? { x: 191, y: 142 } : kind === 'targ' ? { x: 213, y: 125 } : kind === 'fly' ? { x: 133, y: 96 } : { x: 173, y: 96 };
      this.#frame = 0; this.#elapsed = 0;
    }
    if (!state.visible || !this.#animation || !this.#description || !this.#digits) return;
    if (!this.#visible) {
      this.renderer.setAnimatedSprite('bonus-description', this.#description, this.#position, 100);
      this.renderer.setAnimatedSprite('bonus-animation', this.#animation, { x: this.#position.x + 51, y: this.#position.y + 62 }, 101);
      this.#visible = true;
    }
    this.#number('value', state.value, state.valueUnits, state.digitCount, 7);
    this.#number('increment', state.increment, state.incrementUnits, state.digitCount, 70);
    if (this.#increment !== undefined && this.#increment !== state.increment) void this.#playSound();
    this.#increment = state.increment;
  }

  #number(row: string, value: number, units: number | null, width: number, y: number): void {
    const digits = String(Math.abs(value)).split('');
    if (units !== null) digits[digits.length - 1] = String(units);
    for (let index = 0; index < width; index++) {
      const id = `bonus-${row}-${index}`;
      const digit = digits[index - width + digits.length];
      if (digit === undefined) { this.renderer.removeAnimatedSprite(id); this.#digitIds.delete(id); continue; }
      const position = { x: this.#position.x + 157 + index * 28, y: this.#position.y + y };
      if (!this.#digitIds.has(id)) { this.renderer.setAnimatedSprite(id, this.#digits!, position, 102); this.#digitIds.add(id); }
      this.renderer.animatedSpriteSource(id, { x: Number(digit) * 26, y: 0, width: 26, height: 38 });
    }
  }

  async tick(elapsed: number): Promise<void> {
    const animation = this.#animation;
    if (!this.#visible || !animation || this.#frame >= animation.frameCount - 1) return;
    this.#elapsed += elapsed;
    if (this.#elapsed <= 40) return;
    this.#elapsed = 0;
    const frame = await this.engine.nextFlic(animation.decoderId);
    if (this.#animation !== animation) return;
    this.#frame++;
    this.renderer.animatedSpriteFrame('bonus-animation', frame);
  }

  async #playSound(): Promise<void> {
    if (!this.#soundEnabled || !this.#visible || document.visibilityState === 'hidden') return;
    if (!this.#sound) {
      const bytes = await this.assets.bytes('install/public/bonus/bonus.wav');
      this.#sound = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' })));
    }
    if (!this.#soundEnabled || !this.#visible) return;
    this.#sound.currentTime = 0; void this.#sound.play().catch(() => {});
  }
}
