import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { localPath, verifyAssets } from './verify-assets.mjs';

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'virtual-life-artifact-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const bytes = Buffer.from('original bitmap fixture');
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  const objectPath = `assets/game/objects/${sha256.slice(0, 2)}/${sha256}.bmp`;
  await mkdir(path.dirname(localPath(root, objectPath)), { recursive: true });
  await mkdir(path.join(root, 'assets/catalog'), { recursive: true });
  await writeFile(localPath(root, objectPath), bytes);
  await writeFile(path.join(root, 'assets/catalog/rules.json'), '{}');
  const manifest = { formatVersion: 1, objects: [{ sha256, path: objectPath, bytes: bytes.length }],
    resources: { 'install/image.bmp': { sha256 } }, catalogs: { rules: 'assets/catalog/rules.json' },
    statistics: { resourceCount: 1, uniqueObjectCount: 1, storedBytes: bytes.length } };
  await writeFile(path.join(root, 'assets/manifest.json'), JSON.stringify(manifest));
  return { root, bytes, objectPath, manifest };
}

test('publication verification detects byte corruption and unresolved LFS before deployment', async t => {
  const { root, bytes, objectPath } = await fixture(t);
  assert.equal((await verifyAssets(root)).objects, 1);
  await writeFile(localPath(root, objectPath), Buffer.alloc(bytes.length, 0));
  await assert.rejects(verifyAssets(root), /SHA-256/);
  await writeFile(localPath(root, objectPath), 'version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 1\n');
  await assert.rejects(verifyAssets(root), /LFS pointer/);
});

test('publication shell and resource references must resolve to validated files', async t => {
  const { root, manifest } = await fixture(t);
  await writeFile(path.join(root, 'index.html'), '<title>Virtual Life</title>');
  await writeFile(path.join(root, 'engine.wasm'), Buffer.from([0, 97, 115, 109]));
  await writeFile(path.join(root, 'shell-files.json'), '["index.html","engine.wasm"]');
  await writeFile(path.join(root, 'sw.js'), 'const BUILD="fixture";');
  assert.equal((await verifyAssets(root, { published: true })).published, true);
  await writeFile(path.join(root, 'shell-files.json'), '["index.html","missing.wasm"]');
  await assert.rejects(verifyAssets(root, { published: true }), /ENOENT/);
  manifest.resources['install/missing.bmp'] = { sha256: '0'.repeat(64) };
  await writeFile(path.join(root, 'assets/manifest.json'), JSON.stringify(manifest));
  await assert.rejects(verifyAssets(root), /Missing object/);
});

test('artifact paths reject traversal, absolute paths, Windows drives and alternate streams', () => {
  for (const value of ['../escape', '/root', 'C:/file', 'a:stream', 'a\\b', 'a//b', './a']) assert.throws(() => localPath('/artifact', value), /Unsafe/);
});
