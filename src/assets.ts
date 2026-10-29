import type { AssetManifest, AssetObject, DecodedBitmap, EngineContent, MapCatalog, RulesCatalog } from './types';
import type { EngineClient } from './engine-client';
import { AssetCache } from './asset-cache';

export const assetUrl = (path: string): string => `${import.meta.env.BASE_URL}${path.replace(/^\/+/, '')}`;

async function fetchAsset(path: string): Promise<Response> {
  try { return await fetch(assetUrl(path)); }
  catch { throw new Error('资源下载失败，请检查网络连接后重试。'); }
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetchAsset(path);
  if (!response.ok) throw new Error(`资源目录加载失败（${response.status}）`);
  return response.json() as Promise<T>;
}

export class Assets {
  readonly #objects: Map<string, AssetObject>;
  readonly #uiBitmaps = new Map<string, Promise<DecodedBitmap>>();
  readonly #cache = new AssetCache(assetUrl);
  readonly #prefetched = new Set<string>();
  readonly #warming = new Set<string>();
  #queue: AssetObject[] = [];
  #foreground = 0;
  #prefetchTimer: ReturnType<typeof setTimeout> | undefined;
  constructor(readonly manifest: AssetManifest, readonly maps: MapCatalog, readonly rules: RulesCatalog, readonly events: unknown, readonly questionnaires: unknown, private engine: EngineClient) {
    this.#objects = new Map(manifest.objects.map(object => [object.sha256, object]));
    document.addEventListener('visibilitychange', () => this.#schedulePrefetch());
    window.addEventListener('online', () => this.#schedulePrefetch());
    const connection = (navigator as Navigator & { connection?: EventTarget }).connection;
    connection?.addEventListener('change', () => this.#schedulePrefetch());
  }

  static async load(engine: EngineClient): Promise<Assets> {
    const [manifest, maps, rules, events, questionnaires] = await Promise.all([
      getJson<AssetManifest>('assets/manifest.json'),
      getJson<MapCatalog>('assets/catalog/maps.json'),
      getJson<RulesCatalog>('assets/catalog/rules.json'),
      getJson<unknown>('assets/catalog/events.json'),
      getJson<unknown>('assets/catalog/questionnaires.json'),
    ]);
    if (manifest.formatVersion !== 1 || maps.formatVersion !== 1) throw new Error('不支持的资源目录版本');
    return new Assets(manifest, maps, rules, events, questionnaires, engine);
  }

  get engineContent(): EngineContent { return { ...this.maps, rules: this.rules, events: this.events, questionnaires: this.questionnaires }; }

  object(resourceId: string): AssetObject {
    const resource = this.manifest.resources[resourceId];
    const object = resource && this.#objects.get(resource.sha256);
    if (!object) throw new Error(`光盘资源不存在：${resourceId}`);
    return object;
  }

  async bytes(resourceId: string): Promise<ArrayBuffer> {
    this.#foreground++;
    try { return await this.#cache.bytes(this.object(resourceId)); }
    finally { this.#foreground--; this.#schedulePrefetch(); }
  }

  /** Replace obsolete queued work. Already started downloads remain shared with foreground reads. */
  prefetch(resourceIds: string[]): void {
    const unique = new Map<string, AssetObject>();
    for (const resource of resourceIds) {
      const object = this.object(resource);
      if (!this.#prefetched.has(object.sha256) && !this.#warming.has(object.sha256)) unique.set(object.sha256, object);
    }
    this.#queue = [...unique.values()];
    this.#schedulePrefetch();
  }

  #schedulePrefetch(): void {
    if (this.#prefetchTimer !== undefined || !this.#queue.length) return;
    this.#prefetchTimer = setTimeout(() => {
      this.#prefetchTimer = undefined;
      const connection = (navigator as Navigator & { connection?: { saveData?: boolean; effectiveType?: string } }).connection;
      if (this.#foreground || document.hidden || !navigator.onLine || connection?.saveData || ['slow-2g', '2g'].includes(connection?.effectiveType ?? '')) return;
      while (this.#warming.size < 2 && this.#queue.length) {
        const object = this.#queue.shift()!;
        this.#warming.add(object.sha256);
        void this.#cache.warm(object).then(() => { this.#prefetched.add(object.sha256); }).catch(() => {
          // A future scene plan or foreground read may retry; no automatic offline retry loop.
        }).finally(() => { this.#warming.delete(object.sha256); this.#schedulePrefetch(); });
      }
    }, 64);
  }

  async bitmap(resourceId: string, transparentPaletteIndex?: number): Promise<DecodedBitmap> {
    const key = `${resourceId}:${transparentPaletteIndex ?? 'opaque'}`;
    const cached = this.#uiBitmaps.get(key);
    if (cached) return cached;
    const loading = this.bytes(resourceId).then(bytes => this.engine.decodeBmp(bytes, transparentPaletteIndex));
    if (resourceId.startsWith('install/bmp/') || resourceId.startsWith('install/face/')) {
      if (this.#uiBitmaps.size >= 48) this.#uiBitmaps.delete(this.#uiBitmaps.keys().next().value!);
      this.#uiBitmaps.set(key, loading);
      void loading.catch(() => this.#uiBitmaps.delete(key));
    }
    return loading;
  }
  resolveLegacy(relativePath: string): string {
    const path = relativePath.replaceAll('\\', '/').toLowerCase().replace(/^\.\//, '');
    for (const candidate of [path, `install/${path}`, `cd/data/${path}`, `cd/${path}`]) if (this.manifest.resources[candidate]) return candidate;
    throw new Error(`光盘资源不存在：${relativePath}`);
  }
}
