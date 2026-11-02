/** Browser services for the Bevy client. No game rules or visible game rendering live here. */
const SAVE_DATABASE = 'virtual-life-bevy';
const SAVE_FORMAT = 'virtual-life-bevy-save';
export const SAVE_DATABASE_VERSION = 2;
export const SAVE_SLOTS = Object.freeze(Array.from({ length: 9 }, (_, index) => `slot-${index}`));
const checkSlot = slot => { if (!SAVE_SLOTS.includes(slot)) throw new Error('无效的存档槽。'); };
const recordToken = record => record ? JSON.stringify(record) : null;
const OBJECT_CACHE = 'virtual-life-disc-v1';
const MEMORY_LIMIT = 32 * 1024 * 1024;
const AUDIO_CONTEXTS = Symbol.for('virtual-life-bevy.audio-contexts');

/** Track the contexts created by Bevy/CPAL so a real gesture resumes game audio. */
export function installAudioContextTracking() {
  if (globalThis[AUDIO_CONTEXTS]) return;
  const contexts = new Set();
  globalThis[AUDIO_CONTEXTS] = contexts;
  for (const name of ['AudioContext', 'webkitAudioContext']) {
    const Constructor = globalThis[name];
    if (!Constructor) continue;
    globalThis[name] = new Proxy(Constructor, {
      construct(target, args) {
        const context = Reflect.construct(target, args);
        contexts.add(context);
        context.addEventListener('statechange', () => {
          if (context.state === 'closed') contexts.delete(context);
        });
        return context;
      },
    });
  }
}

const failure = error => error instanceof Error ? error.message : String(error);
const request = operation => new Promise((resolve, reject) => {
  operation.onsuccess = () => resolve(operation.result);
  operation.onerror = () => reject(operation.error ?? new Error('浏览器存储读取失败。'));
});
const transactionDone = transaction => new Promise((resolve, reject) => {
  transaction.oncomplete = () => resolve();
  transaction.onabort = transaction.onerror = () => reject(transaction.error ?? new Error('存档事务失败，原存档已保留。'));
});

export async function sha256(bytes, cryptoApi = globalThis.crypto) {
  const digest = await cryptoApi.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
}

export async function validateSave(record, contentId, cryptoApi = globalThis.crypto) {
  if (!record || record.format !== SAVE_FORMAT || record.version !== 1 ||
      typeof record.payload !== 'string' || typeof record.contentId !== 'string' ||
      typeof record.savedAt !== 'string' || !Number.isFinite(Date.parse(record.savedAt)) ||
      !/^[0-9a-f]{64}$/.test(record.checksum ?? '')) {
    throw new Error('不支持的存档格式，请选择 Bevy 网页版导出的存档。');
  }
  if (contentId && record.contentId !== contentId) throw new Error('此存档对应不同的游戏资源版本。');
  if (await sha256(new TextEncoder().encode(record.payload), cryptoApi) !== record.checksum) {
    throw new Error('存档校验失败，请尝试上一份备份。');
  }
  // The application validates the serialized game state before restoring it.
  return record;
}

export class SaveStorage {
  #writes = Promise.resolve();
  constructor(database) { this.database = database; }

  static async open(indexedDb = globalThis.indexedDB) {
    const opening = indexedDb.open(SAVE_DATABASE, SAVE_DATABASE_VERSION);
    opening.onupgradeneeded = event => {
      const database = opening.result;
      if (!database.objectStoreNames.contains('saves')) database.createObjectStore('saves', { keyPath: 'slot' });
      if (!database.objectStoreNames.contains('settings')) database.createObjectStore('settings');
      if (event.oldVersion === 1) {
        const store = opening.transaction.objectStore('saves');
        for (const [source, destination] of [['current', 'slot-1'], ['previous', 'slot-2']]) {
          const reading = store.get(source);
          reading.onsuccess = () => {
            if (reading.result) store.put({ ...reading.result, slot: destination });
            store.delete(source);
          };
        }
      }
    };
    const database = await request(opening);
    database.onversionchange = () => database.close();
    return new SaveStorage(database);
  }

  async load(slot, contentId) {
    checkSlot(slot);
    const record = await request(this.database.transaction('saves').objectStore('saves').get(slot));
    return record ? validateSave(record, contentId) : null;
  }

  async list(contentId) {
    const store = this.database.transaction('saves').objectStore('saves');
    const records = await Promise.all(SAVE_SLOTS.map(slot => request(store.get(slot))));
    return Promise.all(records.map(async (record, index) => {
      const summary = { slot: SAVE_SLOTS[index], occupied: !!record, valid: false, token: recordToken(record) };
      if (!record) return summary;
      try {
        await validateSave(record, contentId);
        const payload = JSON.parse(record.payload);
        return { ...summary, valid: true, savedAt: record.savedAt, name: payload.state?.player?.name ?? '', turn: payload.state?.turn ?? 0 };
      } catch (error) { return { ...summary, error: failure(error) }; }
    }));
  }

  save(slot, payload, contentId, expected) {
    checkSlot(slot);
    if (typeof payload !== 'string' || typeof contentId !== 'string' || !contentId) {
      return Promise.reject(new Error('存档内容或资源版本无效。'));
    }
    return this.#enqueue(async () => {
      const record = {
        format: SAVE_FORMAT, version: 1, slot,
        savedAt: new Date().toISOString(), contentId, payload,
        checksum: await sha256(new TextEncoder().encode(payload)),
      };
      return this.#put(record, expected);
    });
  }

  import(slot, record, contentId, expected) {
    checkSlot(slot);
    return this.#enqueue(async () => {
      await validateSave(record, contentId);
      const { format, version, savedAt, payload, checksum } = record;
      return this.#put({ slot, format, version, savedAt, payload, checksum, contentId: record.contentId }, expected);
    });
  }

  #enqueue(action) {
    const writing = this.#writes.then(action);
    this.#writes = writing.catch(() => {});
    return writing;
  }

  async #put(record, expected) {
    const transaction = this.database.transaction('saves', 'readwrite');
    const done = transactionDone(transaction);
    const store = transaction.objectStore('saves');
    const latest = store.get(record.slot);
    let writeError;
    latest.onsuccess = () => {
      if (expected !== undefined && recordToken(latest.result) !== expected) {
        writeError = new Error('该槽位已被其他页面修改，请刷新列表后重试。');
        transaction.abort(); return;
      }
      try { store.put(record); }
      catch (error) { writeError = error; transaction.abort(); }
    };
    try { await done; return record; }
    catch (error) {
      const cause = writeError ?? error;
      if (cause?.name === 'QuotaExceededError') throw new Error('存储空间不足，旧存档已保留。请释放空间或导出备份。');
      throw cause;
    }
  }

  async settings() {
    const store = this.database.transaction('settings').objectStore('settings');
    const [keys, values] = await Promise.all([request(store.getAllKeys()), request(store.getAll())]);
    return Object.fromEntries(keys.map((key, index) => [String(key), values[index]]));
  }

  async setting(key, value) {
    if (typeof key !== 'string' || !key) throw new Error('设置名称无效。');
    if (value === undefined) return (await request(this.database.transaction('settings').objectStore('settings').get(key))) ?? null;
    const transaction = this.database.transaction('settings', 'readwrite');
    const done = transactionDone(transaction);
    transaction.objectStore('settings').put(value, key);
    await done;
    return value;
  }
}

/** Verified immutable bytes; every consumer receives its own copy. */
export class VerifiedAssets {
  #memory = new Map();
  #pending = new Map();
  #memoryBytes = 0;
  #cache;
  #writeCache = true;
  #reload = new Set();
  #queue = [];
  #warming = new Set();
  #foreground = 0;
  #timer;

  constructor(manifest, baseUrl, environment = globalThis) {
    this.environment = environment;
    this.baseUrl = baseUrl;
    this.manifest = manifest;
    this.objects = new Map(manifest.objects.map(object => [object.sha256, object]));
    this.byPath = new Map(manifest.objects.map(object => [object.path, object]));
  }

  object(reference) {
    let object;
    if (typeof reference === 'string') reference = { resourceId: reference };
    if (reference?.resourceId) object = this.objects.get(this.manifest.resources[reference.resourceId]?.sha256);
    else if (reference?.sha256) object = this.objects.get(reference.sha256);
    else if (reference?.path) object = this.byPath.get(reference.path);
    if (!object || !/^[0-9a-f]{64}$/.test(object.sha256) || !Number.isSafeInteger(object.bytes) || object.bytes < 1) {
      throw new Error(`资源目录中没有此对象：${reference?.resourceId ?? reference?.sha256 ?? reference?.path ?? ''}`);
    }
    const url = new URL(object.path, this.baseUrl);
    if (url.origin !== new URL(this.baseUrl).origin || !url.href.startsWith(this.baseUrl)) throw new Error('资源路径超出应用目录。');
    return object;
  }

  async bytes(reference) {
    this.#foreground++;
    try { return (await this.#read(this.object(reference), false)).slice(0); }
    finally { this.#foreground--; this.schedule(); }
  }

  prefetch(references) {
    const unique = new Map();
    for (const reference of references) {
      const object = this.object(reference);
      if (!this.#memory.has(object.sha256) && !this.#warming.has(object.sha256)) unique.set(object.sha256, object);
    }
    this.#queue = [...unique.values()];
    this.schedule();
  }

  schedule() {
    if (this.#timer || !this.#queue.length) return;
    this.#timer = this.environment.setTimeout(() => {
      this.#timer = undefined;
      const environment = this.environment;
      const connection = environment.navigator?.connection;
      if (this.#foreground || environment.document?.hidden || environment.navigator?.onLine === false ||
          connection?.saveData || ['slow-2g', '2g'].includes(connection?.effectiveType)) return;
      while (this.#warming.size < 2 && this.#queue.length) {
        const object = this.#queue.shift();
        this.#warming.add(object.sha256);
        void this.#read(object, true).catch(() => {}).finally(() => {
          this.#warming.delete(object.sha256); this.schedule();
        });
      }
    }, 64);
  }

  async #storage() {
    return this.#cache ??= Promise.resolve().then(() => this.environment.caches?.open(OBJECT_CACHE)).catch(() => undefined);
  }

  async #valid(bytes, object) {
    return bytes.byteLength === object.bytes && await sha256(bytes, this.environment.crypto) === object.sha256;
  }

  #read(object, background) {
    const cached = this.#memory.get(object.sha256);
    if (cached) {
      this.#memory.delete(object.sha256); this.#memory.set(object.sha256, cached);
      return Promise.resolve(cached);
    }
    if (this.#pending.has(object.sha256)) return this.#pending.get(object.sha256);
    const loading = this.#load(object, background).then(bytes => {
      if (bytes.byteLength <= MEMORY_LIMIT) {
        while (this.#memoryBytes + bytes.byteLength > MEMORY_LIMIT) {
          const oldest = this.#memory.keys().next().value;
          this.#memoryBytes -= this.#memory.get(oldest).byteLength; this.#memory.delete(oldest);
        }
        this.#memory.set(object.sha256, bytes); this.#memoryBytes += bytes.byteLength;
      }
      return bytes;
    }).finally(() => this.#pending.delete(object.sha256));
    this.#pending.set(object.sha256, loading);
    return loading;
  }

  async #load(object, background) {
    const url = new URL(object.path, this.baseUrl).href;
    const cache = await this.#storage();
    let reload = this.#reload.has(object.sha256);
    try {
      const cached = await cache?.match(url, { ignoreVary: true });
      if (cached) {
        const bytes = await cached.arrayBuffer();
        if (await this.#valid(bytes, object)) return bytes;
        reload = true; this.#reload.add(object.sha256);
        await cache.delete(url, { ignoreVary: true });
      }
    } catch { reload = true; }
    const response = await this.environment.fetch(url, {
      cache: reload ? 'reload' : 'default', priority: background ? 'low' : 'high',
      signal: AbortSignal.timeout(120_000),
    });
    if (!response.ok) throw new Error(`资源读取失败（${response.status}）。`);
    const bytes = await response.arrayBuffer();
    if (!await this.#valid(bytes, object)) {
      this.#reload.add(object.sha256); throw new Error('资源长度或 SHA-256 校验失败，请重新加载。');
    }
    this.#reload.delete(object.sha256);
    if (cache && this.#writeCache) {
      try { await cache.put(url, new Response(bytes, { headers: { 'Content-Type': response.headers.get('Content-Type') ?? 'application/octet-stream' } })); }
      catch { this.#writeCache = false; }
    }
    return bytes;
  }

  dispose() { this.environment.clearTimeout(this.#timer); this.#queue = []; }
}

function mediaEvent(element, type, action) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => finish(new Error('影片解码超时，请重试。')), 30_000);
    const loaded = () => finish();
    const failed = () => finish(new Error(`影片解码失败：${element.error?.message ?? type}`));
    const finish = error => {
      clearTimeout(timeout); element.removeEventListener(type, loaded); element.removeEventListener('error', failed);
      if (error) reject(error); else resolve();
    };
    element.addEventListener(type, loaded, { once: true });
    element.addEventListener('error', failed, { once: true });
    try { action?.(); } catch (error) { finish(error); }
  });
}

class VideoFrames {
  #videos = new Map();
  #generation = 0;
  constructor(assets) { this.assets = assets; }
  async frame({ resourceId, milliseconds = 0, frame, fps = 24, generation = 0 }) {
    if (!(fps > 0) || !Number.isFinite(milliseconds) || milliseconds < 0) throw new Error('影片时间无效。');
    const requestedFrame = frame ?? Math.floor(milliseconds * fps / 1000);
    if (!Number.isSafeInteger(requestedFrame) || requestedFrame < 0) throw new Error('影片帧序号无效。');
    const localGeneration = this.#generation;
    let loading = this.#videos.get(resourceId);
    if (!loading) {
      loading = this.assets.bytes({ resourceId }).then(async bytes => {
        const video = document.createElement('video');
        video.muted = true; video.playsInline = true; video.preload = 'auto'; video.disablePictureInPicture = true;
        const url = URL.createObjectURL(new Blob([bytes], { type: 'video/webm' }));
        try { await mediaEvent(video, 'loadeddata', () => { video.src = url; video.load(); }); }
        catch (error) { URL.revokeObjectURL(url); throw error; }
        return { video, url, chain: Promise.resolve(), canvas: document.createElement('canvas') };
      });
      this.#videos.set(resourceId, loading);
      void loading.catch(() => { if (this.#videos.get(resourceId) === loading) this.#videos.delete(resourceId); });
    }
    const item = await loading;
    const operation = item.chain.then(async () => {
      if (localGeneration !== this.#generation) throw new Error('影片请求已取消。');
      const { video, canvas } = item;
      const time = Math.min(Math.max(0, video.duration - 1 / fps), requestedFrame / fps) + .000001;
      if (Math.abs(video.currentTime - time) >= .00001) await mediaEvent(video, 'seeked', () => { video.currentTime = time; });
      if (localGeneration !== this.#generation) throw new Error('影片请求已取消。');
      canvas.width = video.videoWidth; canvas.height = video.videoHeight;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      if (!context) throw new Error('浏览器无法读取影片帧。');
      context.drawImage(video, 0, 0);
      const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
      return { value: { resourceId, frame: requestedFrame, fps, generation, width: canvas.width, height: canvas.height }, bytes: new Uint8Array(pixels.buffer) };
    });
    item.chain = operation.catch(() => {});
    return operation;
  }
  close() {
    this.#generation++;
    for (const loading of this.#videos.values()) void loading.then(({ video, url }) => {
      video.pause(); video.removeAttribute('src'); video.load(); URL.revokeObjectURL(url);
    }).catch(() => {});
    this.#videos.clear();
  }
}

function chooseSave() {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file'; input.accept = '.json,.vlsave'; input.hidden = true;
    const finish = file => { input.remove(); if (file) resolve(file); else reject(new Error('已取消选择存档。')); };
    input.addEventListener('change', () => finish(input.files?.[0]), { once: true });
    input.addEventListener('cancel', () => finish(), { once: true });
    document.body.append(input); input.click();
  });
}

export function downloadSave(record) {
  const { slot: _slot, ...exported } = record;
  const url = URL.createObjectURL(new Blob([JSON.stringify(exported)], { type: 'application/json' }));
  const anchor = document.createElement('a'); anchor.href = url;
  const slot = record.slot?.replace('slot-', '') ?? '备份';
  anchor.download = `虚拟人生-槽位${slot}-${record.savedAt.replace(/[:.]/g, '-')}.vlsave.json`; anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

export function createPlatform(callback) {
  let storage;
  let assets;
  let videos;
  let baseUrl = new URL('./', document.baseURI).href;
  let ime;
  let composing = false;
  let imeGeneration = 0;
  let audioUnlocked = false;
  let diagnostics = Object.freeze({ loading: true });
  let testApi = false;
  let ariaStatus;
  let disposed = false;
  const cleanups = [];
  const emit = (id, operation, value = null, bytes = new Uint8Array(), error = null) => {
    if (!disposed) callback({ id, operation, value, bytes, error });
  };
  const listen = (target, type, listener) => {
    target?.addEventListener(type, listener); cleanups.push(() => target?.removeEventListener(type, listener));
  };
  const publishDiagnostics = () => {
    const api = { get diagnostics() { return diagnostics; } };
    if (testApi) api.input = value => {
      if (!value || typeof value !== 'object') throw new Error('测试输入无效。');
      const input = {};
      if (typeof value.controlId === 'string') input.controlId = value.controlId;
      else if (typeof value.key === 'string') {
        input.key = value.key;
        if (typeof value.down === 'boolean') input.down = value.down;
      } else if (Number.isFinite(value.frameMs) && value.frameMs >= 0) input.frameMs = value.frameMs;
      else throw new Error('测试只允许控件、按键或时间输入。');
      emit(0, 'testInput', input);
    };
    Object.defineProperty(window, '__virtualLifeBevy', { configurable: true, value: Object.freeze(api) });
  };
  publishDiagnostics();
  const viewportCanvas = document.querySelector('#game-canvas');
  const viewportSize = () => {
    const bounds = viewportCanvas?.getBoundingClientRect();
    if (bounds?.width > 0 && bounds.height > 0) emit(0, 'viewportSize', { width: bounds.width, height: bounds.height });
  };
  if (viewportCanvas) {
    const observer = new ResizeObserver(viewportSize);
    observer.observe(viewportCanvas);
    cleanups.push(() => observer.disconnect());
    listen(window, 'resize', viewportSize);
    viewportSize();
  }
  const visibility = () => {
    emit(0, 'visibility', { hidden: document.hidden, online: navigator.onLine }); assets?.schedule();
  };
  listen(document, 'visibilitychange', visibility);
  listen(window, 'online', visibility); listen(window, 'offline', visibility);
  listen(navigator.connection, 'change', () => assets?.schedule());
  const unlockAudio = async () => {
    const contexts = [...(globalThis[AUDIO_CONTEXTS] ?? [])];
    await Promise.all(contexts.filter(context => context.state === 'suspended').map(context => context.resume()));
    const state = contexts.length && contexts.every(context => context.state === 'running') ? 'running' : 'suspended';
    if (!audioUnlocked || state !== 'running') emit(0, 'audioUnlocked', { state });
    audioUnlocked = state === 'running';
    return { state };
  };
  const gestureUnlock = () => {
    // A previously unlocked context may suspend again (or a new Bevy context
    // may appear). Check the live contexts on every real gesture.
    void unlockAudio().catch(error => emit(0, 'audioUnlocked', null, undefined, failure(error)));
  };
  listen(document, 'pointerdown', gestureUnlock); listen(document, 'keydown', gestureUnlock);

  const operations = {
    async init(payload) {
      testApi = payload.testApi === true && ['localhost', '127.0.0.1', '[::1]'].includes(location.hostname);
      publishDiagnostics();
      if (payload.baseUrl) baseUrl = new URL(payload.baseUrl, document.baseURI).href;
      const paths = ['manifest.json', 'catalog/maps.json', 'catalog/rules.json', 'catalog/events.json', 'catalog/questionnaires.json', 'catalog/glyphs.json', 'catalog/native-ui.json', 'catalog/native-housing.json', 'catalog/native-fortune.json', 'catalog/native-school.json', 'catalog/native-allocation.json', 'catalog/native-profile.json'];
      const catalogs = await Promise.all(paths.map(async path => {
        const response = await fetch(new URL(`assets/${path}`, baseUrl));
        if (!response.ok) throw new Error(`资源目录读取失败（${response.status}）：${path}`);
        return response.json();
      }));
      const [manifest, maps, rules, events, questionnaires, glyphs, nativeUi, nativeHousing, nativeFortune, nativeSchool, nativeAllocation, nativeProfile] = catalogs;
      // One lookup table serves all Bevy scene composition; never scan 5,000
      // manifest objects for every sprite on every frame.
      manifest._objects = Object.fromEntries(manifest.objects.map(object => [object.sha256, object]));
      manifest._nativeUi = nativeUi; manifest._nativeHousing = nativeHousing; manifest._nativeFortune = nativeFortune;
      manifest._nativeSchool = nativeSchool;manifest._nativeAllocation = nativeAllocation;
      manifest._nativeProfile = nativeProfile;
      if (manifest.formatVersion !== 1 || !Array.isArray(manifest.objects) || maps.formatVersion !== 1) throw new Error('不支持的资源目录。');
      assets?.dispose(); videos?.close(); assets = new VerifiedAssets(manifest, baseUrl); videos = new VideoFrames(assets);
      let settings = {}, storageError = null;
      try { storage ??= await SaveStorage.open(); settings = await storage.settings(); }
      catch (error) { storageError = failure(error); }
      return { manifest, maps, rules, events, questionnaires, glyphs, settings, storageError, baseUrl, seed: crypto.getRandomValues(new Uint32Array(1))[0] };
    },
    async scene(payload) {
      // Accessibility and diagnostics reflect Bevy state; this bridge does not
      // create visible controls or keep a second mutable gameplay model.
      diagnostics = Object.freeze(structuredClone(payload));
      const canvas = document.querySelector('canvas');
      if (canvas) {
        if (payload.loading === false && !payload.error) canvas.dataset.ready = 'true';
        if (payload.error && canvas.dataset.ready !== 'true') {
          window.dispatchEvent(new CustomEvent('virtual-life-startup-error', { detail: payload.error }));
        }
        canvas.setAttribute('aria-label', payload.description || '虚拟人生');
        canvas.setAttribute('aria-busy', String(!!payload.loading));
        canvas.setAttribute('aria-description', (payload.controls ?? []).filter(control => control.enabled).map(control => control.label).join('，'));
      }
      if (!ariaStatus) {
        ariaStatus = document.createElement('div'); ariaStatus.id = 'bevy-accessibility-status';
        ariaStatus.setAttribute('role', 'status'); ariaStatus.setAttribute('aria-live', 'polite');
        Object.assign(ariaStatus.style, { position: 'fixed', width: '1px', height: '1px', overflow: 'hidden', clipPath: 'inset(50%)' });
        document.body.append(ariaStatus);
      }
      ariaStatus.textContent = payload.error || payload.description || '';
      return null;
    },
    async asset(payload) {
      if (!assets) throw new Error('资源目录尚未初始化。');
      const object = assets.object(payload);
      return { value: { resourceId: payload.resourceId ?? null, generation: payload.generation, ...object }, bytes: new Uint8Array(await assets.bytes(payload)) };
    },
    async prefetch(payload) {
      if (!assets) throw new Error('资源目录尚未初始化。');
      assets.prefetch(payload.resources ?? payload.resourceIds ?? []); return null;
    },
    async save(payload) { storage ??= await SaveStorage.open(); return storage.save(payload.slot, payload.payload, payload.contentId, payload.expected); },
    async listSaves(payload) { storage ??= await SaveStorage.open(); return storage.list(payload.contentId); },
    async commitImport(payload) { storage ??= await SaveStorage.open(); return storage.import(payload.slot, payload.record, payload.contentId, payload.expected); },
    async load(payload) { storage ??= await SaveStorage.open(); return storage.load(payload.slot, payload.contentId); },
    async settingsGet(payload) { storage ??= await SaveStorage.open(); return payload.key ? storage.setting(payload.key) : storage.settings(); },
    async settingsSet(payload) { storage ??= await SaveStorage.open(); return storage.setting(payload.key, payload.value); },
    async import(payload) {
      checkSlot(payload.slot);
      const file = await chooseSave();
      if (file.size > 8 * 1024 * 1024) throw new Error('存档文件超过 8 MiB 限制。');
      return validateSave(JSON.parse(await file.text()), payload.contentId);
    },
    async export(payload) {
      storage ??= await SaveStorage.open();
      const record = await storage.load(payload.slot, payload.contentId);
      if (!record) throw new Error('还没有可以导出的存档。');
      downloadSave(record); return { savedAt: record.savedAt };
    },
    async ime(payload) {
      imeGeneration = payload.generation ?? imeGeneration + 1;
      if (payload.active === false) { ime?.blur(); ime?.remove(); ime = undefined; return null; }
      if (!ime) {
        ime = document.createElement('textarea');
        ime.setAttribute('aria-label', payload.label ?? '角色姓名');
        Object.assign(ime.style, { position: 'fixed', left: '0', bottom: '0', width: '1px', height: '1px', opacity: '0.01', zIndex: '-1' });
        ime.autocomplete = 'off'; ime.spellcheck = false;
        ime.addEventListener('compositionstart', () => { composing = true; });
        ime.addEventListener('compositionend', () => { composing = false; emit(0, 'ime', { text: ime.value, generation: imeGeneration, composing: false }); });
        ime.addEventListener('input', () => { if (!composing) emit(0, 'ime', { text: ime.value, generation: imeGeneration, composing: false }); });
        ime.addEventListener('keydown', event => {
          if (composing || event.isComposing || !['Enter', 'Escape'].includes(event.key)) return;
          event.preventDefault(); event.stopPropagation();
          emit(0, 'ime', { text: ime.value, generation: imeGeneration, composing: false, active: false });
          ime.blur(); ime.remove(); ime = undefined;
          document.querySelector('#game-canvas')?.focus();
        });
        document.body.append(ime);
      }
      ime.value = payload.text ?? ''; ime.maxLength = payload.maxLength ?? 64; ime.focus();
      return { generation: imeGeneration };
    },
    async fullscreen() {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await document.documentElement.requestFullscreen();
      return { active: !!document.fullscreenElement };
    },
    async videoFrame(payload) { if (!videos) throw new Error('资源目录尚未初始化。'); return videos.frame(payload); },
    async videoClose() { videos?.close(); return null; },
    async audioUnlock() { return unlockAudio(); },
    async fatal(payload) { window.dispatchEvent(new CustomEvent('virtual-life-fatal', { detail: payload.message })); return null; },
    async checksum(payload) { return { checksum: await sha256(new TextEncoder().encode(payload.text)) }; },
  };
  return {
    request(id, operation, payload) {
      const action = operations[operation];
      if (!action) { emit(id, operation, null, undefined, `未知浏览器操作：${operation}`); return; }
      // Start immediately so file pickers/fullscreen can retain user activation.
      let result;
      try { result = action(payload ?? {}); }
      catch (error) { emit(id, operation, null, undefined, failure(error)); return; }
      void Promise.resolve(result).then(result => {
        if (result?.bytes instanceof Uint8Array) emit(id, operation, result.value, result.bytes);
        else emit(id, operation, result);
      }).catch(error => emit(id, operation, ['asset', 'videoFrame'].includes(operation) ? { resourceId: payload.resourceId, generation: payload.generation } : null, undefined, failure(error)));
    },
    dispose() {
      disposed = true; for (const cleanup of cleanups) cleanup(); assets?.dispose(); videos?.close();
      ime?.remove(); ariaStatus?.remove(); delete window.__virtualLifeBevy;
    },
  };
}
