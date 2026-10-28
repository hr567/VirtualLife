import init, { Engine, FlicDecoder, decodeBmp } from './wasm/virtual_life_core.js';
import type { WorkerReply, WorkerRequest } from './types';

let engine: Engine | undefined;
const initialized = init();
let decoderSerial = 0;
const flics = new Map<number, { decoder: FlicDecoder; key: number }>();
function applyColorKey(pixels: Uint8Array<ArrayBuffer>, indices: Uint8Array, key: number): void {
  for (let i = 0; i < indices.length; i++) if (indices[i] === key) pixels[i * 4 + 3] = 0;
}
const worker = self as unknown as { postMessage(message: WorkerReply, transfer?: Transferable[]): void; onmessage: ((message: MessageEvent<WorkerRequest>) => void) | null };

worker.onmessage = async ({ data }) => {
  try {
    await initialized;
    let value: unknown;
    const transfers: Transferable[] = [];
    if (data.type === 'init') {
      engine?.free();
      engine = new Engine(String(data.payload));
      value = JSON.parse(engine.snapshot());
    } else if (data.type === 'decodeBmp') {
      const payload = data.payload as { bytes: ArrayBuffer; transparentPaletteIndex?: number };
      const decoded = decodeBmp(new Uint8Array(payload.bytes));
      const pixels = new Uint8Array(decoded.pixels());
      const decodedIndices = decoded.indices();
      const paletteIndices = decodedIndices ? new Uint8Array(decodedIndices) : undefined;
      if (payload.transparentPaletteIndex !== undefined) {
        if (!paletteIndices) throw new Error('此透明位图没有原版调色板索引');
        applyColorKey(pixels, paletteIndices, payload.transparentPaletteIndex);
      }
      value = { width: decoded.width, height: decoded.height, pixels, paletteIndices };
      transfers.push(pixels.buffer);
      if (paletteIndices) transfers.push(paletteIndices.buffer);
      decoded.free();
    } else if (data.type === 'openFlic') {
      const payload = data.payload as { bytes: ArrayBuffer; transparentPaletteIndex?: number };
      const decoder = new FlicDecoder(new Uint8Array(payload.bytes));
      const pixels = new Uint8Array(decoder.nextFrame());
      const indices = new Uint8Array(decoder.indices());
      const key = payload.transparentPaletteIndex ?? indices[0]!;
      applyColorKey(pixels, indices, key);
      const decoderId = ++decoderSerial;
      flics.set(decoderId, { decoder, key });
      value = { decoderId, pixels, paletteIndices: indices, width: decoder.width, height: decoder.height, frameCount: decoder.frameCount, frameDelayMs: decoder.frameDelayMs };
      transfers.push(pixels.buffer, indices.buffer);
    } else if (data.type === 'nextFlic') {
      const item = flics.get(Number(data.payload));
      if (!item) throw new Error('动画已经关闭');
      const pixels = new Uint8Array(item.decoder.nextFrame());
      const indices = new Uint8Array(item.decoder.indices());
      applyColorKey(pixels, indices, item.key);
      value = { pixels, paletteIndices: indices, width: item.decoder.width, height: item.decoder.height };
      transfers.push(pixels.buffer, indices.buffer);
    } else if (data.type === 'closeFlic') {
      const id = Number(data.payload);
      flics.get(id)?.decoder.free();
      flics.delete(id);
    } else {
      if (!engine) throw new Error('游戏引擎尚未初始化');
      switch (data.type) {
        case 'dispatch': value = JSON.parse(engine.dispatch(JSON.stringify(data.payload))); break;
        case 'snapshot': value = JSON.parse(engine.snapshot()); break;
        case 'serialize': value = engine.serialize(); break;
        case 'restore': value = JSON.parse(engine.restore(String(data.payload))); break;
      }
    }
    worker.postMessage({ id: data.id, ok: true, value }, transfers);
  } catch (error) {
    worker.postMessage({ id: data.id, ok: false, error: error instanceof Error ? error.message : String(error) });
  }
};
