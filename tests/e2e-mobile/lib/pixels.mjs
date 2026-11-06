import { PNG } from 'pngjs';

export const decode = (buf) => PNG.sync.read(buf);

/** Fraction of pixels (0..1) in the rect (image px) whose colour differs noticeably between two PNGs. */
export function diffFraction(a, b, rect) {
  const A = decode(a), B = decode(b);
  const { x = 0, y = 0, w = A.width, h = A.height } = rect || {};
  let n = 0;
  for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) {
    const k = (j * A.width + i) * 4;
    if (Math.abs(A.data[k] - B.data[k]) + Math.abs(A.data[k + 1] - B.data[k + 1]) + Math.abs(A.data[k + 2] - B.data[k + 2]) > 24) n++;
  }
  return n / (w * h);
}

/** Distinct colours (4 bits per channel) in a rect: a flat strip has 1-2, a tab bar with icons many more. */
export function colourCount(buf, rect) {
  const P = decode(buf);
  const { x = 0, y = 0, w = P.width, h = P.height } = rect || {};
  const seen = new Set();
  for (let j = y; j < y + h; j++) for (let i = x; i < x + w; i++) {
    const k = (j * P.width + i) * 4;
    seen.add(((P.data[k] >> 4) << 8) | ((P.data[k + 1] >> 4) << 4) | (P.data[k + 2] >> 4));
  }
  return seen.size;
}
