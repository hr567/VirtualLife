import type { Command, DecodedBitmap, EngineContent, EngineResult, OpenedFlic, Snapshot, WorkerReply, WorkerRequest } from './types';

export class EngineClient {
  readonly worker = new Worker(new URL('./engine.worker.ts', import.meta.url), { type: 'module' });
  #nextId = 0;
  #seq = 0;
  #pending = new Map<number, { resolve: (value: unknown) => void; reject: (reason: Error) => void }>();
  #commands: Promise<unknown> = Promise.resolve();

  constructor() {
    this.worker.onmessage = ({ data }: MessageEvent<WorkerReply>) => {
      const pending = this.#pending.get(data.id);
      if (!pending) return;
      this.#pending.delete(data.id);
      if (data.ok) pending.resolve(data.value);
      else pending.reject(new Error(data.error ?? '游戏引擎发生错误'));
    };
    this.worker.onerror = (event) => {
      for (const pending of this.#pending.values()) pending.reject(new Error(event.message || '游戏引擎加载失败'));
      this.#pending.clear();
    };
  }

  #request<T>(type: WorkerRequest['type'], payload?: unknown, transfer: Transferable[] = []): Promise<T> {
    const id = ++this.#nextId;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve: value => resolve(value as T), reject });
      this.worker.postMessage({ id, type, payload } satisfies WorkerRequest, transfer);
    });
  }

  async init(content: EngineContent): Promise<Snapshot> {
    const snapshot = await this.#request<Snapshot>('init', JSON.stringify(content));
    this.#seq = snapshot.lastSeq;
    return snapshot;
  }

  dispatch(command: Command): Promise<EngineResult> {
    const next = this.#commands.then(async () => {
      const result = await this.#request<EngineResult>('dispatch', { ...command, seq: this.#seq + 1 });
      this.#seq = result.snapshot.lastSeq;
      return result;
    });
    this.#commands = next.catch(() => {});
    return next;
  }

  async serialize(): Promise<string> { await this.#commands; return this.#request('serialize'); }
  async restore(save: string): Promise<EngineResult> {
    await this.#commands;
    const result = await this.#request<EngineResult>('restore', save);
    this.#seq = result.snapshot.lastSeq;
    return result;
  }
  decodeBmp(bytes: ArrayBuffer, transparentPaletteIndex?: number): Promise<DecodedBitmap> { return this.#request('decodeBmp', { bytes, transparentPaletteIndex }, [bytes]); }
  openFlic(bytes: ArrayBuffer, transparentPaletteIndex?: number): Promise<OpenedFlic> { return this.#request('openFlic', { bytes, transparentPaletteIndex }, [bytes]); }
  nextFlic(decoderId: number): Promise<DecodedBitmap> { return this.#request('nextFlic', decoderId); }
  closeFlic(decoderId: number): Promise<void> { return this.#request('closeFlic', decoderId); }
}
