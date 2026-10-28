// Re-inspect original object headers without re-extracting or discarding derived media.
import { readFile, rename, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { inspectMedia } from './legacy-formats.mjs';
import { localPath } from './verify-assets.mjs';
const root = path.resolve(import.meta.dirname, '..');
const manifestPath = localPath(root, 'assets/manifest.json');
const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
const names = new Map(Object.entries(manifest.resources).map(([id, entry]) => [entry.sha256, id]));
let changed = 0;
for (const object of manifest.objects) {
  if (object.derivedFrom) continue;
  const bytes = await readFile(localPath(root, object.path));
  if (createHash('sha256').update(bytes).digest('hex') !== object.sha256) throw new Error(`Original object changed: ${object.path}`);
  const { extension, ...media } = inspectMedia(bytes, names.get(object.sha256));
  const next = `assets/game/objects/${object.sha256.slice(0, 2)}/${object.sha256}.${extension}`;
  if (next !== object.path) {
    await rename(localPath(root, object.path), localPath(root, next));
    console.log(`${object.path} -> ${next}`);
    object.path = next;
  }
  if (JSON.stringify(object.media) !== JSON.stringify(media)) changed++;
  object.media = media;
}
await writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
console.log(`Refreshed ${changed} media records; original SHA-256 values unchanged.`);
