// Renders the app's icon and splash artwork in the website's sticker style.
//
//   node scripts/generate-assets.mjs   -> writes assets/*.png
//   npx capacitor-assets generate      -> fans assets/ out into every Android and iOS size
//
// `npm run assets` does both. The emblem is a chunky check mark over rising usage bars on
// a sticker tile. It deliberately isn't a Poké Ball: store icons are where trademark
// complaints start, so the in-app CSS Poké Ball stays out of them.

import { mkdir, readFile, writeFile } from 'node:fs/promises';
import opentype from 'opentype.js';
import sharp from 'sharp';

// Fredoka Bold, as on the website's headings. The wordmark is converted to outlines so the
// render doesn't depend on the fonts installed on the machine running this script.
const fontFile = await readFile('node_modules/@fontsource/fredoka/files/fredoka-latin-700-normal.woff');
const fredoka = opentype.parse(fontFile.buffer.slice(fontFile.byteOffset, fontFile.byteOffset + fontFile.byteLength));

const LIGHT = { bg: '#fff6e6', dots: '#f1dfbf', panel: '#fffdf8', ink: '#2b2045', accent: '#5b4bff', pop: '#ffcb05', pink: '#ff6fa8' };
/** The website's fourth chart colour, so the tallest bar doesn't blend into the check. */
const TEAL = '#129e82';
const DARK = { bg: '#17122e', dots: '#3b3170', panel: '#221a42', ink: '#f4efff', accent: '#a395ff', pop: '#ffd84a', pink: '#ff8cc0' };
/** The tile keeps its light colours on the dark splash, like a sticker on a night sky. */
const TILE = { panel: LIGHT.panel, edge: LIGHT.ink, drop: '#08051a' };

/** Polka dots, as on the website: big dots on a 26px grid with small ones between. */
function dots(width, height, colors, scale) {
  const step = 26 * scale;
  return `
    <defs>
      <pattern id="dots" width="${step}" height="${step}" patternUnits="userSpaceOnUse">
        <circle cx="0" cy="0" r="${1.7 * scale}" fill="${colors.dots}"/>
        <circle cx="${step}" cy="0" r="${1.7 * scale}" fill="${colors.dots}"/>
        <circle cx="0" cy="${step}" r="${1.7 * scale}" fill="${colors.dots}"/>
        <circle cx="${step}" cy="${step}" r="${1.7 * scale}" fill="${colors.dots}"/>
        <circle cx="${step / 2}" cy="${step / 2}" r="${1.1 * scale}" fill="${colors.dots}"/>
      </pattern>
    </defs>
    <rect width="${width}" height="${height}" fill="${colors.bg}"/>
    <rect width="${width}" height="${height}" fill="url(#dots)"/>`;
}

/**
 * The emblem in a 512×512 box: a slightly tilted sticker tile with an offset shadow, three
 * rising bars and a chunky check mark with its own offset shadow.
 */
function emblem() {
  const check = 'M146 196 L218 262 L370 112';
  return `
    <g transform="rotate(-6 256 256)">
      <rect x="76" y="86" width="372" height="372" rx="78" fill="${TILE.drop}"/>
      <rect x="62" y="70" width="372" height="372" rx="78" fill="${TILE.panel}" stroke="${TILE.edge}" stroke-width="18"/>
      <g stroke="${TILE.edge}" stroke-width="12">
        <rect x="128" y="322" width="64" height="76" rx="16" fill="${LIGHT.pink}"/>
        <rect x="216" y="284" width="64" height="114" rx="16" fill="${LIGHT.pop}"/>
        <rect x="304" y="238" width="64" height="160" rx="16" fill="${TEAL}"/>
      </g>
      <g fill="none" stroke-linecap="round" stroke-linejoin="round">
        <path d="${check}" transform="translate(14 14)" stroke="${TILE.drop}" stroke-width="72"/>
        <path d="${check}" stroke="${TILE.edge}" stroke-width="72"/>
        <path d="${check}" stroke="${LIGHT.accent}" stroke-width="44"/>
      </g>
    </g>`;
}

/** SVG path data for `text` in Fredoka Bold, starting at (x, baseline). Returns its width too. */
function textPath(text, x, baseline, size) {
  const path = fredoka.getPath(text, x, baseline, size);
  return { d: path.toPathData(1), width: fredoka.getAdvanceWidth(text, size) };
}

/** "PokeChecks", with "Checks" in purple over a yellow offset, as on the website. */
function wordmark(cx, baseline, size, colors) {
  const poke = fredoka.getAdvanceWidth('Poke', size);
  const checks = fredoka.getAdvanceWidth('Checks', size);
  const x = cx - (poke + checks) / 2;
  const a = textPath('Poke', x, baseline, size);
  const b = textPath('Checks', x + poke, baseline, size);
  const shadow = textPath('Checks', x + poke + size * 0.045, baseline + size * 0.045, size);
  return `
    <path d="${a.d}" fill="${colors.ink}"/>
    <path d="${shadow.d}" fill="${colors.pop}"/>
    <path d="${b.d}" fill="${colors.accent}"/>`;
}

const svg = (w, h, body) => `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}">${body}</svg>`;

async function render(file, w, h, body) {
  await sharp(Buffer.from(svg(w, h, body))).png().toFile(file);
  console.log('wrote', file);
}

await mkdir('assets', { recursive: true });

// iOS and older Android: the whole square, no transparency.
await render('assets/icon-only.png', 1024, 1024, `${dots(1024, 1024, LIGHT, 2)}<g transform="translate(112 112) scale(1.5625)">${emblem()}</g>`);

// Android adaptive icon. capacitor-assets already insets the foreground by 16.7% on each
// side (Android's safe zone), so the emblem fills most of this canvas; at 80% the tile's
// corners still clear a circular launcher mask.
await render('assets/icon-foreground.png', 1024, 1024, `<g transform="translate(102 102) scale(1.6)">${emblem()}</g>`);
await render('assets/icon-background.png', 1024, 1024, dots(1024, 1024, LIGHT, 2));

// Splash screens: emblem and wordmark centred, small enough to survive any crop.
for (const [file, colors] of [['assets/splash.png', LIGHT], ['assets/splash-dark.png', DARK]]) {
  await render(
    file,
    2732,
    2732,
    `${dots(2732, 2732, colors, 3)}
     <g transform="translate(1066 900) scale(1.17)">${emblem()}</g>
     ${wordmark(1366, 1820, 210, colors)}`,
  );
}

await writeFile('assets/README.md', 'Generated by `npm run assets` (scripts/generate-assets.mjs). Edit the script, not these files.\n');
