import type { AssetObject } from './types';

const OBJECT_CACHE = 'virtual-life-disc-v1';
const MEMORY_BYTES = 32 * 1024 * 1024;

/** Immutable, verified disc bytes. Callers receive copies because the engine transfers buffers. */
export class AssetCache {
  readonly #memory = new Map<string, ArrayBuffer>();
  readonly #pending = new Map<string, Promise<ArrayBuffer>>();
  readonly #reload = new Set<string>();
  #memoryBytes = 0;
  #storage: Promise<Cache | undefined> | undefined;
  #canWrite = true;
  private url: (path: string) => string;

  constructor(url: (path: string) => string) { this.url = url; }

  async bytes(object: AssetObject): Promise<ArrayBuffer> {
    return (await this.#read(object, false)).slice(0);
  }

  async warm(object: AssetObject): Promise<void> {
    await this.#read(object, true);
  }

  #read(object: AssetObject, background: boolean): Promise<ArrayBuffer> {
    const cached = this.#memory.get(object.sha256);
    if (cached) {
      this.#memory.delete(object.sha256);
      this.#memory.set(object.sha256, cached);
      return Promise.resolve(cached);
    }
    const existing = this.#pending.get(object.sha256);
    if (existing) return existing;
    const loading = this.#load(object, background).then(bytes => {
      if (bytes.byteLength <= MEMORY_BYTES) {
        while (this.#memoryBytes + bytes.byteLength > MEMORY_BYTES) {
          const oldest = this.#memory.keys().next().value!;
          this.#memoryBytes -= this.#memory.get(oldest)!.byteLength;
          this.#memory.delete(oldest);
        }
        this.#memory.set(object.sha256, bytes);
        this.#memoryBytes += bytes.byteLength;
      }
      return bytes;
    }).finally(() => { this.#pending.delete(object.sha256); });
    this.#pending.set(object.sha256, loading);
    return loading;
  }

  #cache(): Promise<Cache | undefined> {
    return this.#storage ??= Promise.resolve().then(() => caches.open(OBJECT_CACHE)).catch(() => undefined);
  }

  async #valid(bytes: ArrayBuffer, object: AssetObject): Promise<boolean> {
    if (bytes.byteLength !== object.bytes) return false;
    const digest = await crypto.subtle.digest('SHA-256', bytes);
    return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('') === object.sha256;
  }

  async #load(object: AssetObject, background: boolean): Promise<ArrayBuffer> {
    const url = new URL(this.url(object.path), location.href).href;
    const cache = await this.#cache();
    let reload = this.#reload.has(object.sha256);
    try {
      const cached = await cache?.match(url, { ignoreVary: true });
      if (cached) {
        const bytes = await cached.arrayBuffer();
        if (await this.#valid(bytes, object)) return bytes;
        reload = true;
        this.#reload.add(object.sha256);
        await cache?.delete(url, { ignoreVary: true });
      }
    } catch { reload = true; /* Cache failure must not prevent online play. */ }

    let response: Response;
    let bytes: ArrayBuffer;
    try {
      response = await fetch(url, {
        cache: reload ? 'reload' : 'default', priority: background ? 'low' : 'high',
        signal: AbortSignal.timeout(120_000),
      });
      if (!response.ok) throw new Error(`资源读取失败（${response.status}）`);
      bytes = await response.arrayBuffer();
    } catch (error) {
      if (error instanceof Error && error.message.startsWith('资源读取失败')) throw error;
      throw new Error('资源下载失败，请检查网络连接后重试。');
    }
    if (!await this.#valid(bytes, object)) {
      this.#reload.add(object.sha256);
      throw new Error('资源校验失败，请刷新后重试');
    }
    this.#reload.delete(object.sha256);
    if (cache && this.#canWrite) {
      try {
        await cache.put(url, new Response(bytes, { headers: { 'Content-Type': response.headers.get('Content-Type') ?? 'application/octet-stream' } }));
      } catch { this.#canWrite = false; /* Quota pressure never affects saves or online reads. */ }
    }
    return bytes;
  }
}
