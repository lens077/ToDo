// Renders the app icon from `icon.svg.mjs` into the PNG sources Tauri and the
// PWA manifest consume. Run `pnpm icon` from apps/desktop.
//
// macOS Dock icons are not full-bleed squares. This uses a 1024 canvas with a
// 900 squircle centred in it: enough transparent room for the shadow while its
// visual mass matches current Docker/QQ Dock icons. The PWA maskable icon is the same face rendered
// edge to edge, because the platform applies its own mask there.
import { chromium } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { dockIconSvg, maskableIconSvg } from './icon.svg.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, 'out');
await mkdir(out, { recursive: true });

const browser = await chromium.launch();
async function render(svg, size, file) {
  const page = await browser.newPage({ viewport: { width: size, height: size }, deviceScaleFactor: 1 });
  await page.setContent(
    `<!doctype html><html><body style="margin:0;background:transparent">${svg}</body></html>`,
  );
  await page.locator('svg').first().screenshot({ path: file, omitBackground: true });
  await page.close();
}

await writeFile(path.join(out, 'dock.svg'), dockIconSvg());
await render(dockIconSvg(), 1024, path.join(out, 'dock-1024.png'));
await render(maskableIconSvg(512), 512, path.join(out, 'maskable-512.png'));
await render(maskableIconSvg(192), 192, path.join(out, 'maskable-192.png'));
await browser.close();
console.log(`wrote ${out}`);
