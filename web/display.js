/** Presents Bevy's original 640x480 RGBA buffer; no game drawing rules live here. */
export async function chooseDisplay(gpu = globalThis.navigator?.gpu, timeoutMs = 3000) {
  let timeout;
  try {
    const adapter = await Promise.race([
      gpu?.requestAdapter({ powerPreference: 'low-power' }),
      new Promise(resolve => { timeout = setTimeout(() => resolve(null), timeoutMs); }),
    ]);
    return adapter ? 'webgpu' : 'canvas2d';
  } catch {
    return 'canvas2d';
  } finally {
    clearTimeout(timeout);
  }
}

export function useCanvas2d() {
  return document.querySelector('#game-canvas')?.dataset.renderer === 'canvas2d';
}

let presenter;
export function presentCanvas2d(bytes) {
  if (!presenter) {
    const canvas = document.querySelector('#game-canvas');
    const context = canvas?.getContext('2d', { alpha: false });
    if (!context) throw new Error('无法创建 Canvas 2D 显示设备。');
    const source = document.createElement('canvas');
    source.width = 640; source.height = 480;
    const sourceContext = source.getContext('2d', { alpha: false });
    if (!sourceContext) throw new Error('无法创建原版画面缓冲。');
    let cssWidth = 640, cssHeight = 480;
    const redraw = () => {
      context.imageSmoothingEnabled = false;
      context.fillStyle = '#000'; context.fillRect(0, 0, canvas.width, canvas.height);
      const scale = Math.min(cssWidth / 640, cssHeight / 480);
      // Project CSS geometry into the integer backing store on each axis.
      // This keeps fractional CSS sizes aligned with the physical input grid.
      const x = canvas.width / cssWidth, y = canvas.height / cssHeight;
      if (scale > 0) context.drawImage(source, (cssWidth - 640 * scale) / 2 * x,
        (cssHeight - 480 * scale) / 2 * y, 640 * scale * x, 480 * scale * y);
    };
    const resize = () => {
      const bounds = canvas.getBoundingClientRect();
      if (bounds.width <= 0 || bounds.height <= 0) return;
      cssWidth = bounds.width; cssHeight = bounds.height;
      // Chrome's emulated DPR can disagree with devicePixelContentBoxSize.
      // Use the same logical rectangle and DPR as the browser's input events.
      const width = Math.max(1, Math.round(bounds.width * devicePixelRatio));
      const height = Math.max(1, Math.round(bounds.height * devicePixelRatio));
      if (canvas.width !== width) canvas.width = width;
      if (canvas.height !== height) canvas.height = height;
      redraw();
    };
    // Winit reports the window's new physical size; with no GPU surface there
    // is no renderer to resize its canvas backing store for us.
    const resizeObserver = new ResizeObserver(resize);
    try { resizeObserver.observe(canvas, { box: 'device-pixel-content-box' }); }
    catch { resizeObserver.observe(canvas); }
    const observer = new MutationObserver(redraw);
    observer.observe(canvas, { attributes: true, attributeFilter: ['width', 'height'] });
    presenter = { sourceContext, redraw, observer, resizeObserver };
    resize();
  }
  if (bytes.byteLength !== 640 * 480 * 4) throw new Error('原版画面缓冲尺寸错误。');
  const rgba = new Uint8ClampedArray(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  presenter.sourceContext.putImageData(new ImageData(rgba, 640, 480), 0, 0);
  presenter.redraw();
}
