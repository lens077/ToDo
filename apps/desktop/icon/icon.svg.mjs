// The app icon as SVG, in the product's own palette (docs/desktop-mvp-spec.md §2).
//
// Artwork: a task ledger. Three rows of checkbox + line; two open rows in
// cobalt, the last one ticked in moss. The same square box and moss fill the
// board's cards use, so the Dock and the window agree.

const INK = '#1C2733';
const COBALT = '#275EFE';
const MOSS = '#648C72';
const PAPER_LIGHT = '#FBF9F2';
const PAPER_DEEP = '#ECE7DA';

/**
 * Apple-style continuous corner ("squircle") path, the algorithm Figma uses
 * for corner smoothing. `smoothing` 0 is a plain rounded rectangle; Apple's
 * Big Sur template is well matched by radius ≈ 22.5% of the side and 0.6.
 */
export function squirclePath(x, y, size, radius, smoothing = 0.6) {
  const rad = (deg) => (deg * Math.PI) / 180;
  const p = Math.min((1 + smoothing) * radius, size / 2);
  const s = p / radius - 1;
  const arcMeasure = 90 * (1 - s);
  const arc = Math.sin(rad(arcMeasure / 2)) * radius * Math.SQRT2;
  const alpha = (90 - arcMeasure) / 2;
  const p3p4 = radius * Math.tan(rad(alpha / 2));
  const beta = 45 * s;
  const c = p3p4 * Math.cos(rad(beta));
  const d = c * Math.tan(rad(beta));
  const b = (p - arc - c - d) / 3;
  const a = 2 * b;
  const r = radius;
  const n = (v) => Number(v.toFixed(3));
  const w = size;
  return [
    `M ${n(x + w - p)} ${n(y)}`,
    `c ${n(a)} 0 ${n(a + b)} 0 ${n(a + b + c)} ${n(d)}`,
    `a ${r} ${r} 0 0 1 ${n(arc)} ${n(arc)}`,
    `c ${n(d)} ${n(c)} ${n(d)} ${n(b + c)} ${n(d)} ${n(a + b + c)}`,
    `L ${n(x + w)} ${n(y + w - p)}`,
    `c 0 ${n(a)} 0 ${n(a + b)} ${n(-d)} ${n(a + b + c)}`,
    `a ${r} ${r} 0 0 1 ${n(-arc)} ${n(arc)}`,
    `c ${n(-c)} ${n(d)} ${n(-(b + c))} ${n(d)} ${n(-(a + b + c))} ${n(d)}`,
    `L ${n(x + p)} ${n(y + w)}`,
    `c ${n(-a)} 0 ${n(-(a + b))} 0 ${n(-(a + b + c))} ${n(-d)}`,
    `a ${r} ${r} 0 0 1 ${n(-arc)} ${n(-arc)}`,
    `c ${n(-d)} ${n(-c)} ${n(-d)} ${n(-(b + c))} ${n(-d)} ${n(-(a + b + c))}`,
    `L ${n(x)} ${n(y + p)}`,
    `c 0 ${n(-a)} 0 ${n(-(a + b))} ${n(d)} ${n(-(a + b + c))}`,
    `a ${r} ${r} 0 0 1 ${n(arc)} ${n(-arc)}`,
    `c ${n(c)} ${n(-d)} ${n(b + c)} ${n(-d)} ${n(a + b + c)} ${n(-d)}`,
    'Z',
  ].join(' ');
}

/** The ledger rows, drawn in a `face`-sized square starting at (ox, oy). */
function ledger(ox, oy, face) {
  const u = face / 824; // design units: the artwork was laid out on the 824 grid
  const box = 104 * u;
  const stroke = 18 * u;
  const bar = 64 * u;
  const rows = [
    { y: 226, tone: COBALT, width: 316, done: false },
    { y: 386, tone: COBALT, width: 246, done: false },
    { y: 546, tone: MOSS, width: 176, done: true },
  ];
  const boxX = ox + 186 * u;
  const barX = ox + 336 * u;
  return rows
    .map(({ y, tone, width, done }) => {
      const top = oy + y * u;
      const boxRect = done
        ? `<rect x="${boxX}" y="${top}" width="${box}" height="${box}" rx="${12 * u}" fill="${MOSS}"/>
           <path d="M ${boxX + 26 * u} ${top + 54 * u} l ${22 * u} ${22 * u} l ${34 * u} ${-40 * u}" fill="none" stroke="${PAPER_LIGHT}" stroke-width="${16 * u}" stroke-linecap="round" stroke-linejoin="round"/>`
        : `<rect x="${boxX + stroke / 2}" y="${top + stroke / 2}" width="${box - stroke}" height="${box - stroke}" rx="${8 * u}" fill="none" stroke="${INK}" stroke-width="${stroke}"/>`;
      const barY = top + (box - bar) / 2;
      return `${boxRect}<rect x="${barX}" y="${barY}" width="${width * u}" height="${bar}" rx="${bar / 2}" fill="${tone}" opacity="${done ? 0.55 : 1}"/>`;
    })
    .join('\n');
}

export function dockIconSvg() {
  const canvas = 1024;
  // The previous 824 face plus a dark rim read a full icon-size smaller beside
  // Docker and QQ. A 900 face preserves the Dock's transparent breathing room
  // while matching their perceived mass; the paper itself is now the edge.
  const face = 900;
  const offset = (canvas - face) / 2;
  const facePath = squirclePath(offset, offset, face, face * 0.225);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${canvas}" height="${canvas}" viewBox="0 0 ${canvas} ${canvas}">
  <defs>
    <linearGradient id="paper" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="${PAPER_LIGHT}"/>
      <stop offset="1" stop-color="${PAPER_DEEP}"/>
    </linearGradient>
    <filter id="shadow" x="-20%" y="-20%" width="140%" height="150%">
      <feDropShadow dx="0" dy="12" stdDeviation="13" flood-color="#000" flood-opacity="0.3"/>
    </filter>
    <clipPath id="faceClip"><path d="${facePath}"/></clipPath>
  </defs>
  <path d="${facePath}" fill="url(#paper)" filter="url(#shadow)"/>
  <g clip-path="url(#faceClip)">
    <!-- a whisper of the app's grid paper, so the face is not flat plastic -->
    <path d="${Array.from({ length: 12 }, (_, i) => `M ${offset + (i + 1) * (face / 13)} ${offset} v ${face}`).join(' ')}" stroke="${INK}" stroke-opacity="0.045" stroke-width="3"/>
    <path d="${Array.from({ length: 12 }, (_, i) => `M ${offset} ${offset + (i + 1) * (face / 13)} h ${face}`).join(' ')}" stroke="${INK}" stroke-opacity="0.045" stroke-width="3"/>
    ${ledger(offset, offset, face)}
  </g>
</svg>`;
}

/** Edge-to-edge face for maskable PWA icons: the platform supplies the shape. */
export function maskableIconSvg(size = 512) {
  // Safe zone for maskable icons is the inner 80%; the ledger sits well inside it.
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 1024 1024">
  <defs>
    <linearGradient id="paper" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="${PAPER_LIGHT}"/>
      <stop offset="1" stop-color="${PAPER_DEEP}"/>
    </linearGradient>
  </defs>
  <rect width="1024" height="1024" fill="url(#paper)"/>
  <rect x="24" y="24" width="976" height="976" fill="none" stroke="${INK}" stroke-width="48"/>
  ${ledger(100, 100, 824)}
</svg>`;
}
