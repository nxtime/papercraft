// Multi-touch through CDP Input.dispatchTouchEvent (Playwright's touchscreen only taps).
const pt = (id, x, y) => ({ x, y, id, radiusX: 6, radiusY: 6, force: 1 });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function touchSession(page) {
  const cdp = await page.context().newCDPSession(page);
  const send = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
  return {
    /** Moves N fingers together from `from` points to `to` points in `steps` steps. */
    async gesture(from, to, { steps = 12, hold = 0, stepMs = 16 } = {}) {
      await send('touchStart', from.map(([x, y], i) => pt(i, x, y)));
      if (hold) await sleep(hold);
      for (let s = 1; s <= steps; s++) {
        const t = s / steps;
        await send('touchMove', from.map(([x, y], i) => pt(i, x + (to[i][0] - x) * t, y + (to[i][1] - y) * t)));
        await sleep(stepMs);
      }
      await send('touchEnd', []);
      await sleep(120);
    },
    /** A quick tap with N fingers at the given points. */
    async tap(points, holdMs = 50) {
      await send('touchStart', points.map(([x, y], i) => pt(i, x, y)));
      await sleep(holdMs);
      await send('touchEnd', []);
      await sleep(150);
    },
    longPress: (x, y, ms = 800) => touchSessionHold(send, x, y, ms),
    close: () => cdp.detach(),
  };
}

async function touchSessionHold(send, x, y, ms) {
  await send('touchStart', [pt(0, x, y)]);
  await sleep(ms);
  await send('touchEnd', []);
  await sleep(150);
}
