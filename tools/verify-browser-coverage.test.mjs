import assert from 'node:assert/strict';
import { test } from 'node:test';
import { verifyBrowserCoverage } from './verify-browser-coverage.mjs';

function report(ids, shard = null, executed = true) {
  return {
    config: { shard }, errors: [],
    suites: [{ suites: [{ specs: ids.map(id => ({
      id, title: `test ${id}`, ok: true,
      tests: [{ projectId: '', projectName: '', expectedStatus: 'passed', status: executed ? 'expected' : 'skipped', results: executed ? [{ status: 'passed' }] : [] }],
    })) }] }],
  };
}

function fixture() {
  const list = report(['a', 'b', 'c', 'd'], null, false);
  const shards = ['a', 'b', 'c', 'd'].map((id, index) => report([id], { current: index + 1, total: 4 }));
  return { list, shards, merged: report(['a', 'b', 'c', 'd']) };
}

test('all expected tests must pass in both the individual shards and merged blobs', () => {
  const { list, shards, merged } = fixture();
  for (const spec of merged.suites[0].suites[0].specs) delete spec.tests[0].projectId;
  assert.deepEqual(verifyBrowserCoverage(list, shards, { merged }), { tests: 4, shards: 4 });
});

test('a missing test, duplicate test or unexpected test blocks deployment', () => {
  const { list, shards } = fixture();
  shards[0].suites = [];
  assert.throws(() => verifyBrowserCoverage(list, shards), /missing 1 test/);
  shards[0] = report(['b'], { current: 1, total: 4 });
  assert.throws(() => verifyBrowserCoverage(list, shards), /duplicate test/);
  shards[0] = report(['unknown'], { current: 1, total: 4 });
  assert.throws(() => verifyBrowserCoverage(list, shards), /unexpected test/);
});

test('missing, duplicated or incorrectly numbered shard reports block deployment', () => {
  const { list, shards } = fixture();
  assert.throws(() => verifyBrowserCoverage(list, shards.slice(0, 3)), /Missing shard/);
  shards[3].config.shard.current = 3;
  assert.throws(() => verifyBrowserCoverage(list, shards), /Duplicate shard/);
  shards[3].config.shard.current = 5;
  assert.throws(() => verifyBrowserCoverage(list, shards), /Invalid shard/);
  shards[3].config.shard = { current: 4, total: 5 };
  assert.throws(() => verifyBrowserCoverage(list, shards), /Incorrect shard total/);
});

test('listing a test without executing it does not satisfy coverage', () => {
  const { list, shards } = fixture();
  const execution = shards[0].suites[0].suites[0].specs[0].tests[0];
  execution.results = [];
  assert.throws(() => verifyBrowserCoverage(list, shards), /not executed/);
  for (const status of ['failed', 'timedOut', 'skipped', 'interrupted']) {
    execution.results = [{ status }];
    assert.throws(() => verifyBrowserCoverage(list, shards), /failed, skipped or interrupted/);
  }
  execution.expectedStatus = 'skipped';
  assert.throws(() => verifyBrowserCoverage(list, shards), /test must run/);
});

test('global Playwright errors and incomplete blob merges block deployment', () => {
  const { list, shards, merged } = fixture();
  shards[0].errors.push({ message: 'worker crashed' });
  assert.throws(() => verifyBrowserCoverage(list, shards, { merged }), /outside individual tests/);
  shards[0].errors = [];
  const results = merged.suites[0].suites[0].specs[0].tests[0].results;
  results.push({ status: 'passed' });
  assert.throws(() => verifyBrowserCoverage(list, shards, { merged }), /duplicate executions/);
  results.pop();
  merged.suites[0].suites[0].specs.pop();
  assert.throws(() => verifyBrowserCoverage(list, shards, { merged }), /Merged blob report: missing/);
});
