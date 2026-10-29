#!/usr/bin/env node
// Build the Virgo landing site into dist/. No dependencies: Node 18+ only.
//
//   dist/index.html      language router — picks /zh/ or /en/ from a remembered
//                        choice, else navigator.languages, else English
//   dist/en/index.html   English page, fully rendered (no client-side i18n)
//   dist/zh/index.html   Chinese page, fully rendered
//   dist/site.css        shared stylesheet, linked with a content hash
//   dist/sitemap.xml, dist/robots.txt, and everything in public/
//
// Both languages are rendered from the same skeleton (src/page.mjs); the build
// fails if the two content files do not have the same shape, so a section can
// never exist in one language only.

import { createHash } from "node:crypto";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import en from "./src/content.en.mjs";
import zh from "./src/content.zh.mjs";
import { SITE } from "./src/site.mjs";
import { renderPage, renderPrivacy, renderRouter } from "./src/page.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const DIST = join(HERE, "dist");
const CONTENT = { en, zh };

/** Same keys, same array lengths, same value types — text may differ. */
function assertSameShape(a, b, path = "content") {
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) {
      throw new Error(`${path}: en and zh differ in length (${a?.length} vs ${b?.length})`);
    }
    a.forEach((item, i) => assertSameShape(item, b[i], `${path}[${i}]`));
    return;
  }
  if (a && typeof a === "object") {
    if (!b || typeof b !== "object") throw new Error(`${path}: missing in zh`);
    const ka = Object.keys(a).sort().join(",");
    const kb = Object.keys(b).sort().join(",");
    if (ka !== kb) throw new Error(`${path}: keys differ\n  en: ${ka}\n  zh: ${kb}`);
    for (const key of Object.keys(a)) assertSameShape(a[key], b[key], `${path}.${key}`);
    return;
  }
  if (typeof a !== typeof b) throw new Error(`${path}: en is ${typeof a}, zh is ${typeof b}`);
}

assertSameShape(en, zh);

rmSync(DIST, { recursive: true, force: true });
mkdirSync(DIST, { recursive: true });
cpSync(join(HERE, "public"), DIST, { recursive: true });

const css = readFileSync(join(HERE, "src/site.css"), "utf8");
const cssHref = `/site.css?v=${createHash("sha256").update(css).digest("hex").slice(0, 10)}`;
writeFileSync(join(DIST, "site.css"), css);

for (const lang of Object.keys(CONTENT)) {
  const html = renderPage(CONTENT[lang], { lang, cssHref, content: CONTENT });
  mkdirSync(join(DIST, lang), { recursive: true });
  writeFileSync(join(DIST, lang, "index.html"), html);
  // Privacy page (footer link; noindex, not in the sitemap).
  const privacy = renderPrivacy(CONTENT[lang], { lang, cssHref, content: CONTENT });
  mkdirSync(join(DIST, "privacy", lang), { recursive: true });
  writeFileSync(join(DIST, "privacy", lang, "index.html"), privacy);
}
writeFileSync(join(DIST, "index.html"), renderRouter(CONTENT));

const urls = Object.keys(CONTENT)
  .map(
    (lang) => `  <url>
    <loc>${SITE.origin}/${lang}/</loc>
${Object.entries(CONTENT)
  .map(([l, c]) => `    <xhtml:link rel="alternate" hreflang="${c.hreflang}" href="${SITE.origin}/${l}/"/>`)
  .join("\n")}
    <xhtml:link rel="alternate" hreflang="x-default" href="${SITE.origin}/"/>
  </url>`,
  )
  .join("\n");
writeFileSync(
  join(DIST, "sitemap.xml"),
  `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml">
${urls}
</urlset>
`,
);
writeFileSync(join(DIST, "robots.txt"), `User-agent: *\nAllow: /\n\nSitemap: ${SITE.origin}/sitemap.xml\n`);

console.log(`built ${Object.keys(CONTENT).length} languages → ${DIST}`);
