import { test, expect } from '../lib/fixtures.mjs';
import { findDeep } from '../lib/mcp.mjs';
import { decode } from '../lib/pixels.mjs';
import { setTimeout as sleep } from 'node:timers/promises';

const S = 3; // screenshot px per CSS px
const AB = 400; // artboard size

const box = (css) => `<div style="position:absolute;${css}"></div>`;
// Four panels on a white artboard; every probe below is in artboard px.
const HTML = [
  // A: rounded clipping frame (r=60), red child fills the square so the corner must be cut away.
  `<div style="position:absolute;left:20px;top:20px;width:120px;height:120px;border-radius:60px;overflow:hidden;background:#0000ff">${box('left:0;top:0;width:120px;height:120px;background:#ff0000')}</div>`,
  // B: layer blur on a black square with hard edges.
  `<div style="position:absolute;left:200px;top:20px;width:120px;height:120px;filter:blur(10px)">${box('left:20px;top:20px;width:80px;height:80px;background:#000000')}</div>`,
  // C: red shape with a hard right edge at x=140, glass with background blur over it.
  box('left:20px;top:200px;width:120px;height:120px;background:#ff0000'),
  box('left:60px;top:200px;width:120px;height:120px;background:rgba(255,255,255,0.1);backdrop-filter:blur(10px)'),
  // D: multiply of red over grey.
  box('left:220px;top:200px;width:120px;height:120px;background:#808080'),
  box('left:260px;top:240px;width:120px;height:120px;background:#ff0000;mix-blend-mode:multiply'),
].join('');

test('canvas effects render: rounded clip, layer blur, background blur, blend', async ({ app }) => {
  const { mcp, page } = app;
  const ab = await mcp.call('create_artboard', { name: 'FX', styles: { width: `${AB}px`, height: `${AB}px`, background: '#ffffff' } });
  const abId = findDeep(ab, (o) => typeof o.id === 'string')?.id ?? ab.nodeId ?? ab.id;
  await mcp.call('write_html', { mode: 'insert-children', targetNodeId: abId, html: HTML });
  await mcp.call('select_nodes', { nodeIds: [] });
  await mcp.call('set_viewport', { nodeIds: [abId], zoom: 1, force: true });
  await sleep(1500);

  const Z = 0.9; // the artboard is 400px wide, the phone viewport 390
  await mcp.call('set_viewport', { nodeIds: [abId], zoom: Z, force: true });
  await sleep(1000);
  const png = decode(await page.screenshot({ path: `${process.env.PAPERCRAFT_E2E_SHOTS || 'D:/Projects/papercraft-wt/mobile-shots/e2e'}/render-fx${process.env.PAPERCRAFT_E2E_GL === '1' ? '-gl' : ''}.png` }));
  const at = (cx, cy) => { const k = (cy * png.width + cx) * 4; return { r: png.data[k], g: png.data[k + 1], b: png.data[k + 2] }; };
  const white = (c) => c.r === 255 && c.g === 255 && c.b === 255;
  // The phone chrome moves the artboard, so find its top-left corner as the first pure-white pixels.
  const midX = Math.round(png.width / 2);
  let top = 0;
  while (top < png.height && !white(at(midX, top))) top++;
  let left = midX;
  while (left > 0 && white(at(left - 1, top + 4))) left--;
  expect(top, 'artboard found').toBeLessThan(png.height - 100);
  const px = (x, y) => at(Math.round(left + x * Z * S), Math.round(top + y * Z * S));
  const near = (c, [r, g, b], tol = 12) => Math.abs(c.r - r) <= tol && Math.abs(c.g - g) <= tol && Math.abs(c.b - b) <= tol;

  expect(near(px(100, 170), [255, 255, 255]), 'artboard background').toBe(true);
  // A: the corner is clipped away (white), the middle is the red child.
  expect(near(px(24, 24), [255, 255, 255]), `corner clipped ${JSON.stringify(px(24, 24))}`).toBe(true);
  expect(near(px(80, 80), [255, 0, 0]), 'child inside the clip').toBe(true);
  // B: centre solid black; a pixel just inside the hard edge (x=220 is the edge) is softened.
  expect(near(px(260, 80), [0, 0, 0], 20), 'blurred centre still dark').toBe(true);
  const edge = px(222, 80);
  expect(edge.r, `edge softened ${JSON.stringify(edge)}`).toBeGreaterThan(40);
  expect(px(210, 80).r, 'blur spreads outside the edge').toBeLessThan(250);
  expect(px(210, 80).r, 'blur spreads outside the edge').toBeGreaterThan(0);
  // C: red is hard-edged at x=140; the glass (x 60..180) blurs what is behind it.
  const inside = px(136, 260);
  expect(inside.g, `backdrop blurred ${JSON.stringify(inside)}`).toBeGreaterThan(60);
  expect(near(px(30, 260), [255, 0, 0]), 'unblurred red outside the glass').toBe(true);
  // D: multiply over grey 128 gives dark red; over white it stays red.
  const mul = px(290, 270);
  expect(near(mul, [128, 0, 0], 16), `multiply ${JSON.stringify(mul)}`).toBe(true);
  expect(near(px(360, 350), [255, 0, 0]), 'multiply over white stays red').toBe(true);

  // getContext returns the existing context only when the canvas already has that type.
  const kind = await page.evaluate(() => { const c = document.querySelector('canvas#papercraft'); return c.getContext('webgl2') ? 'webgl2' : c.getContext('webgpu') ? 'webgpu' : 'other'; });
  expect(kind).toBe(process.env.PAPERCRAFT_E2E_GL === '1' ? 'webgl2' : 'webgpu');

  expect(app.errors).toEqual([]);
  expect(app.consoleErrors).toEqual([]);
});
