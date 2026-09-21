// The application's names.
//
// Brand and product are two different names doing two different jobs: users
// search for "Sumery Virgo" and have to find it among everything else called
// Virgo, while the menu bar only has room for "Virgo". The repository, the
// `todo` CLI, the crates and the TODO.md format keep their names on purpose.
// A rename touches half a dozen files and the one it misses fails silently,
// so the mapping is pinned here against the real config files. The rules are
// the Sumery brand system (Libra repo, docs/brand/DESIGN.md, section 1).
//
//   node --test apps/desktop/identity.test.mjs   (pnpm --filter @todo/desktop test:identity)
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const desktop = dirname(fileURLToPath(import.meta.url));
const read = (rel) => readFileSync(join(desktop, rel), 'utf8');

const config = JSON.parse(read('src-tauri/tauri.conf.json'));
const infoPlist = read('src-tauri/Info.plist');
const manifest = JSON.parse(read('public/manifest.webmanifest'));

function plistValue(key) {
  return infoPlist.match(new RegExp(`<key>${key}</key>\\s*<string>([^<]*)</string>`))?.[1];
}

test('the searchable name carries the brand', () => {
  assert.equal(config.productName, 'Sumery Virgo');
  assert.equal(plistValue('CFBundleDisplayName'), 'Sumery Virgo');
  assert.equal(config.app.windows[0].title, 'Sumery Virgo');
  assert.match(read('index.html'), /<title>Sumery Virgo<\/title>/);
  // The PWA is the same build in a browser; it is listed under the same name.
  assert.equal(manifest.name, 'Sumery Virgo');
  assert.equal(manifest.short_name, 'Virgo');
});

test('the menu bar name is the product alone and fits', () => {
  const menuName = plistValue('CFBundleName');
  assert.equal(menuName, 'Virgo');
  // Apple truncates CFBundleName past 15 characters.
  assert.ok(menuName.length <= 15, `CFBundleName is ${menuName.length} characters`);
});

test('the bundle id belongs to the brand', () => {
  assert.equal(config.identifier, 'com.sumery.virgo');
  assert.match(config.identifier, /^com\.sumery\./);
});

test('the engineering names stay', () => {
  // Deliberately unchanged: the CLI is the format's tool, not the product.
  assert.match(read('src-tauri/Cargo.toml'), /^name = "todo-desktop"/m);
  assert.equal(JSON.parse(read('package.json')).name, '@todo/desktop');
});

test('the packaging script builds the bundle Tauri actually produces', () => {
  // tauri.conf.json productName decides the .app name; the script must agree.
  const script = read('../../scripts/package-macos.sh');
  assert.match(script, /bundle\/macos\/Sumery Virgo\.app"/);
  assert.doesNotMatch(script, /\/Todo\.app/);
});

test('Windows shows the brand where Windows has room for it', () => {
  assert.equal(config.bundle.publisher, 'Sumery');
  assert.equal(config.bundle.windows.nsis.startMenuFolder, 'Sumery');
  assert.equal(config.bundle.windows.nsis.installMode, 'currentUser');
});

test('the sidebar carries the brand mark and the old one is gone', () => {
  const sidebar = read('src/components/Sidebar.tsx');
  assert.match(sidebar, /SUMERY · VIRGO/);
  assert.match(sidebar, /<strong>Virgo<\/strong>/);
  assert.doesNotMatch(sidebar, /FIELD \/ TODO|LOCAL WORK MEMORY|>FT</);
});

test('the accent tokens are amber, with dark ink on filled controls', () => {
  const css = read('src/style.css');
  assert.match(css, /--accent: #b45309;/);
  assert.match(css, /--primary-fill: #f5a524;/);
  assert.match(css, /--primary-ink: #1b1547;/);
  assert.doesNotMatch(css, /--violet/);
});
