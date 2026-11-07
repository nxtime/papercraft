import { test as base, expect } from '@playwright/test';
import { startRelay } from './relay.mjs';
import { connectMcp, findDeep } from './mcp.mjs';
import { touchSession } from './touch.mjs';
import { mkdirSync } from 'node:fs';
import { randomBytes } from 'node:crypto';

export const SHOTS = process.env.PAPERCRAFT_E2E_SHOTS || 'D:/Projects/papercraft-wt/mobile-shots/e2e';
export const PHONE = process.env.PAPERCRAFT_E2E_PHONE === '1';
export const VIEW = { width: 390, height: 844 };
mkdirSync(SHOTS, { recursive: true });

export const test = base.extend({
  relay: [async ({}, use) => {
    const relay = await startRelay();
    await use(relay);
    await relay.stop();
  }, { scope: 'worker' }],

  // Phone-like context. A token pre-seeded in localStorage lets the test reach the tab over MCP.
  app: async ({ browser, relay }, use) => {
    const token = randomBytes(24).toString('hex');
    const context = await browser.newContext({ viewport: VIEW, deviceScaleFactor: 3, hasTouch: true, isMobile: true });
    // PAPERCRAFT_E2E_GL=1 hides WebGPU so the app falls back to WebGL2.
    if (process.env.PAPERCRAFT_E2E_GL === '1') await context.addInitScript(() => { Object.defineProperty(navigator, 'gpu', { value: undefined }); });
    await context.addInitScript((t) => { try { localStorage.setItem('papercraft.relay.token', t); } catch { /* storage blocked */ } }, token);
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    // Shader compile errors and wgpu validation failures ("Invalid CommandBuffer") are console errors.
    const consoleErrors = [];
    page.on('console', (m) => { if (m.type() === 'error' && !m.text().startsWith('Failed to load resource')) consoleErrors.push(m.text()); });
    await page.goto(relay.origin + '/');
    await page.waitForSelector('canvas#papercraft');
    const mcp = await connectMcp(relay.origin, token);
    // The tab connects to the relay a moment after the wasm starts, and shows the home screen:
    // get_basic_info only works once a project is open, so create and open one.
    let opened = false;
    await expect.poll(async () => {
      try {
        if (!opened) {
          const { fileId } = await mcp.call('create_file', { name: 'e2e' });
          await mcp.call('open_file', { fileId });
          opened = true;
        }
        await mcp.call('get_basic_info');
        return true;
      } catch { return false; }
    }, { timeout: 60_000, intervals: [500] }).toBe(true);
    const touch = await touchSession(page);
    const shot = (name) => page.screenshot({ path: `${SHOTS}/${name}.png` });
    const viewport = async () => findDeep(await mcp.call('get_basic_info'), (o) => Array.isArray(o.rect) && 'zoom' in o);
    await use({ page, mcp, touch, shot, viewport, errors, consoleErrors, token });
    await context.close();
  },
});
export { expect };
