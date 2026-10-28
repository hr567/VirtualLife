import { readFileSync, writeFileSync } from 'node:fs';
import { parseFortuneData, parseHighTestData } from './legacy-formats.mjs';

const manifest = JSON.parse(readFileSync(new URL('../assets/manifest.json', import.meta.url), 'utf8'));
const readResource = (id) => {
  const object = manifest.objects.find((object) => object.sha256 === manifest.resources[id]?.sha256);
  if (!object) throw new Error(`Missing questionnaire source ${id}`);
  return readFileSync(new URL(`../${object.path}`, import.meta.url));
};
const target = new URL('../assets/catalog/questionnaires.json', import.meta.url);
const catalog = JSON.parse(readFileSync(target, 'utf8'));
Object.assign(catalog.fortune, parseFortuneData(readResource('install/data1/fortune.idx'), readResource('install/data1/fortune.dat')));
Object.assign(catalog.highTest, parseHighTestData(readResource('install/data1/hightest.idx'), readResource('install/data1/hightest.dat')));
writeFileSync(target, `${JSON.stringify(catalog, null, 2)}\n`);
console.log(JSON.stringify({ fortuneCount: catalog.fortune.records.length, highTestCount: catalog.highTest.records.length, unresolved: catalog.fortune.records.filter((record) => !record.runtimeCompatible).map(({ id, issues }) => ({ id, issues })) }, null, 2));
