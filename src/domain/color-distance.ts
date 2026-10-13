export type Lab = readonly [number, number, number];

/** sRGB → CIELAB, D65 reference white. */
export function hexToLab(hex: string): Lab {
  const [r, g, b] = hex.slice(1).match(/../g)!.map(value => parseInt(value, 16) / 255)
    .map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4);
  const f = (value: number) => value > 216 / 24389 ? Math.cbrt(value) : (24389 / 27 * value + 16) / 116;
  const x = f((.4124564 * r + .3575761 * g + .1804375 * b) / .95047);
  const y = f(.2126729 * r + .7151522 * g + .072175 * b);
  const z = f((.0193339 * r + .119192 * g + .9503041 * b) / 1.08883);
  return [116 * y - 16, 500 * (x - y), 200 * (y - z)];
}

/** CIEDE2000, kL = kC = kH = 1. Sharma/Wu/Dalal (2005), DOI 10.1002/col.20070. */
export function deltaE2000([l1, a1, b1]: Lab, [l2, a2, b2]: Lab): number {
  const radians = (degrees: number) => degrees * Math.PI / 180;
  const cos = (degrees: number) => Math.cos(radians(degrees));
  const sin = (degrees: number) => Math.sin(radians(degrees));
  const c = (Math.hypot(a1, b1) + Math.hypot(a2, b2)) / 2;
  const g = .5 * (1 - Math.sqrt(c ** 7 / (c ** 7 + 25 ** 7)));
  const ap1 = (1 + g) * a1, ap2 = (1 + g) * a2;
  const cp1 = Math.hypot(ap1, b1), cp2 = Math.hypot(ap2, b2);
  const hue = (a: number, b: number) => a === 0 && b === 0 ? 0 : (Math.atan2(b, a) * 180 / Math.PI + 360) % 360;
  const h1 = hue(ap1, b1), h2 = hue(ap2, b2);
  const dl = l2 - l1, dc = cp2 - cp1;
  let dh = h2 - h1;
  if (cp1 * cp2 === 0) dh = 0;
  else if (dh > 180) dh -= 360;
  else if (dh < -180) dh += 360;
  const dH = 2 * Math.sqrt(cp1 * cp2) * sin(dh / 2);
  const lm = (l1 + l2) / 2, cm = (cp1 + cp2) / 2;
  let hm = h1 + h2;
  if (cp1 * cp2 !== 0) {
    if (Math.abs(h1 - h2) <= 180) hm /= 2;
    else hm = (hm + (hm < 360 ? 360 : -360)) / 2;
  }
  const t = 1 - .17 * cos(hm - 30) + .24 * cos(2 * hm) + .32 * cos(3 * hm + 6) - .20 * cos(4 * hm - 63);
  const sl = 1 + .015 * (lm - 50) ** 2 / Math.sqrt(20 + (lm - 50) ** 2);
  const sc = 1 + .045 * cm, sh = 1 + .015 * cm * t;
  const rt = -2 * Math.sqrt(cm ** 7 / (cm ** 7 + 25 ** 7)) * sin(60 * Math.exp(-(((hm - 275) / 25) ** 2)));
  const vl = dl / sl, vc = dc / sc, vh = dH / sh;
  return Math.sqrt(vl ** 2 + vc ** 2 + vh ** 2 + rt * vc * vh);
}
