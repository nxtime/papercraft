import { test, expect, PHONE, VIEW } from '../lib/fixtures.mjs';
import { findDeep } from '../lib/mcp.mjs';
import { diffFraction, colourCount } from '../lib/pixels.mjs';
import { setTimeout as sleep } from 'node:timers/promises';

const S = 3; // deviceScaleFactor: screenshot px per CSS px
const BAR_H = 64; // CSS px of the bottom tab bar
const TABS = ['Canvas', 'Layers', 'Properties', 'Chat', 'Pages'];
const tabCentre = (name) => [(VIEW.width * (TABS.indexOf(name) + 0.5)) / TABS.length, VIEW.height - BAR_H / 2];

// The phone UI is not on this base. Those tests are marked test.fail (they run and must fail) until
// the mobile-ui branch is merged; PAPERCRAFT_E2E_PHONE=1 removes the mark so they must pass.
const phone = (info) => info.fail(!PHONE, 'needs the mobile-ui branch (run with PAPERCRAFT_E2E_PHONE=1 once merged)');

/** A 200x200 red layer named "Box" on a 400x400 artboard; returns its id. */
async function makeBox(mcp) {
  const ab = await mcp.call('create_artboard', { name: 'Phone', styles: { width: '400px', height: '400px' } });
  const abId = findDeep(ab, (o) => typeof o.id === 'string')?.id ?? ab.nodeId ?? ab.id;
  await mcp.call('write_html', { mode: 'insert-children', targetNodeId: abId,
    html: '<div layer-name="Box" style="width:200px;height:200px;background:#e53935"></div>' });
  const kids = await mcp.call('get_children', { nodeId: abId });
  const box = findDeep(kids, (o) => o.name === 'Box' && typeof o.id === 'string');
  if (!box) throw new Error('Box layer not found: ' + JSON.stringify(kids).slice(0, 500));
  return box.id;
}

/** Screen position (CSS px) of a node's centre, from its world box and the reported viewport. */
async function boxCentre(mcp, id, viewport) {
  const n = findDeep(await mcp.call('get_node_info', { nodeId: id }), (o) => 'worldX' in o);
  const vp = await viewport();
  const [rx, ry] = vp.rect;
  return { n, x: (n.worldX + n.width / 2 - rx) * vp.zoom, y: (n.worldY + n.height / 2 - ry) * vp.zoom };
}

const frameBox = (mcp, id) => mcp.call('set_viewport', { nodeIds: [id], zoom: 1, force: true });

test.describe('harness (runs on the current base)', () => {
  test('1a. loads, canvas fills the viewport, no horizontal scroll', async ({ app }) => {
    await app.shot('01-load');
    const m = await app.page.evaluate(() => {
      const c = document.querySelector('canvas#papercraft').getBoundingClientRect();
      const de = document.documentElement;
      return { w: c.width, h: c.height, sw: de.scrollWidth, cw: de.clientWidth };
    });
    expect(m.w).toBe(VIEW.width);
    expect(m.h).toBe(VIEW.height);
    expect(m.sw).toBeLessThanOrEqual(m.cw);
    expect(app.errors).toEqual([]);
  });

  test('MCP link reaches the tab', async ({ app }) => {
    const info = await app.mcp.call('get_basic_info');
    expect(findDeep(info, (o) => Array.isArray(o.rect) && 'zoom' in o)).toBeTruthy();
  });

  test('8. manifest is served and valid', async ({ relay, request }, info) => {
    phone(info);
    const res = await request.get(relay.origin + '/manifest.webmanifest');
    expect(res.status()).toBe(200);
    const m = JSON.parse(await res.text());
    expect(m.name || m.short_name).toBeTruthy();
    expect(m.start_url).toBeTruthy();
    expect(['standalone', 'fullscreen', 'minimal-ui']).toContain(m.display);
    expect(m.icons?.length).toBeGreaterThan(0);
    for (const i of m.icons) expect((await request.get(new URL(i.src, relay.origin + '/').href)).status()).toBe(200);
    expect(await (await request.get(relay.origin + '/')).text()).toMatch(/<link[^>]+rel="manifest"/);
  });

  test('4. a tap selects a layer created through MCP (works on the base)', async ({ app }) => {
    const { mcp, touch, viewport, shot } = app;
    const id = await makeBox(mcp);
    await frameBox(mcp, id);
    await mcp.call('select_nodes', { nodeIds: [] });
    const c = await boxCentre(mcp, id, viewport);
    await touch.tap([[c.x, c.y]]);
    await shot('04-tap-select');
    expect(findDeep(await mcp.call('get_selection'), (o) => o.id === id)).toBeTruthy();
  });
});

test.describe('phone UI (expected to fail until mobile-ui is merged)', () => {
  test.beforeEach(({}, info) => phone(info));

  test('1b. a bottom bar is visible', async ({ app }) => {
    await sleep(1000);
    const buf = await app.shot('01-bottom-bar');
    const bar = { x: 0, y: (VIEW.height - BAR_H) * S, w: VIEW.width * S, h: BAR_H * S };
    // Five icons and labels: far more colours than a flat canvas strip, and more than the canvas above it.
    expect(colourCount(buf, bar)).toBeGreaterThan(12);
    expect(colourCount(buf, { ...bar, y: bar.y - bar.h })).toBeLessThan(colourCount(buf, bar));
    // The desktop build also has a floating tool strip down there, so the bar must also work: Layers opens a sheet.
    await app.touch.tap([tabCentre('Layers')]);
    await sleep(500);
    const mid = { x: 0, y: 400 * S, w: VIEW.width * S, h: 200 * S };
    expect(diffFraction(buf, await app.shot('01-layers-sheet'), mid)).toBeGreaterThan(0.2);
  });

  test('2. pinch zoom changes the zoom', async ({ app }) => {
    const { touch, viewport, shot } = app;
    const before = (await viewport()).zoom;
    const cx = VIEW.width / 2, cy = 300;
    await touch.gesture([[cx - 40, cy], [cx + 40, cy]], [[cx - 130, cy], [cx + 130, cy]]);
    await shot('02-pinch');
    expect((await viewport()).zoom).toBeGreaterThan(before * 1.5);
  });

  test('3. a two-finger pan moves the view', async ({ app }) => {
    const { touch, viewport, shot } = app;
    const before = await viewport();
    await touch.gesture([[150, 300], [240, 300]], [[50, 200], [140, 200]]);
    await shot('03-pan');
    const after = await viewport();
    expect(Math.abs(after.rect[0] - before.rect[0]) + Math.abs(after.rect[1] - before.rect[1])).toBeGreaterThan(20);
    expect(after.zoom).toBeCloseTo(before.zoom, 1);
  });

  test('5. a drag moves the layer; a two-finger tap undoes', async ({ app }) => {
    const { mcp, touch, viewport, shot } = app;
    const id = await makeBox(mcp);
    await frameBox(mcp, id);
    const c = await boxCentre(mcp, id, viewport);
    await touch.tap([[c.x, c.y]]);
    await touch.gesture([[c.x, c.y]], [[c.x + 60, c.y + 40]]);
    await shot('05-drag');
    expect((await boxCentre(mcp, id, viewport)).n.worldX).toBeGreaterThan(c.n.worldX + 20);
    await touch.tap([[60, 300], [330, 300]]);
    await shot('05-undo');
    expect((await boxCentre(mcp, id, viewport)).n.worldX).toBeCloseTo(c.n.worldX, 0);
  });

  test('6. a long-press opens the context menu', async ({ app }) => {
    const { mcp, touch, viewport, shot } = app;
    const id = await makeBox(mcp);
    await frameBox(mcp, id);
    const c = await boxCentre(mcp, id, viewport);
    const before = await shot('06-before');
    await touch.longPress(c.x, c.y);
    await sleep(300);
    const after = await shot('06-context-menu');
    const x = Math.max(0, Math.round((c.x - 20) * S)), y = Math.max(0, Math.round((c.y - 20) * S));
    const rect = { x, y, w: Math.min(200 * S, VIEW.width * S - x), h: Math.min(260 * S, VIEW.height * S - y) };
    expect(diffFraction(before, after, rect)).toBeGreaterThan(0.05);
  });

  test('7. the Properties sheet opens and a drag down closes it', async ({ app }) => {
    const { mcp, touch, viewport, shot } = app;
    const id = await makeBox(mcp);
    await frameBox(mcp, id);
    const c = await boxCentre(mcp, id, viewport);
    await touch.tap([[c.x, c.y]]);
    const closed = await shot('07-closed');
    await touch.tap([tabCentre('Properties')]);
    await sleep(500);
    const open = await shot('07-open');
    const mid = { x: 0, y: 400 * S, w: VIEW.width * S, h: 200 * S };
    expect(diffFraction(closed, open, mid)).toBeGreaterThan(0.2);
    await touch.gesture([[VIEW.width / 2, 470]], [[VIEW.width / 2, 830]], { steps: 15 });
    await sleep(500);
    expect(diffFraction(closed, await shot('07-dragged-down'), mid)).toBeLessThan(0.05);
  });

  // TODO(server-agent): enable once that branch is merged. Needs a fake agent on the relay (a
  // stand-in for `--agents claude,opencode`) and its relay_proto chat frames; the test then taps the
  // Chat tab, types in the on-screen field, sends, and expects the reply region to change.
  test.skip('9. the Chat tab sends a message and a reply appears', async () => {});
});
