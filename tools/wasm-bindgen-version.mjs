import { readFileSync } from 'node:fs';
const lock = readFileSync(new URL('../Cargo.lock', import.meta.url), 'utf8').replaceAll('\r\n', '\n');
const version = lock.match(/\[\[package\]\]\nname = "wasm-bindgen"\nversion = "([^"]+)"/)?.[1];
if (!version) throw new Error('Cargo.lock does not pin wasm-bindgen');
process.stdout.write(version);
