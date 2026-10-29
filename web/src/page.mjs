// The page skeleton shared by both languages: the Sumery six-screen landing
// layout (work/docs/brand/LANDING.md) — hero, how it works, boundaries, what it
// covers, free & sponsorship (decision ④), get / notify, footer.
//
// Content strings are trusted, hand-written HTML (they carry <code>, <kbd>,
// <strong>); attribute values go through attr().

import { SITE } from "./site.mjs";
import { renderArt } from "./art.mjs";

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const attr = (s) => esc(s).replace(/"/g, "&quot;");
const umamiEvent = (event) =>
  event ? ` data-umami-event="${attr(event.name)}"${Object.entries(event.data ?? {}).map(([k, v]) => ` data-umami-event-${k}="${attr(v)}"`).join("")}` : "";

const arrow = `<svg class="i" aria-hidden="true" viewBox="0 0 24 24"><path d="M4 12h15M13 6l6 6-6 6"/></svg>`;

function action(a) {
  const external = /^mailto:|^https?:/.test(a.href);
  return `<a class="btn btn-${a.kind}" href="${attr(a.href)}"${external && !a.href.startsWith("mailto:") ? ' rel="noopener"' : ""}${umamiEvent(a.event)}>${a.label}${a.kind === "primary" ? arrow : ""}</a>`;
}

const shot = (s) => `
      <figure class="shot">
        <img src="${attr(s.src)}" alt="${attr(s.alt)}" width="${s.width}" height="${s.height}" loading="lazy" decoding="async">
        <figcaption>${s.caption}</figcaption>
      </figure>`;

function section({ id, title, sub }, body, extraClass = "") {
  return `
    <section class="band ${extraClass}" id="${id}" aria-labelledby="h-${id}">
      <div class="band-head">
        <h2 id="h-${id}">${title}</h2>
        ${sub ? `<p class="band-sub">${sub}</p>` : ""}
      </div>
      ${body}
    </section>`;
}

/** One language's page. `content` holds every language, for the switch and hreflang. */
export function renderPage(c, { lang, cssHref, content }) {
  const others = Object.entries(content).filter(([l]) => l !== lang);
  const alternates = Object.entries(content)
    .map(([l, lc]) => `<link rel="alternate" hreflang="${lc.hreflang}" href="${SITE.origin}/${l}/">`)
    .join("\n");
  const switcher = others
    .map(
      ([l, lc]) =>
        `<a class="lang" href="/${l}/" hreflang="${lc.hreflang}" lang="${lc.htmlLang}" data-lang="${l}" aria-label="${attr(lc.ui.readIn)}" data-umami-event="lang_switch" data-umami-event-to="${l}">${lc.langName}</a>`,
    )
    .join("");

  const hero = `
    <section class="hero" aria-labelledby="h-hero">
      <div class="hero-copy">
        <p class="eyebrow">${c.brand.sub}</p>
        <h1 id="h-hero">${c.hero.title.map((line) => `<span class="line">${line}</span>`).join("")}</h1>
        <p class="lede">${c.hero.lede}</p>
        <div class="actions">${c.hero.actions.map(action).join("")}</div>
        ${c.hero.fine ? `<p class="fine">${c.hero.fine}</p>` : ""}
      </div>
      <figure class="hero-art">
        ${renderArt(c.art)}
        <figcaption>${c.ui.artCaption}</figcaption>
      </figure>
    </section>`;

  const how = section(
    c.how,
    `<ol class="steps">${c.how.steps
      .map(
        (s, i) => `
        <li class="step">
          <span class="step-n" aria-hidden="true">${String(i + 1).padStart(2, "0")}</span>
          <h3>${s.title}</h3>
          <p>${s.body}</p>
        </li>`,
      )
      .join("")}</ol>
      ${c.how.note ? `<p class="band-note">${c.how.note}</p>` : ""}
      ${c.how.shot ? shot(c.how.shot) : ""}`,
  );

  const bounds = section(
    c.bounds,
    `<ul class="bounds">${c.bounds.items
      .map((b) => `<li><h3>${b.title}</h3><p>${b.body}</p></li>`)
      .join("")}</ul>`,
    "band-paper",
  );

  const catalog = section(
    c.catalog,
    `<dl class="catalog">${c.catalog.items
      .map((it) => `<div class="catalog-item"><dt>${it.name}</dt><dd>${it.body}</dd></div>`)
      .join("")}</dl>
      ${c.catalog.after ? `<p class="band-note">${c.catalog.after}</p>` : ""}
      ${c.catalog.shot ? shot(c.catalog.shot) : ""}`,
  );

  const tierItems = c.free.sponsor.tiers
    .map((t, i) => {
      const isCustom = t === c.free.sponsor.customLabel;
      const amount = SITE.sponsor?.tiers[i];
      const inner = isCustom || !amount ? t : `<button class="tier" type="button" data-sponsor-amount="${amount}">${t}</button>`;
      return `<li>${inner}</li>`;
    })
    .join("");
  const free = section(
    c.free,
    `<div class="free">
        <div class="free-copy"><p>${c.free.body}</p></div>
        <div class="sponsor">
          <p>${c.free.sponsor.body}</p>
          <ul class="tiers">${tierItems}</ul>
          <p class="channels">${c.free.sponsor.channels}</p>
          <p class="backers">${c.free.sponsor.backers}</p>
          <p class="sponsor-msg" data-sponsor-msg hidden></p>
        </div>
      </div>`,
    "band-paper",
  );

  const get = section(
    c.get,
    `<div class="get">
        <div class="get-copy">
          ${c.get.body.map((p) => `<p>${p}</p>`).join("")}
          <div class="actions">${action({ ...c.get.cta, kind: "primary" })}</div>
        </div>
        <table class="platforms">
          <caption class="sr-only">${c.get.tableCaption}</caption>
          <thead><tr><th scope="col">${c.get.columns[0]}</th><th scope="col">${c.get.columns[1]}</th></tr></thead>
          <tbody>${c.get.rows.map((r) => `<tr><th scope="row">${r[0]}</th><td>${r[1]}</td></tr>`).join("")}</tbody>
        </table>
      </div>`,
    "band-get",
  );

  const f = SITE.filing;
  return `<!doctype html>
<html lang="${c.htmlLang}" data-scene="${SITE.scene}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(c.meta.title)}</title>
<meta name="description" content="${attr(c.meta.description)}">
<link rel="canonical" href="${SITE.origin}/${lang}/">
${alternates}
<link rel="alternate" hreflang="x-default" href="${SITE.origin}/">
<meta property="og:type" content="website">
<meta property="og:site_name" content="${attr(c.brand.kicker)}">
<meta property="og:title" content="${attr(c.meta.title)}">
<meta property="og:description" content="${attr(c.meta.description)}">
<meta property="og:url" content="${SITE.origin}/${lang}/">
<meta property="og:locale" content="${c.ogLocale}">
<meta property="og:image" content="${SITE.origin}/favicon-512.png">
<meta name="theme-color" content="${SITE.themeColor}">
<link rel="icon" href="/favicon-light.png" type="image/png" media="(prefers-color-scheme: light)">
<link rel="icon" href="/favicon-dark.png" type="image/png" media="(prefers-color-scheme: dark)">
<link rel="icon" href="/favicon.png" type="image/png">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="stylesheet" href="${cssHref}">
<script defer src="${SITE.umami.src}" data-website-id="${SITE.umami.websiteId}" data-domains="${SITE.umami.domains}" data-exclude-search="true"></script>
<meta name="sponsor-api" content="${attr(SITE.sponsor?.api ?? "")}">
<script defer src="/sponsor.js"></script>
<script>document.addEventListener("click",function(e){var a=e.target.closest&&e.target.closest("[data-lang]");if(a)try{localStorage.setItem("sumery.lang",a.getAttribute("data-lang"))}catch(_){}});</script>
</head>
<body>
<a class="skip" href="#main">${c.ui.skip}</a>
<p class="status" role="note"><span class="status-label">${c.status.label}</span> <span>${c.status.text}</span> ${action({ ...c.status.cta, kind: "text" })}</p>
<header class="top">
  <a class="mark" href="/${lang}/" aria-label="${attr(c.ui.home)}">
    <picture><source srcset="/favicon-dark.png" media="(prefers-color-scheme: dark)"><img class="mark-icon" src="/favicon-light.png" alt="" width="32" height="32"></picture>
    <span class="mark-text"><span class="kicker">${c.brand.kicker}</span><span class="wordmark">${c.brand.name}</span></span>
  </a>
  <nav class="nav" aria-label="${attr(c.ui.nav)}">
    <ul>${c.nav.map((n) => `<li><a href="${attr(n.href)}">${n.label}</a></li>`).join("")}</ul>
  </nav>
  ${switcher}
</header>
<main id="main">
${hero}
${how}
${bounds}
${catalog}
${free}
${get}
</main>
<footer class="foot">
  <p class="foot-sub">${c.footer.sub}</p>
  <p class="foot-note">${c.footer.note} <a href="/privacy/${lang}/">${c.privacy.title}</a></p>
  <p class="foot-filing">${f.owner} · <a href="${f.icp.href}" rel="noopener noreferrer" target="_blank">${f.icp.text}</a> · <a href="${f.police.href}" rel="noopener noreferrer" target="_blank">${f.police.text}</a> · <a href="mailto:${f.email}">${f.email}</a></p>
</footer>
</body>
</html>
`;
}

/**
 * privacy page: a small standalone page linked from the footer. Rendered per
 * language by build.mjs into privacy/<lang>/index.html.
 */
export function renderPrivacy(c, { lang, cssHref }) {
  return `<!doctype html>
<html lang="${c.htmlLang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(c.privacy.title)} · ${esc(c.brand.name)}</title>
<meta name="robots" content="noindex">
<link rel="icon" href="/favicon-light.png" type="image/png" media="(prefers-color-scheme: light)">
<link rel="icon" href="/favicon-dark.png" type="image/png" media="(prefers-color-scheme: dark)">
<link rel="icon" href="/favicon.png" type="image/png">
<link rel="stylesheet" href="${cssHref}">
</head>
<body>
<main id="main" class="privacy-page">
  <h1>${esc(c.privacy.title)}</h1>
  <p class="privacy-updated">${esc(c.privacy.updated)}</p>
  ${c.privacy.body.map((p) => `<p>${p}</p>`).join("")}
  <p><a class="btn btn-text" href="/${lang}/">← ${esc(c.ui.home)}</a></p>
</main>
</body>
</html>
`;
}

/**
 * dist/index.html: send the reader to the language they asked for. An explicit
 * choice (the switch in the header) wins; otherwise the first zh or en entry in
 * navigator.languages; otherwise English. Without JavaScript the page is a
 * plain chooser.
 */
export function renderRouter(content) {
  const langs = Object.keys(content);
  const links = Object.entries(content)
    .map(([l, c]) => `<a href="/${l}/" hreflang="${c.hreflang}" lang="${c.htmlLang}" data-lang="${l}">${c.langName}</a>`)
    .join(" · ");
  const alternates = Object.entries(content)
    .map(([l, c]) => `<link rel="alternate" hreflang="${c.hreflang}" href="${SITE.origin}/${l}/">`)
    .join("\n");
  return `<!doctype html>
<html lang="en" data-scene="${SITE.scene}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${esc(content.en.brand.kicker)}</title>
<link rel="canonical" href="${SITE.origin}/">
${alternates}
<link rel="alternate" hreflang="x-default" href="${SITE.origin}/">
<link rel="icon" href="/favicon-light.png" type="image/png" media="(prefers-color-scheme: light)">
<link rel="icon" href="/favicon-dark.png" type="image/png" media="(prefers-color-scheme: dark)">
<link rel="icon" href="/favicon.png" type="image/png">
<meta name="theme-color" content="${SITE.themeColor}">
<script>
(function () {
  var langs = ${JSON.stringify(langs)}, pick = null;
  try { pick = localStorage.getItem("sumery.lang"); } catch (e) {}
  if (langs.indexOf(pick) < 0) {
    pick = "en";
    var prefs = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language || ""];
    for (var i = 0; i < prefs.length; i++) {
      var p = String(prefs[i]).toLowerCase().split("-")[0];
      if (langs.indexOf(p) >= 0) { pick = p; break; }
    }
  }
  location.replace("/" + pick + "/" + location.search + location.hash);
})();
</script>
<style>body{margin:0;min-height:100vh;display:grid;place-items:center;font:16px/1.6 -apple-system,BlinkMacSystemFont,"Segoe UI","PingFang SC",sans-serif;background:#f7efe4;color:#2a2740}html[data-scene=night] body{background:#0a0c1a;color:#fffbf3}a{color:inherit}</style>
</head>
<body>
<p>${links}</p>
</body>
</html>
`;
}
