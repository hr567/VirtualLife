import type { AssetManifest, AssetObject, DecodedBitmap, EngineContent, MapCatalog, RulesCatalog } from './types';
import type { EngineClient } from './engine-client';

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
  constructor(readonly manifest: AssetManifest, readonly maps: MapCatalog, readonly rules: RulesCatalog, readonly events: unknown, readonly questionnaires: unknown, private engine: EngineClient) {
    this.#objects = new Map(manifest.objects.map(object => [object.sha256, object]));
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
    const object = this.object(resourceId);
    const response = await fetchAsset(object.path);
    if (!response.ok) throw new Error(`资源读取失败（${response.status}）`);
    const bytes = await response.arrayBuffer();
    const digest = await crypto.subtle.digest('SHA-256', bytes);
    const hash = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
    if (hash !== object.sha256) throw new Error('资源校验失败，请刷新后重试');
    return bytes;
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
