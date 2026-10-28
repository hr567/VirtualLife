// Assemble a separate reference runtime without executing the disc installer.
import { readFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { resolve, dirname, relative, isAbsolute } from 'node:path';
const root = resolve(import.meta.dirname, '..');
const destination = resolve(root, 'research/runtime');
const manifest = JSON.parse(readFileSync(resolve(root, 'assets/manifest.json'), 'utf8'));
const objects = new Map(manifest.objects.map(object => [object.sha256, object]));
mkdirSync(destination, { recursive: true });
let copied = 0;
for (const [id, resource] of Object.entries(manifest.resources)) {
  if (!id.startsWith('install/')) continue;
  const target = resolve(destination, resource.source.relativePath);
  const rel = relative(destination, target);
  if (rel.startsWith('..') || isAbsolute(rel)) throw new Error(`Unsafe original path: ${id}`);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(resolve(root, objects.get(resource.sha256).path), target);
  copied++;
}
copyFileSync(resolve(root, 'research/original/N2Game.exe'), resolve(destination, 'N2Game.exe'));
console.log(`Assembled ${copied} resources and N2Game.exe in ${destination}. No installer executed.`);
