import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import test from 'node:test';
import { SaveStorage, sha256, validateSave, VerifiedAssets } from '../../../web/platform.js';

// Minimal transactional double: writes commit together after all queued
// requests, and aborts discard every mutation, including the backup rotation.
class TransactionalDatabase {
  stores = { saves: new Map(), settings: new Map() };
  failSlot = null;
  transaction(name, mode) {
    const working = new Map(structuredClone([...this.stores[name]]));
    let pending = 0, aborted = false;
    const transaction = {
      error: null,
      abort() {
        if (aborted) return;
        aborted = true; queueMicrotask(() => transaction.onabort?.());
      },
    };
    const operation = value => {
      pending++;
      const request = {};
      queueMicrotask(() => {
        if (!aborted) { request.result = value(); request.onsuccess?.(); }
        if (!--pending) queueMicrotask(() => {
          if (aborted || pending) return;
          if (mode === 'readwrite') this.stores[name] = working;
          transaction.oncomplete?.();
        });
      });
      return request;
    };
    transaction.objectStore = () => ({
      get: key => operation(() => structuredClone(working.get(key))),
      getAllKeys: () => operation(() => [...working.keys()]),
      getAll: () => operation(() => structuredClone([...working.values()])),
      put: (value, key) => {
        if (value?.slot === this.failSlot) {
          this.failSlot = null; throw new DOMException('full', 'QuotaExceededError');
        }
        return operation(() => { working.set(key ?? value.slot, structuredClone(value)); return key ?? value.slot; });
      },
    });
    return transaction;
  }
}

test('nine slots persist independently and automatic saves only replace slot zero', async () => {
  const database=new TransactionalDatabase(), storage=new SaveStorage(database);
  for(let slot=0;slot<9;slot++) await storage.save('slot-'+slot,JSON.stringify({state:{player:{name:'角色'+slot},turn:slot}}),'content');
  const before=structuredClone([...database.stores.saves]);
  await storage.save('slot-0','next roll','content');
  assert.equal((await storage.load('slot-0')).payload,'next roll');
  for(let slot=1;slot<9;slot++) assert.deepEqual(await storage.load('slot-'+slot),before[slot][1]);
  await assert.rejects(storage.load('slot-9'),/无效/);
  assert.throws(()=>storage.save('current','payload','content'),/无效/);
});

test('quota failure retains an existing slot and a later write still succeeds', async () => {
  const database=new TransactionalDatabase(), storage=new SaveStorage(database);
  await storage.save('slot-0','before','content');
  const before=structuredClone([...database.stores.saves]);database.failSlot='slot-0';
  await assert.rejects(storage.save('slot-0','failed','content'),/旧存档已保留/);
  assert.deepEqual([...database.stores.saves],before);
  await storage.save('slot-0','after','content');assert.equal((await storage.load('slot-0')).payload,'after');
  await storage.setting('volume',37);assert.equal(await storage.setting('volume'),37);
});

test('imports target the requested slot and preserve exported bytes and save time', async () => {
  const database=new TransactionalDatabase(), storage=new SaveStorage(database);
  const record=await storage.save('slot-3','import payload','content');
  await storage.import('slot-8',record,'content',null);
  assert.deepEqual(await storage.load('slot-8'),{...record,slot:'slot-8'});
  assert.deepEqual(await storage.load('slot-3'),record);
  await assert.rejects(storage.import('slot-8',{...record,payload:'corrupt'},'content'),/校验/);
  await assert.rejects(storage.import('slot-8',record,'different'),/不同/);
  assert.deepEqual(await storage.load('slot-8'),{...record,slot:'slot-8'});
});

test('slot listing distinguishes empty, invalid, and valid saves and rejects stale manual writes', async () => {
  const database=new TransactionalDatabase(), storage=new SaveStorage(database);
  await storage.save('slot-1',JSON.stringify({state:{player:{name:'角色'},turn:12}}),'content');
  const listed=await storage.list('content');assert.equal(listed.length,9);
  assert.equal(listed[0].occupied,false);assert.equal(listed[1].name,'角色');assert.equal(listed[1].valid,true);
  await storage.save('slot-1','other tab','content');
  await assert.rejects(storage.save('slot-1','stale','content',listed[1].token),/其他页面/);
  assert.equal((await storage.load('slot-1')).payload,'other tab');
  database.stores.saves.get('slot-1').checksum='0'.repeat(64);
  assert.equal((await storage.list('content'))[1].valid,false);
});

test('writes keep their order while the first WebCrypto checksum is delayed', async () => {
  const database=new TransactionalDatabase(), storage=new SaveStorage(database), original=crypto.subtle.digest;
  let release,started;const waiting=new Promise(resolve=>release=resolve), hashing=new Promise(resolve=>started=resolve);
  crypto.subtle.digest=async function(algorithm,bytes){if(new TextDecoder().decode(bytes)==='first'){started();await waiting;}return original.call(this,algorithm,bytes);};
  try {
    const first=storage.save('slot-0','first','content');await hashing;
    const second=storage.save('slot-0','second','content');assert.equal(database.stores.saves.size,0);
    release();await Promise.all([first,second]);assert.equal((await storage.load('slot-0')).payload,'second');
  }finally{release();crypto.subtle.digest=original;}
});

async function fixture(contents = ['original disc bytes']) {
  const objects = await Promise.all(contents.map(async (content, index) => {
    const bytes = typeof content === 'string' ? new TextEncoder().encode(content) : content;
    const hash = await sha256(bytes, webcrypto);
    return { sha256: hash, path: `assets/game/objects/${hash.slice(0, 2)}/${hash}.bin`, bytes: bytes.byteLength, data: bytes, resource: `resource-${index}` };
  }));
  const manifest = {
    formatVersion: 1,
    objects: objects.map(({ data: _data, resource: _resource, ...object }) => object),
    resources: Object.fromEntries(objects.map(object => [object.resource, { sha256: object.sha256 }])),
  };
  const storage = new Map();
  const fetches = [];
  const writes = [];
  const timers = new Map();
  let nextTimer = 1;
  const cache = {
    async match(url) { return storage.get(url)?.clone(); },
    async put(url, response) { writes.push(url); storage.set(url, response.clone()); },
    async delete(url) { return storage.delete(url); },
  };
  const environment = {
    crypto: webcrypto,
    caches: { async open() { return cache; } },
    document: { hidden: false },
    navigator: { onLine: true, connection: { saveData: false, effectiveType: '4g' } },
    setTimeout(callback) { const id = nextTimer++; timers.set(id, callback); return id; },
    clearTimeout(id) { timers.delete(id); },
    async fetch(url, options) {
      fetches.push({ url, options });
      const object = objects.find(object => url.endsWith(object.path));
      assert.ok(object, 'Only manifest object URLs may be fetched');
      return new Response(object.data.slice());
    },
  };
  const settle = () => new Promise(resolve => setImmediate(resolve));
  const flushTimers = () => { const callbacks = [...timers.values()]; timers.clear(); for (const callback of callbacks) callback(); };
  const assets = new VerifiedAssets(manifest, 'https://example.test/VirtualLife/', environment);
  return { assets, manifest, objects, storage, writes, fetches, environment, cache, flushTimers, settle };
}

test('new save envelope validates checksum and rejects the old client format', async () => {
  const payload = JSON.stringify({ version: 2, phase: 'playing' });
  const record = { format: 'virtual-life-bevy-save', version: 1, savedAt: '2026-10-29T00:00:00.000Z', contentId: 'content-a', payload, checksum: await sha256(new TextEncoder().encode(payload), webcrypto) };
  assert.equal(await validateSave(record, 'content-a', webcrypto), record);
  await assert.rejects(validateSave({ ...record, format: 'virtual-life-web-save' }, 'content-a', webcrypto), /Bevy/);
  await assert.rejects(validateSave(record, 'content-b', webcrypto), /不同/);
  await assert.rejects(validateSave({ ...record, payload: '{}' }, 'content-a', webcrypto), /校验/);
  await assert.rejects(validateSave({ ...record, savedAt: 'invalid' }, 'content-a', webcrypto), /格式/);
});

test('asset reads deduplicate downloads, verify persistence and return isolated byte buffers', async () => {
  const context = await fixture();
  const [left, right] = await Promise.all([context.assets.bytes('resource-0'), context.assets.bytes('resource-0')]);
  assert.equal(context.fetches.length, 1);
  assert.equal(context.writes.length, 1);
  assert.notEqual(left, right);
  new Uint8Array(left)[0] ^= 255;
  assert.deepEqual(new Uint8Array(await context.assets.bytes('resource-0')), context.objects[0].data);
  assert.deepEqual(new Uint8Array(right), context.objects[0].data);
  assert.equal(context.fetches[0].options.priority, 'high');
});

test('corrupt persistent cache is removed and refetched without trusting HTTP cache', async () => {
  const context = await fixture();
  const url = new URL(context.objects[0].path, 'https://example.test/VirtualLife/').href;
  context.storage.set(url, new Response('corrupt original disc bytes'));
  const result = await context.assets.bytes('resource-0');
  assert.deepEqual(new Uint8Array(result), context.objects[0].data);
  assert.equal(context.fetches[0].options.cache, 'reload');
  assert.equal(await sha256(await context.storage.get(url).arrayBuffer(), webcrypto), context.objects[0].sha256);
});

test('corrupt network resources are neither returned nor persisted', async () => {
  const context = await fixture();
  context.environment.fetch = async () => new Response('invalid resource');
  await assert.rejects(context.assets.bytes('resource-0'), /SHA-256/);
  assert.equal(context.storage.size, 0);
  context.environment.fetch = async (_url, options) => {
    assert.equal(options.cache, 'reload'); return new Response(context.objects[0].data);
  };
  assert.deepEqual(new Uint8Array(await context.assets.bytes('resource-0')), context.objects[0].data);
});

test('CacheStorage quota errors preserve online asset access', async () => {
  const context = await fixture(['first', 'second']);
  let writes = 0;
  context.cache.put = async () => { writes++; throw new DOMException('full', 'QuotaExceededError'); };
  assert.equal(new TextDecoder().decode(await context.assets.bytes('resource-0')), 'first');
  assert.equal(new TextDecoder().decode(await context.assets.bytes('resource-1')), 'second');
  assert.equal(writes, 1);
});

test('prefetch starts at most two requests and yields to a foreground request', async () => {
  const context = await fixture(['zero', 'one', 'two', 'three']);
  const pending = [];
  context.environment.fetch = (url, options) => new Promise(resolve => { pending.push({ url, options, resolve }); });
  context.assets.prefetch(['resource-0', 'resource-1', 'resource-2']);
  context.flushTimers(); await context.settle();
  assert.equal(pending.length, 2);
  assert.ok(pending.every(item => item.options.priority === 'low'));
  const foreground = context.assets.bytes('resource-3'); await context.settle();
  assert.equal(pending.length, 3);
  assert.equal(pending[2].options.priority, 'high');
  pending[0].resolve(new Response(context.objects[0].data));
  await context.settle(); await context.settle(); context.flushTimers(); await context.settle();
  assert.equal(pending.length, 3, 'No new background work while the foreground read is pending');
  pending[2].resolve(new Response(context.objects[3].data)); await foreground;
  context.flushTimers(); await context.settle();
  assert.equal(pending.length, 4);
  pending[1].resolve(new Response(context.objects[1].data));
  pending[3].resolve(new Response(context.objects[2].data));
  await context.settle(); context.assets.dispose();
});

test('hidden, offline, data-saving and slow connections pause background starts', async () => {
  for (const pause of ['hidden', 'offline', 'saveData', '2g']) {
    const context = await fixture();
    if (pause === 'hidden') context.environment.document.hidden = true;
    if (pause === 'offline') context.environment.navigator.onLine = false;
    if (pause === 'saveData') context.environment.navigator.connection.saveData = true;
    if (pause === '2g') context.environment.navigator.connection.effectiveType = '2g';
    context.assets.prefetch(['resource-0']); context.flushTimers(); await context.settle();
    assert.equal(context.fetches.length, 0, pause);
    context.environment.document.hidden = false; context.environment.navigator.onLine = true;
    context.environment.navigator.connection = { saveData: false, effectiveType: '4g' };
    context.assets.schedule(); context.flushTimers(); await context.settle(); await context.settle();
    assert.equal(context.fetches.length, 1, `${pause} resumes`);
    context.assets.dispose();
  }
});

test('unregistered references never trigger network reads', async () => {
  const context = await fixture();
  await assert.rejects(context.assets.bytes({ path: '../outside.bin' }), /没有/);
  await assert.rejects(context.assets.bytes({ sha256: '0'.repeat(64) }), /没有/);
  assert.equal(context.fetches.length, 0);
});

test('raw memory cache evicts least-recently-used data above 32 MiB', async () => {
  const first = new Uint8Array(17 * 1024 * 1024).fill(1);
  const second = new Uint8Array(17 * 1024 * 1024).fill(2);
  const context = await fixture([first, second]);
  context.environment.caches = undefined;
  await context.assets.bytes('resource-0');
  await context.assets.bytes('resource-1');
  await context.assets.bytes('resource-0');
  assert.equal(context.fetches.length, 3);
});
