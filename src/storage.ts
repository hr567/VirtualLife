export interface SaveRecord { slot: 'current' | 'previous'; savedAt: string; contentId: string; payload: string; checksum: string }
interface ExportedSave { format: 'virtual-life-web-save'; version: 1; savedAt: string; contentId: string; payload: string; checksum: string }

const request = <T>(operation: IDBRequest<T>): Promise<T> => new Promise((resolve, reject) => {
  operation.onsuccess = () => resolve(operation.result);
  operation.onerror = () => reject(operation.error ?? new Error('读取浏览器存储失败'));
});
const completed = (transaction: IDBTransaction): Promise<void> => new Promise((resolve, reject) => {
  transaction.oncomplete = () => resolve();
  transaction.onerror = transaction.onabort = () => reject(transaction.error ?? new Error('存档写入失败'));
});
export async function checksum(text: string): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(hash), byte => byte.toString(16).padStart(2, '0')).join('');
}

export class BrowserStorage {
  private writes: Promise<unknown> = Promise.resolve();
  private constructor(private db: IDBDatabase) {}
  static async open(): Promise<BrowserStorage> {
    const opening = indexedDB.open('virtual-life', 1);
    opening.onupgradeneeded = () => {
      opening.result.createObjectStore('saves', { keyPath: 'slot' });
      opening.result.createObjectStore('settings');
    };
    const db = await request(opening);
    db.onversionchange = () => db.close();
    return new BrowserStorage(db);
  }
  async save(payload: string, contentId: string): Promise<SaveRecord> {
    const savedAt = new Date().toISOString();
    const task = this.writes.then(async () => this.commit({ slot: 'current', savedAt, contentId, payload, checksum: await checksum(payload) }));
    this.writes = task.catch(() => {});
    return task;
  }
  private async commit(record: SaveRecord): Promise<SaveRecord> {
    // Digest outside the write transaction: awaiting WebCrypto inside IndexedDB
    // would allow the transaction to close. Recheck the record atomically to
    // avoid overwriting a concurrent tab's newer snapshot with a stale backup.
    for (let attempt = 0; attempt < 8; attempt++) {
      const old = await request(this.db.transaction('saves').objectStore('saves').get('current')) as SaveRecord | undefined;
      const valid = old && typeof old.payload === 'string' && typeof old.checksum === 'string' && await checksum(old.payload) === old.checksum;
      const transaction = this.db.transaction('saves', 'readwrite');
      const done = completed(transaction);
      const store = transaction.objectStore('saves');
      const latest = store.get('current');
      let conflict = false;
      let writeError: unknown;
      latest.onsuccess = () => {
        if (JSON.stringify(latest.result) !== JSON.stringify(old)) {
          conflict = true;
          transaction.abort();
          return;
        }
        try {
          if (valid) store.put({ ...old, slot: 'previous' });
          store.put(record);
        } catch (error) {
          writeError = error;
          transaction.abort();
        }
      };
      try { await done; return record; }
      catch (error) {
        if (conflict) continue;
        const failure = writeError ?? error;
        if (failure instanceof DOMException && failure.name === 'QuotaExceededError') throw new Error('浏览器存储空间不足，旧存档已保留。请清理空间后重试或导出备份。');
        throw failure;
      }
    }
    throw new Error('其他页面正在更新存档，请关闭重复打开的游戏页面后重试。');
  }
  async load(slot: SaveRecord['slot'] = 'current'): Promise<SaveRecord | undefined> {
    const record = await request(this.db.transaction('saves').objectStore('saves').get(slot)) as SaveRecord | undefined;
    if (record && await checksum(record.payload) !== record.checksum) throw new Error('存档校验失败，请尝试恢复上一份存档。');
    return record;
  }
  async setting<T>(key: string, fallback: T): Promise<T> {
    const value = await request(this.db.transaction('settings').objectStore('settings').get(key)) as T | undefined;
    return value ?? fallback;
  }
  async setSetting(key: string, value: unknown): Promise<void> {
    const transaction = this.db.transaction('settings', 'readwrite');
    const done = completed(transaction);
    transaction.objectStore('settings').put(value, key);
    await done;
  }
  async exportFile(): Promise<void> {
    const record = await this.load();
    if (!record) throw new Error('还没有可以导出的存档。');
    const { slot: _slot, ...fields } = record;
    const exported: ExportedSave = { format: 'virtual-life-web-save', version: 1, ...fields };
    const url = URL.createObjectURL(new Blob([JSON.stringify(exported)], { type: 'application/json' }));
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `虚拟人生-${record.savedAt.slice(0, 10)}.vlsave.json`;
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }
  async readImport(file: File, contentId: string): Promise<ExportedSave> {
    if (file.size > 8 * 1024 * 1024) throw new Error('存档文件超过 8 MiB 限制。');
    const value: unknown = JSON.parse(await file.text());
    if (!value || typeof value !== 'object') throw new Error('存档格式错误。');
    const record = value as Partial<ExportedSave>;
    if (record.format !== 'virtual-life-web-save' || record.version !== 1 || typeof record.payload !== 'string' || typeof record.checksum !== 'string') throw new Error('不支持的存档格式。');
    if (record.contentId !== contentId) throw new Error('此存档对应不同的游戏资源版本，当前版本无法导入。');
    if (await checksum(record.payload) !== record.checksum) throw new Error('导入文件校验失败。');
    return record as ExportedSave;
  }
}
