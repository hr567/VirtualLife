import test from 'node:test';
import assert from 'node:assert/strict';
import { chooseDisplay } from '../web/display.js';

test('display selection falls back for missing, rejected and unavailable GPU adapters', async () => {
  assert.equal(await chooseDisplay(null), 'canvas2d');
  assert.equal(await chooseDisplay({ requestAdapter: async () => null }), 'canvas2d');
  assert.equal(await chooseDisplay({ requestAdapter: async () => { throw new Error('device unavailable'); } }), 'canvas2d');
});
test('display selection keeps a usable GPU and bounds an unresponsive adapter', async () => {
  assert.equal(await chooseDisplay({ requestAdapter: async () => ({}) }), 'webgpu');
  assert.equal(await chooseDisplay({ requestAdapter: () => new Promise(() => {}) }, 1), 'canvas2d');
});
