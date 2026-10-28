import type { Assets } from './assets';
import type { GameRenderer } from './renderer';
import type { Command, EngineResult, SpiritLottery } from './types';

/** The native spirit lottery panel, composited over the current map. */
export class SpiritSlotsView {
  #active = false;
  #generation = 0;
  #state: SpiritLottery | undefined;
  #raf = 0;
  #lastTime = 0;
  #step: Promise<void> | undefined;
  #stopping = false;
  #button: HTMLButtonElement | undefined;
  #status: HTMLElement | undefined;

  constructor(private renderer: GameRenderer, private assets: Assets, private ui: HTMLElement, private send: (command: Command) => Promise<EngineResult>, private inform: (message: string) => void) {
    document.addEventListener('visibilitychange', () => {
      this.#lastTime = 0;
      if (document.visibilityState === 'hidden') { cancelAnimationFrame(this.#raf); this.#raf = 0; }
      else this.#schedule();
    });
    document.addEventListener('keydown', event => {
      if (this.#active && this.#state?.phase === 'spinning' && !document.querySelector('dialog[open]') && ['Enter', ' ', 'Escape'].includes(event.key)) {
        event.preventDefault(); this.#stop();
      }
    });
  }

  get active(): boolean { return this.#active; }
  hide(): void {
    this.#active = false; this.#generation++; cancelAnimationFrame(this.#raf); this.#raf = 0; this.#lastTime = 0;
    for (const id of ['spirit-panel', 'spirit-attribute', ...[0, 1, 2, 3].map(index => `spirit-digit-${index}`)]) this.renderer.removeAnimatedSprite(id);
    this.ui.classList.remove('spirit-slots-active');
  }

  async show(state: SpiritLottery): Promise<void> {
    this.#state = state;
    if (!this.#active) {
      this.#active = true; const generation = ++this.#generation;
      const panel = state.spirit === 2 ? state.positiveMoney ? 'l1' : 'l2' : state.spirit === 3 ? 'l4' : 'l3';
      const [background, digits, attribute] = await Promise.all([panel, 'l5', 'l6'].map(name => this.assets.bitmap(this.assets.resolveLegacy(`spirit/${name}.bmp`), 1)));
      if (!this.#active || generation !== this.#generation) return;
      this.renderer.setAnimatedSprite('spirit-panel', background!, { x: 191, y: 141 }, 100);
      if (state.spirit !== 2) this.renderer.setAnimatedSprite('spirit-attribute', attribute!, { x: 216, y: 186 }, 101);
      for (let index = state.spirit === 2 ? 0 : 1; index < 4; index++) this.renderer.setAnimatedSprite(`spirit-digit-${index}`, digits!, { x: 303 + index * 32, y: 185 }, 101);
      this.ui.hidden = false; this.ui.classList.add('spirit-slots-active');
      const button = document.createElement('button'); button.className = 'game-hitbox spirit-stop'; button.setAttribute('aria-label', '停止神灵转盘'); button.onclick = () => this.#stop();
      const status = document.createElement('span'); status.className = 'sr-only spirit-result'; status.setAttribute('role', 'status');
      this.#button = button; this.#status = status; this.ui.append(button, status);
      this.inform('点击画面或按回车、空格停止神灵转盘。');
    }
    if (!this.#active) return;
    for (let index = state.spirit === 2 ? 0 : 1; index < 4; index++) this.renderer.animatedSpriteSource(`spirit-digit-${index}`, { x: 0, y: state.digits[index]!.value * 21, width: 11, height: 21 });
    if (state.spirit !== 2) this.renderer.animatedSpriteSource('spirit-attribute', { x: 0, y: state.attribute.value * 21, width: 49, height: 21 });
    if (this.#button) this.#button.disabled = state.phase !== 'spinning';
    if (this.#status && state.phase === 'result') this.#status.textContent = `${state.spirit === 2 ? '金钱' : ['智力', '道德', '魅力', '体能'][state.attribute.value]} ${state.digits.slice(state.spirit === 2 ? 0 : 1).map(digit => digit.value).join('')}`;
    this.#schedule();
  }

  #stop(): void {
    if (!this.#active || this.#stopping || this.#state?.phase !== 'spinning') return;
    this.#stopping = true; cancelAnimationFrame(this.#raf); this.#raf = 0;
    void (async () => {
      await this.#step;
      if (!this.#active) return;
      const result = await this.send({ type: 'minigameAction', action: 'stop' });
      if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
    })().catch(error => this.inform(String(error))).finally(() => { this.#stopping = false; this.#schedule(); });
  }

  #schedule(): void {
    if (!this.#active || this.#raf || this.#step || this.#stopping || document.visibilityState === 'hidden') return;
    this.#raf = requestAnimationFrame(now => {
      this.#raf = 0;
      if (document.querySelector('dialog[open]')) { this.#lastTime = 0; this.#schedule(); return; }
      const elapsedMs = this.#lastTime ? Math.max(1, Math.round(now - this.#lastTime)) : 16; this.#lastTime = now;
      const generation = this.#generation;
      this.#step = this.send({ type: 'minigameTick', elapsedMs }).then(result => {
        if (!result.ok) throw new Error(typeof result.error === 'string' ? result.error : result.error?.message ?? result.snapshot.message);
      }).catch(error => {
        if (this.#active && generation === this.#generation) { this.inform(`神灵转盘暂停：${String(error)}`); this.hide(); }
      }).finally(() => { this.#step = undefined; this.#schedule(); });
    });
  }
}
