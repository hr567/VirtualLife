import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import ts from 'typescript';

async function storagePage(page: Page): Promise<void> {
  const source = await readFile('src/storage.ts', 'utf8');
  const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ES2022 } }).outputText;
  await page.route('**/storage-harness', route => route.fulfill({ contentType: 'text/html', body: '<title>Storage test</title>' }));
  await page.goto('./storage-harness');
  await page.evaluate(async code => {
    const url = URL.createObjectURL(new Blob([code], { type: 'text/javascript' }));
    const module = await import(url); URL.revokeObjectURL(url);
    (window as any).storageModule = module;
    (window as any).store = await module.BrowserStorage.open();
  }, code);
}

test('corrupt current save never replaces the previous valid snapshot', async ({ page }) => {
  await storagePage(page);
  const result = await page.evaluate(async () => {
    const store = (window as any).store;
    await store.save('one', 'content'); await store.save('two', 'content');
    await new Promise<void>((resolve, reject) => {
      const request = indexedDB.open('virtual-life', 1);
      request.onsuccess = () => {
        const db = request.result, transaction = db.transaction('saves', 'readwrite');
        transaction.objectStore('saves').put({ slot: 'current', payload: 'corrupted', checksum: 'wrong' });
        transaction.oncomplete = () => { db.close(); resolve(); }; transaction.onabort = () => reject(transaction.error);
      };
    });
    let rejected = false; try { await store.load(); } catch { rejected = true; }
    await store.save('three', 'content');
    return { rejected, current: (await store.load()).payload, previous: (await store.load('previous')).payload };
  });
  expect(result).toEqual({ rejected: true, current: 'three', previous: 'one' });
});

test('failed write rolls back both slots and subsequent saves remain possible', async ({ page }) => {
  await storagePage(page);
  const result = await page.evaluate(async () => {
    const store = (window as any).store;
    await store.save('one', 'content'); await store.save('two', 'content');
    const put = IDBObjectStore.prototype.put;
    IDBObjectStore.prototype.put = function (record: any, key?: IDBValidKey) {
      if (this.name === 'saves' && record.payload === 'failed') throw new DOMException('simulated quota exhaustion', 'QuotaExceededError');
      return key === undefined ? put.call(this, record) : put.call(this, record, key);
    };
    let error = ''; try { await store.save('failed', 'content'); } catch (failure) { error = String(failure); }
    finally { IDBObjectStore.prototype.put = put; }
    const retained = [(await store.load()).payload, (await store.load('previous')).payload];
    await store.save('three', 'content');
    return { error, retained, current: (await store.load()).payload, previous: (await store.load('previous')).payload };
  });
  expect(result.error).toContain('存储空间不足');
  expect(result.retained).toEqual(['two', 'one']);
  expect([result.current, result.previous]).toEqual(['three', 'two']);
});

test('save invocation order survives delayed checksum work', async ({ page }) => {
  await storagePage(page);
  const result = await page.evaluate(async () => {
    const store = (window as any).store, digest = crypto.subtle.digest.bind(crypto.subtle);
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    let first = true;
    crypto.subtle.digest = async (...args: Parameters<SubtleCrypto['digest']>) => {
      if (first) { first = false; await gate; } return digest(...args);
    };
    try {
      const earlier = store.save('earlier', 'content'), later = store.save('later', 'content');
      await new Promise(resolve => setTimeout(resolve, 50)); release(); await Promise.all([earlier, later]);
      return [(await store.load()).payload, (await store.load('previous')).payload];
    } finally { crypto.subtle.digest = digest; }
  });
  expect(result).toEqual(['later', 'earlier']);
});

test('concurrent browser connections preserve the immediately preceding valid save', async ({ page }) => {
  await storagePage(page);
  const result = await page.evaluate(async () => {
    const module = (window as any).storageModule, first = (window as any).store, second = await module.BrowserStorage.open();
    await first.save('initial', 'content');
    await Promise.all([first.save('left', 'content'), second.save('right', 'content')]);
    const pair = [(await first.load()).payload, (await first.load('previous')).payload];
    const valid = await first.readImport(new File([JSON.stringify({ format: 'virtual-life-web-save', version: 1,
      contentId: 'content', payload: 'imported', checksum: await module.checksum('imported') })], 'save.json'), 'content');
    let rejected = false;
    try { await first.readImport(new File([JSON.stringify({ ...valid, payload: 'tampered' })], 'bad.json'), 'content'); } catch { rejected = true; }
    return { pair, rejected };
  });
  expect(result.pair.sort()).toEqual(['left', 'right']); expect(result.rejected).toBe(true);
});
