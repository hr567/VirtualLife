import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, open, readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const LFS_HEADER = 'version https://git-lfs.github.com/spec/v1';
const LIMIT = 1_000_000_000;

export function localPath(root, relative) {
  if (typeof relative !== 'string' || !relative || relative.includes('\\') ||
      relative.includes(':') || relative.startsWith('/') || relative.split('/').some(part => !part || part === '.' || part === '..')) {
    throw new Error(`Unsafe artifact path: ${relative}`);
  }
  const absolute = path.resolve(root, relative);
  if (!absolute.startsWith(path.resolve(root) + path.sep)) throw new Error(`Artifact escapes root: ${relative}`);
  return absolute;
}

async function fileInfo(file) {
  const info = await lstat(file);
  if (!info.isFile() || info.isSymbolicLink() || info.nlink > 1) throw new Error(`Artifact must be a regular unlinked file: ${file}`);
  const handle = await open(file, 'r');
  try {
    const header = Buffer.alloc(LFS_HEADER.length);
    await handle.read(header, 0, header.length, 0);
    if (header.toString('utf8') === LFS_HEADER) throw new Error(`Unresolved Git LFS pointer: ${file}`);
  } finally { await handle.close(); }
  return info;
}

async function hashFile(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}

async function walk(root, relative = '') {
  const files = [];
  for (const entry of await readdir(path.join(root, relative), { withFileTypes: true })) {
    const child = relative ? `${relative}/${entry.name}` : entry.name;
    if (entry.isSymbolicLink()) throw new Error(`Symbolic link in artifact: ${child}`);
    if (entry.isDirectory()) files.push(...await walk(root, child));
    else files.push(child);
  }
  return files;
}

export async function verifyAssets(root, { published = false } = {}) {
  root = path.resolve(root);
  await fileInfo(localPath(root, 'assets/manifest.json'));
  const manifest = JSON.parse(await readFile(localPath(root, 'assets/manifest.json'), 'utf8'));
  if (manifest.formatVersion !== 1 || !Array.isArray(manifest.objects) || !manifest.resources) throw new Error('Unsupported resource manifest');
  const byHash = new Map();
  const paths = new Set();
  for (const object of manifest.objects) {
    if (!/^[a-f0-9]{64}$/.test(object.sha256) || byHash.has(object.sha256)) throw new Error('Invalid or duplicate object hash');
    if (!object.path.startsWith(`assets/game/objects/${object.sha256.slice(0, 2)}/${object.sha256}.`) || paths.has(object.path)) throw new Error(`Invalid content address: ${object.path}`);
    if (!Number.isSafeInteger(object.bytes) || object.bytes < 1) throw new Error(`Invalid object size: ${object.path}`);
    byHash.set(object.sha256, object);
    paths.add(object.path);
  }
  let cursor = 0;
  let storedBytes = 0;
  await Promise.all(Array.from({ length: 8 }, async () => {
    while (cursor < manifest.objects.length) {
      const object = manifest.objects[cursor++];
      const file = localPath(root, object.path);
      const info = await fileInfo(file);
      if (info.size !== object.bytes) throw new Error(`Wrong resource length: ${object.path}`);
      if (await hashFile(file) !== object.sha256) throw new Error(`Wrong resource SHA-256: ${object.path}`);
      if (object.derivedFrom && !byHash.has(object.derivedFrom)) throw new Error(`Missing derived resource source: ${object.path}`);
      storedBytes += info.size;
    }
  }));
  for (const [id, resource] of Object.entries(manifest.resources)) {
    if (!byHash.has(resource.sha256)) throw new Error(`Missing object for ${id}`);
    if (/\.(exe|dll|cab)$/i.test(id)) throw new Error(`Local executable or installer in public resource set: ${id}`);
    if (resource.derivedFrom && !manifest.resources[resource.derivedFrom]) throw new Error(`Unknown derived source for ${id}`);
  }
  if (manifest.statistics.resourceCount !== Object.keys(manifest.resources).length ||
      manifest.statistics.uniqueObjectCount !== byHash.size || manifest.statistics.storedBytes !== storedBytes) throw new Error('Manifest statistics disagree with verified resources');
  for (const catalog of new Set([...Object.values(manifest.catalogs), 'assets/catalog/rules.json'])) {
    await fileInfo(localPath(root, catalog));
    JSON.parse(await readFile(localPath(root, catalog), 'utf8'));
  }
  let totalBytes = storedBytes;
  let fileCount = byHash.size;
  if (published) {
    const files = await walk(root);
    totalBytes = 0;
    fileCount = files.length;
    for (const relative of files) {
      if (/\.(exe|dll|cab|pdb|gpr|asm)$/i.test(relative) || relative.startsWith('research/')) throw new Error(`Local research file in published output: ${relative}`);
      totalBytes += (await fileInfo(localPath(root, relative))).size;
    }
    const shell = JSON.parse(await readFile(localPath(root, 'shell-files.json'), 'utf8'));
    if (!Array.isArray(shell) || !shell.includes('index.html') || !shell.some(file => file.endsWith('.wasm'))) throw new Error('Application shell is missing index.html or WebAssembly');
    for (const relative of shell) await fileInfo(localPath(root, relative));
    const serviceWorker = await readFile(localPath(root, 'sw.js'), 'utf8');
    if (serviceWorker.includes('__BUILD_ID__')) throw new Error('Service Worker build version was not injected');
  }
  if (totalBytes >= LIMIT) throw new Error(`Pages artifact exceeds budget: ${totalBytes} >= ${LIMIT}`);
  return { root, resources: Object.keys(manifest.resources).length, objects: byHash.size, storedBytes, fileCount, totalBytes, budgetBytes: LIMIT, published };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const published = process.argv.includes('--dist');
  const rootFlag = process.argv.indexOf('--root');
  const root = rootFlag === -1 ? (published ? 'dist' : '.') : process.argv[rootFlag + 1];
  verifyAssets(root, { published }).then(result => console.log(JSON.stringify(result, null, 2))).catch(error => { console.error(error.message); process.exitCode = 1; });
}
