// The deployment gate compares actual executions with the unsharded test list.
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

function testsIn(report) {
  assert.deepEqual(report.errors, [], 'Playwright reported errors outside individual tests');
  const tests = [];
  function visit(suite) {
    for (const spec of suite.specs ?? []) {
      assert.equal(typeof spec.id, 'string', 'Missing Playwright test ID');
      for (const test of spec.tests) {
        // Blob merges omit projectId; projectName is present in both formats.
        tests.push({ key: JSON.stringify([spec.id, test.projectName ?? '']), spec, test });
      }
    }
    for (const child of suite.suites ?? []) visit(child);
  }
  for (const suite of report.suites) visit(suite);
  return tests;
}

function verifyExecutions(expected, reports, label) {
  const seen = new Set();
  for (const report of reports) {
    for (const { key, spec, test } of testsIn(report)) {
      assert.ok(expected.has(key), `${label}: unexpected test ${spec.title} (${key})`);
      assert.ok(!seen.has(key), `${label}: duplicate test ${spec.title} (${key})`);
      seen.add(key);
      assert.equal(test.expectedStatus, 'passed', `${label}: test must run: ${spec.title}`);
      assert.equal(test.status, 'expected', `${label}: test did not pass: ${spec.title}`);
      assert.ok(spec.ok, `${label}: failing test: ${spec.title}`);
      assert.ok(test.results.length > 0, `${label}: test was not executed: ${spec.title}`);
      assert.equal(test.results.length, 1, `${label}: duplicate executions: ${spec.title}`);
      for (const result of test.results) {
        assert.equal(result.status, 'passed', `${label}: failed, skipped or interrupted execution: ${spec.title}`);
      }
    }
  }
  const missing = [...expected.keys()].filter(key => !seen.has(key));
  assert.equal(missing.length, 0, `${label}: missing ${missing.length} test(s): ${missing.map(key => expected.get(key)).join(', ')}`);
  return seen.size;
}

export function verifyBrowserCoverage(list, reports, { shardCount = 4, merged } = {}) {
  assert.equal(list.config.shard, null, 'The expected test list must be unsharded');
  const expected = new Map();
  for (const { key, spec } of testsIn(list)) {
    assert.ok(!expected.has(key), `Duplicate expected test ID: ${key}`);
    expected.set(key, spec.title);
  }
  assert.ok(expected.size > 0, 'The expected test list is empty');
  assert.equal(reports.length, shardCount, `Missing shard report: require ${shardCount}`);
  const shards = new Set();
  for (const report of reports) {
    const shard = report.config.shard;
    assert.equal(shard?.total, shardCount, 'Incorrect shard total');
    assert.ok(Number.isInteger(shard.current) && shard.current >= 1 && shard.current <= shardCount, 'Invalid shard index');
    assert.ok(!shards.has(shard.current), `Duplicate shard ${shard.current}`);
    shards.add(shard.current);
  }
  const count = verifyExecutions(expected, reports, 'Shard reports');
  if (merged) verifyExecutions(expected, [merged], 'Merged blob report');
  return { tests: count, shards: shards.size };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    const [listPath, reportsPath, shardCount = '4'] = process.argv.slice(2);
    assert.ok(listPath && reportsPath, 'Usage: verify-browser-coverage.mjs <test-list.json> <reports-directory> [shard-count]');
    const json = async file => JSON.parse(await readFile(file, 'utf8'));
    const names = (await readdir(reportsPath)).filter(name => /^shard-\d+\.json$/.test(name)).sort();
    const result = verifyBrowserCoverage(await json(listPath), await Promise.all(names.map(name => json(path.join(reportsPath, name)))), {
      shardCount: Number(shardCount), merged: await json(path.join(reportsPath, 'merged.json')),
    });
    console.log(`Verified all ${result.tests} browser tests passed exactly once across ${result.shards} shards and the merged blob report.`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
