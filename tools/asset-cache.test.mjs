import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { AssetCache } from '../src/asset-cache.ts';
import { mapResources, resourcePlan } from '../src/preload-plan.ts';

const root = 'https://game.example/VirtualLife/';

function objectFor(bytes) {
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  return { sha256, path: `assets/game/objects/${sha256.slice(0, 2)}/${sha256}.bmp`, bytes: bytes.length, media: 'bmp' };
}

function browser(t, { fetch: download, openFails = false, readFails = false, writeFails = false } = {}) {
  const entries = new Map();
  const calls = { downloads: [], reads: [], writes: [], deletes: [] };
  const cache = {
    async match(url) {
      calls.reads.push(url);
      if (readFails) throw new Error('Cache read unavailable');
      return entries.get(url)?.clone();
    },
    async put(url, response) {
      calls.writes.push(url);
      if (writeFails) throw new Error('QuotaExceededError');
      entries.set(url, response.clone());
    },
    async delete(url) { calls.deletes.push(url); return entries.delete(url); },
  };
  const replace = (key, value) => {
    const descriptor = Object.getOwnPropertyDescriptor(globalThis, key);
    Object.defineProperty(globalThis, key, { configurable: true, writable: true, value });
    t.after(() => {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor);
      else delete globalThis[key];
    });
  };
  replace('location', { href: root });
  replace('caches', { async open(name) { assert.equal(name, 'virtual-life-disc-v1'); if (openFails) throw new Error('Cache disabled'); return cache; } });
  replace('fetch', async (url, options) => { calls.downloads.push({ url, options }); return download(url, options); });
  return { entries, calls, create: () => new AssetCache(path => `/VirtualLife/${path}`) };
}

test('warming and concurrent aliases share one verified download and return independently transferable copies', async t => {
  const bytes = Buffer.from('original resource bytes');
  const object = objectFor(bytes);
  let startDownload;
  const started = new Promise(resolve => { startDownload = resolve; });
  let finishDownload;
  const finished = new Promise(resolve => { finishDownload = resolve; });
  const { entries, calls, create } = browser(t, { fetch: async () => { startDownload(); await finished; return new Response(bytes); } });
  const cache = create();
  const warming = cache.warm(object);
  const firstRead = cache.bytes({ ...object });
  const secondRead = cache.bytes({ ...object });
  await started;
  assert.equal(calls.downloads.length, 1);
  assert.equal(calls.downloads[0].options.priority, 'low');
  finishDownload();
  const [, first, second] = await Promise.all([warming, firstRead, secondRead]);
  assert.notEqual(first, second);
  assert.deepEqual(Buffer.from(first), bytes);
  const transferred = structuredClone(first, { transfer: [first] });
  assert.equal(first.byteLength, 0);
  assert.deepEqual(Buffer.from(transferred), bytes);
  assert.deepEqual(Buffer.from(second), bytes);
  assert.deepEqual(Buffer.from(await cache.bytes(object)), bytes);
  assert.equal(calls.downloads.length, 1);
  assert.equal(calls.writes.length, 1);
  assert.deepEqual(Buffer.from(await entries.get(new URL(object.path, root).href).arrayBuffer()), bytes);
});

test('a fresh client reads the persistent object without another network request', async t => {
  const bytes = Buffer.from('persist between game sessions');
  const object = objectFor(bytes);
  const { calls, create } = browser(t, { fetch: async () => new Response(bytes) });
  await create().warm(object);
  const restored = await create().bytes(object);
  assert.deepEqual(Buffer.from(restored), bytes);
  assert.equal(calls.downloads.length, 1);
  assert.equal(calls.writes.length, 1);
});

for (const corruption of ['length', 'sha256']) {
  test(`a persisted object with invalid ${corruption} is removed and repaired with a reload request`, async t => {
    const bytes = Buffer.from('trusted original');
    const object = objectFor(bytes);
    const { entries, calls, create } = browser(t, { fetch: async () => new Response(bytes) });
    const url = new URL(object.path, root).href;
    entries.set(url, new Response(corruption === 'length' ? Buffer.from('short') : Buffer.alloc(bytes.length, 0)));
    assert.deepEqual(Buffer.from(await create().bytes(object)), bytes);
    assert.deepEqual(calls.deletes, [url]);
    assert.equal(calls.downloads.length, 1);
    assert.equal(calls.downloads[0].options.cache, 'reload');
    assert.deepEqual(Buffer.from(await entries.get(url).arrayBuffer()), bytes);
  });
}

test('network corruption is never persisted and the next foreground attempt can succeed', async t => {
  const bytes = Buffer.from('validated retry resource');
  const object = objectFor(bytes);
  let attempts = 0;
  const { entries, calls, create } = browser(t, { fetch: async () => new Response(++attempts === 1 ? Buffer.alloc(bytes.length, 0) : bytes) });
  const cache = create();
  await assert.rejects(cache.warm(object), /资源校验失败/);
  assert.equal(entries.size, 0);
  assert.equal(calls.writes.length, 0);
  assert.deepEqual(Buffer.from(await cache.bytes(object)), bytes);
  assert.equal(calls.downloads.length, 2);
  assert.equal(calls.downloads[1].options.cache, 'reload');
  assert.equal(calls.writes.length, 1);
});

test('a failed repair of persisted corruption keeps bypassing the browser HTTP cache on retry', async t => {
  const bytes = Buffer.from('repair after a temporary network outage');
  const object = objectFor(bytes);
  let attempts = 0;
  const { entries, calls, create } = browser(t, { fetch: async () => {
    if (++attempts === 1) throw new TypeError('Failed to fetch');
    return new Response(bytes);
  } });
  entries.set(new URL(object.path, root).href, new Response(Buffer.alloc(bytes.length, 0)));
  const cache = create();
  await assert.rejects(cache.warm(object), /资源下载失败/);
  assert.deepEqual(Buffer.from(await cache.bytes(object)), bytes);
  assert.deepEqual(calls.downloads.map(call => call.options.cache), ['reload', 'reload']);
});

for (const failure of ['openFails', 'readFails', 'writeFails']) {
  test(`${failure} does not prevent online loading or memory reuse`, async t => {
    const bytes = Buffer.from('play while cache unavailable');
    const object = objectFor(bytes);
    const { calls, create } = browser(t, { [failure]: true, fetch: async () => new Response(bytes) });
    const cache = create();
    assert.deepEqual(Buffer.from(await cache.bytes(object)), bytes);
    assert.deepEqual(Buffer.from(await cache.bytes(object)), bytes);
    assert.equal(calls.downloads.length, 1);
    assert.equal(calls.downloads[0].options.cache, failure === 'readFails' ? 'reload' : 'default');
  });
}

test('HTTP and network failures release the shared attempt so a later read retries', async t => {
  const bytes = Buffer.from('recover after server failure');
  const object = objectFor(bytes);
  let attempts = 0;
  const { calls, create } = browser(t, { fetch: async () => {
    attempts++;
    if (attempts === 1) return new Response('unavailable', { status: 503 });
    if (attempts === 2) throw new TypeError('Failed to fetch');
    return new Response(bytes);
  } });
  const cache = create();
  await assert.rejects(cache.bytes(object), /资源读取失败（503）/);
  await assert.rejects(cache.bytes(object), /资源下载失败/);
  assert.equal(calls.writes.length, 0);
  assert.deepEqual(Buffer.from(await cache.bytes(object)), bytes);
  assert.equal(calls.downloads.length, 3);
});

test('raw bytes evicted from the 32 MiB memory budget remain reusable from persistent storage', async t => {
  const first = Buffer.alloc(17 * 1024 * 1024, 1);
  const second = Buffer.alloc(17 * 1024 * 1024, 2);
  const a = objectFor(first), b = objectFor(second);
  const { calls, create } = browser(t, { fetch: async url => new Response(url.endsWith(`${a.sha256}.bmp`) ? first : second) });
  const cache = create();
  await cache.warm(a);
  await cache.warm(b);
  assert.equal(calls.reads.length, 2);
  await cache.warm(a);
  assert.equal(calls.reads.length, 3);
  assert.equal(calls.downloads.length, 2);
  assert.equal(calls.writes.length, 2);
});

function planFixture() {
  const maps = ['park', 'study', 'lost', 'social', 'business'].map(id => ({
    id, sourceResource: `cd/data/map/${id}/${id}.mmi`,
    backgroundTiles: [{ resourceId: `cd/data/map/${id}/tile.bmp` }], legacySections: {},
  }));
  const resources = Object.fromEntries([
    'install/bmp/pm0301.bmp', ...maps.flatMap(map => map.backgroundTiles.map(tile => tile.resourceId)),
    ...['park', 'school1', 'school2', 'lost', 'social'].map(name => `install/music/${name}.wav`),
  ].map(resource => [resource, { sha256: 'same-hash-for-aliases' }]));
  const rules = {
    stages: [
      { id: 0, mapPath: 'map/park/park', musicPath: 'music/park.wav', stageTurns: 30 },
      { id: 1, mapPath: 'map/park/park', musicPath: 'music/park.wav', stageTurns: 25 },
      { id: 2, mapPath: 'map/park/park', musicPath: 'music/park.wav', stageTurns: 15 },
      { id: 3, mapPath: 'map/study/study', musicPath: 'music/school1.wav', stageTurns: 15 },
      { id: 4, mapPath: 'map/study/study', musicPath: 'music/school2.wav', stageTurns: 20 },
      { id: 5, mapPath: 'map/lost/lost', musicPath: 'music/lost.wav', stageTurns: -1 },
      { id: 6, mapPath: 'map/social/social', musicPath: 'music/social.wav', stageTurns: 100 },
    ],
    characters: { directories: [], animationSuffixes: [], portraits: [] },
  };
  return {
    manifest: { resources }, maps: { maps }, rules,
    resolveLegacy(relative) {
      const path = relative.replaceAll('\\', '/').toLowerCase().replace(/^\.\//, '');
      for (const candidate of [path, `install/${path}`, `cd/data/${path}`, `cd/${path}`]) {
        if (resources[candidate]) return candidate;
      }
      throw new Error(`Missing resource: ${relative}`);
    },
  };
}

test('map plans deduplicate tiles and resolve only map decorations beside the source map', () => {
  assert.deepEqual(mapResources({
    sourceResource: 'cd/data/map/park/park.mmi',
    backgroundTiles: [{ resourceId: 'tile.bmp' }, { resourceId: 'tile.bmp' }],
    legacySections: {
      Spirit_1: { File: 'ANI\\TREE.FLC' },
      Floater_2: { File: 'WATER.FLC' },
      Other_3: { File: 'ignored.flc' },
      Spirit_4: { File: { bytes: [1] } },
    },
  }), ['tile.bmp', 'cd/data/map/park/ani/tree.flc', 'cd/data/map/park/water.flc']);
});

test('startup works with a partial manifest, prioritizes character creation and excludes unrelated career maps', () => {
  const assets = planFixture();
  const plan = resourcePlan(assets);
  assert.ok(plan.includes('install/bmp/pm0301.bmp'));
  assert.ok(plan.includes('cd/data/map/park/tile.bmp'));
  assert.ok(plan.indexOf('install/bmp/pm0301.bmp') < plan.indexOf('cd/data/map/park/tile.bmp'));
  assert.ok(plan.includes('install/music/park.wav'));
  assert.ok(!plan.includes('cd/data/map/business/tile.bmp'));
  assert.equal(plan.length, new Set(plan).size);
  assert.ok(plan.every(resource => assets.manifest.resources[resource]));
});

test('a new life prepares the actual stage-two characters for both genders', () => {
  const assets = planFixture();
  assets.rules.characters.animationSuffixes = ['M00w1a.flc'];
  for (const [index, model] of [[0, 'm27'], [2, 'm28'], [33, 'f28']]) {
    assets.rules.characters.directories[index] = `charani/${model}/`;
    assets.rules.characters.portraits[index] = `face/${model}.bmp`;
    for (const resource of [`cd/data/charani/${model}/${model}w1a.flc`, `install/face/${model}.bmp`]) {
      assets.manifest.resources[resource] = { sha256: resource };
    }
  }
  const plan = resourcePlan(assets);
  for (const model of ['m28', 'f28']) {
    assert.ok(plan.includes(`cd/data/charani/${model}/${model}w1a.flc`));
    assert.ok(plan.includes(`install/face/${model}.bmp`));
  }
  assert.ok(!plan.includes('cd/data/charani/m27/m27w1a.flc'));
});

function stageSnapshot(stage, stageTurn, mapId) {
  return { mapId, player: { sex: 'male' }, life: { stage, stageTurn, profession: -1 } };
}

test('next-stage resources arrive near the boundary, including future character art', () => {
  const assets = planFixture();
  assets.rules.characters.directories[3] = 'actors/m03/';
  assets.rules.characters.animationSuffixes = ['M00a.flc'];
  assets.rules.characters.portraits[3] = 'face/p03.bmp';
  for (const resource of ['install/actors/m03/m03a.flc', 'install/face/p03.bmp']) {
    assets.manifest.resources[resource] = { sha256: resource };
  }
  const early = resourcePlan(assets, stageSnapshot(2, 9, 'park'));
  assert.ok(!early.includes('cd/data/map/study/tile.bmp'));
  const near = resourcePlan(assets, stageSnapshot(2, 10, 'park'));
  for (const resource of ['cd/data/map/study/tile.bmp', 'install/music/school1.wav', 'install/actors/m03/m03a.flc', 'install/face/p03.bmp']) {
    assert.ok(near.includes(resource), resource);
  }
  assert.ok(!near.includes('cd/data/map/business/tile.bmp'));
});

test('school exam branches preload study, lost and social while university skips lost', () => {
  const assets = planFixture();
  const school = resourcePlan(assets, stageSnapshot(3, 10, 'study'));
  for (const resource of ['cd/data/map/study/tile.bmp', 'cd/data/map/lost/tile.bmp', 'cd/data/map/social/tile.bmp', 'install/music/school2.wav', 'install/music/lost.wav', 'install/music/social.wav']) {
    assert.ok(school.includes(resource), resource);
  }
  assert.ok(!school.includes('cd/data/map/business/tile.bmp'));
  const university = resourcePlan(assets, stageSnapshot(4, 15, 'study'));
  assert.ok(university.includes('cd/data/map/social/tile.bmp'));
  assert.ok(!university.includes('cd/data/map/lost/tile.bmp'));
});

test('untimed lost stage does not speculate a social-map transition', () => {
  const plan = resourcePlan(planFixture(), stageSnapshot(5, 100, 'lost'));
  assert.ok(plan.includes('cd/data/map/lost/tile.bmp'));
  assert.ok(!plan.includes('cd/data/map/social/tile.bmp'));
  assert.ok(!plan.includes('install/music/social.wav'));
});
