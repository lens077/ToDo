# Tauri 2.x Proof-of-Viability Research

Verification of which well-known desktop/productivity apps are actually built with Tauri.
All claims below are backed by primary sources (repo file contents, official docs, official blogs).

Research date: 2026-08-30. Latest Tauri release at time of research: `tauri-v2.11.5` (2026-07-01).

---

## TASK A — Where does Tauri list showcase apps?

**Key finding: there is no official Tauri showcase page anymore, and `awesome-tauri` deliberately deleted its entire application list.**

| Fact | Evidence |
|---|---|
| `https://tauri.app/showcase/` | **HTTP 404** — does not exist |
| `https://v2.tauri.app/showcase/` | **HTTP 404** — does not exist |
| `https://tauri.app/` | 200, but lists only sponsors (1Password), no app showcase |
| awesome-tauri repo | https://github.com/tauri-apps/awesome-tauri (8,048 stars, default branch `dev`) |

The current awesome-tauri README contains ONLY these sections:
`Guides & Tutorials`, `Articles`, `Templates`, `Plugins`, `Integrations`.

**All applications were removed on 2026-08-20** by PR "Remove all applications":
https://github.com/tauri-apps/awesome-tauri/pull/876 (merged 2026-08-20, no description given).
A follow-up commit (2026-08-24) added a "no-application rule to PR template".

⚠️ **Interpretation caveat:** the PR body was empty, so the *reason* for removal is NOT
documented. Do not present this as "Tauri is losing apps" — most plausibly it is list-maintenance
burden (link rot, unverifiable closed-source claims). But the practical consequence is real:
**Tauri currently has no official, curated list of applications built with it.**

### Recovered pre-removal list (historical snapshot)

The app list is still recoverable from the parent commit `c625de266743519d82105ff42ea46528769ec3df`:
https://github.com/tauri-apps/awesome-tauri/blob/c625de266743519d82105ff42ea46528769ec3df/README.md

It had 543 lines with app categories: Audio & Video, ChatGPT clients, Data, **Developer tools**,
Ebook readers, Email & Feeds, File management, Finance, Gaming, Information, Learning, Networking,
Office & Writing, **Productivity**, Search, Security, Social media, Utilities.

Notable developer-tools entries: GitButler, Hoppscotch, Yaak, Pake, Aptakube, JET Pilot, KFtray,
GitLight, Rivet, DevTools-X, Dropcode, Keadex Mina.

Notable productivity entries (relevant to a TODO app): Kanri (Kanban), Takma (Kanban to-do),
Banban (Kanban), Progressive (todo w/ progress tracking), mynd (developer todo-list), Clippy
(clipboard), EcoPaste, PasteBar, Qopy, Kunkun (launcher), Blink Eye, Focust.

---

## TASK B — Verification table

| App | Tauri? | Actual stack | Evidence URL | Confirmed? |
|---|---|---|---|---|
| **Obsidian** | **NO** | Electron (closed source) | https://github.com/obsidianmd/obsidian-api — plugin docs say `isDesktopOnly`: "whether your plugin uses NodeJS or **Electron** APIs"; "Import NodeJS or Electron API using `require('fs')` or `require('electron')`" | **Confirmed** (indirect but authoritative — official API docs) |
| **Logseq** | **NO** | Electron + ClojureScript; mobile = Capacitor | https://github.com/logseq/logseq/blob/master/package.json — `"main": "static/electron.js"`, scripts `release-electron`, `electron-watch`; Capacitor deps (`@capacitor/cli` ^8.3.1). **grep for "tauri" in package.json = 0 hits** | **Confirmed** |
| **Spacedrive** | **YES (desktop)** | Tauri 2.1 + React 19 (desktop); **React Native/Expo** (mobile); Rust core | https://github.com/spacedriveapp/spacedrive/blob/main/apps/tauri/src-tauri/Cargo.toml — `tauri = { version = "2.1", ... }` | **Confirmed** |
| **Pake** | **YES** | Tauri **2.10.2** + Rust | https://github.com/tw93/Pake/blob/main/src-tauri/Cargo.toml — `tauri = { version = "2.10.2" }`, `tauri-build = "2.5.5"`; plus `src-tauri/tauri.conf.json` | **Confirmed** |
| **Cursor** | **NO** | Electron (VS Code fork, closed source) | https://cursor.com/blog/shadow-workspace — Cursor's own engineering blog: "a hidden **Electron** window"; "Because of **Electron sandboxing**, the two renderer processes cannot communicate…" | **Confirmed** (primary — vendor's own blog) |
| **Warp** | **NO** | Native Rust + GPU (Metal), custom UI framework | https://www.warp.dev/blog/how-warp-works — "After a very brief experiment with **Electron**, we quickly pivoted to building in **Rust and rendering directly on the GPU using Metal**"; "we decided to build our own UI framework in Rust" | **Confirmed** (primary — vendor's own blog) |
| **AppFlowy** | **NO** | **Flutter/Dart** frontend + Rust backend (`rust-lib`) | https://github.com/AppFlowy-IO/AppFlowy/blob/main/frontend/appflowy_flutter/pubspec.yaml — `flutter: ">=3.27.4"`. Repo language = Dart. Tree: `frontend/appflowy_flutter` + `frontend/rust-lib`, **no `src-tauri`** | **Confirmed** |
| **Xournal++** | **NO** | C++ / GTK3 | https://github.com/xournalpp/xournalpp — repo language C++; description: "Written in **C++ with GTK3**". Tree has `cmake/`, `src/`, `ui/`, no `src-tauri` | **Confirmed** |
| **Rnote** | **NO** (but is Rust) | Rust + **GTK4 / libadwaita** (not Tauri) | https://github.com/flxzt/rnote/blob/main/Cargo.toml — `gtk4 = { version = "0.10.0" }`, `adw = { package = "libadwaita" }`. 11,597 stars | **Confirmed** |
| **Yaak** | **YES** | Tauri 2 + **React** (TypeScript) | https://github.com/mountain-loop/yaak/blob/master/crates-tauri/yaak-app-client/tauri.conf.json (+ `tauri.release.conf.json`, `tauri.linux.conf.json`); root `package.json` build script uses `tauri` CLI | **Confirmed** |
| **Hoppscotch desktop** | **YES** | Tauri 2 + **Vue 3** | https://github.com/hoppscotch/hoppscotch/blob/main/packages/hoppscotch-desktop/src-tauri/Cargo.toml — `tauri = { version = "2", features = ["devtools"] }`; package.json has `vue: 3.5.41`, `@tauri-apps/api: 2.1.1` | **Confirmed** |
| **Kando** | **NO** | **Electron** (Electron Forge) + TypeScript | https://github.com/kando-menu/kando/blob/main/package.json — `"electron": "^43.4.1"`, `@electron-forge/cli` in start/package/make/publish scripts | **Confirmed** |
| **Mission Center** | **NO** | Rust + **GTK4 / libadwaita**, meson build | https://gitlab.com/mission-center-devs/mission-center — `Cargo.toml` has `[dependencies.adw] package = "libadwaita"` and `[dependencies.gtk] package = "gtk4"`; `meson.build` present, no `src-tauri`. 654 stars | **Confirmed** |
| **Payload** | **AMBIGUOUS → YES (the desktop one)** | See ambiguity note below | https://payload.app/ (live, HTTP 200) — listed in awesome-tauri "File management" as closed source | **Partially confirmed** (listing only, closed source) |
| **Noor** | **AMBIGUOUS → resolved** | Closed-source team chat app | https://noor.to/ (live, HTTP 200) — listed in awesome-tauri "Office & Writing" | **Partially confirmed** (listing only, closed source) |
| **Clippy** | **AMBIGUOUS → repo now DEAD** | Was a Tauri clipboard manager | Listed as https://github.com/0-don/clippy — **now HTTP 404, no redirect** | **NOT confirmed — link dead** |
| **GitButler** | **YES** | Tauri **2.11.1** + **Svelte/SvelteKit** + Rust | https://github.com/gitbutlerapp/gitbutler/blob/master/crates/gitbutler-tauri/Cargo.toml — `tauri = { version = "^2.11.1", features = ["unstable"] }`; + `crates/gitbutler-tauri/tauri.conf.json` | **Confirmed** |
| **Zed** | **NO** | Native Rust + own **GPUI** framework | https://github.com/zed-industries/zed/tree/main/crates/gpui — GPUI crate exists; no tauri dependency. (Note: Zed's GPUI descends from the same Rust UI framework work as Warp's — see Warp blog crediting Nathan Sobo) | **Confirmed** |

### Ambiguity notes (as requested)

- **Payload** — genuinely ambiguous. **`payloadcms.com`** is a JavaScript/Next.js headless CMS
  (NOT a desktop app, NOT Tauri). The awesome-tauri entry is a *different* product:
  **`payload.app`** — "Drag & drop file transfers over local networks and online", marked
  `closed source`, listed under File management. Because it is closed source I could **not**
  independently verify its Tauri usage beyond the awesome-tauri listing.
- **Noor** — resolved via the archived list: **`noor.to`**, "Chat app for high-performance teams…
  designed for uninterrupted deep work", marked `closed source`, listed under Office & Writing.
  Not verifiable beyond the listing.
- **Clippy** — three distinct things share this name: (1) **`cargo clippy`**, the official Rust
  linter (not an app); (2) the Microsoft Office assistant (historical); (3) a **Tauri clipboard
  manager** that WAS listed in awesome-tauri as `github.com/0-don/clippy` — "Clipboard manager with
  sync & encryption". **That repo now returns 404 with no redirect**, so it is deleted/renamed and
  I could not verify it. Other Tauri clipboard managers in the archived list are alive and are
  better examples: **EcoPaste**, **PasteBar**, **Qopy**.

---

## Deep dive: the strongest confirmed examples

### GitButler — the single best proof-of-viability for a developer tool

- **Repo:** https://github.com/gitbutlerapp/gitbutler — **21,588 stars**, actively developed
  (last push 2026-08-30, same day as research).
- **Self-description (repo metadata):** "The GitButler version control client, backed by Git,
  **powered by Tauri/Rust/Svelte**".
- **Tauri version:** `^2.11.1` — current-generation Tauri 2.x, not legacy v1.
  Also uses `tauri-build 2.6.1`.
- **Frontend: Svelte / SvelteKit.** Confirmed via `apps/desktop/package.json`
  (`@sveltejs/kit`, `@sveltejs/vite-plugin-svelte`, `svelte`, `svelte-check`, `@sentry/sveltekit`).
- **Architecture note:** it does NOT use the default `src-tauri/` layout. The Tauri host lives in
  `crates/gitbutler-tauri/` (with `tauri.conf.json`, `tauri.conf.release.json`,
  `tauri.conf.nightly.json`) inside a large Rust workspace of ~60 `but-*`/`gitbutler-*` crates.
  This is evidence Tauri scales to a serious multi-crate Rust codebase.
- **Plugin usage (production-grade):** updater, dialog, fs, log, os, process, shell, store,
  window-state, single-instance w/ deep-link, clipboard-manager.
- **Maturity:** 100+ releases on the first API page alone; releases `release/0.22.3` (2026-08-29)
  and `release/0.22.2` (2026-08-28) — a near-daily cadence. Installers are distributed via
  https://gitbutler.com rather than as GitHub release assets (release assets list is empty).
- **Mobile: NO.** There is **no `gen/apple` or `gen/android`** under `crates/gitbutler-tauri/`
  (HTTP 404). Desktop only.

### Spacedrive — confirmed Tauri, but read the mobile detail carefully

- **Repo:** https://github.com/spacedriveapp/spacedrive — **38,876 stars**, **NOT archived**,
  last push 2026-07-29.
- **Is it winding down?** **No evidence of winding down.** The opposite: it is on an active v2 push.
  Releases show `v2.0.0-alpha.2` (2026-02-07) and `v2.0.0-alpha.1` (2025-12-26), after the last
  stable `0.4.3` (2025-03-24). The README points to a new site `v2.spacedrive.com`.
  ⚠️ However, note the honest read: **no stable release since March 2025** — v2 has been in alpha
  for ~8 months. Momentum is real but shipping stable is slow.
- **Tauri version:** `2.1` with features `macos-private-api`, `devtools`, `protocol-asset`.
- **Frontend: React 19** (`apps/tauri/package.json`).
- **⚠️ Mobile is NOT Tauri.** `apps/mobile/package.json` uses **`expo ~54.0.0` and
  `react-native 0.81.5`** — a completely separate React Native/Expo codebase, with `ios/` and
  `android/` native dirs. The Tauri side's generated schemas are
  `desktop-schema.json`, `linux-schema.json`, `macOS-schema.json` — **desktop only, no mobile schema.**
  So despite the README advertising "apps for macOS, Windows, Linux, **iOS and Android**", the
  mobile apps do **not** use Tauri.
- **⚠️ Additional signal:** the repo contains `apps/gpui-photo-grid`, whose `Cargo.toml` depends on
  `gpui = { git = "https://github.com/zed-industries/zed", branch = "main" }` — i.e. Spacedrive is
  **experimenting with Zed's native GPUI renderer**. Last commits there are 2025-11/12, while
  `apps/tauri` is still being actively developed (2026-04). Read this as an *experiment*, not a
  confirmed migration — but it is a genuine yellow flag worth telling the client about.

### Yaak — closest analogue to a polished small dev-tool product

- **Repo:** https://github.com/mountain-loop/yaak — **19,134 stars**, active (push 2026-08-30).
- **Stack:** Tauri 2 + **React** (TypeScript), Rust workspace under `crates-tauri/`, `crates/`,
  `crates-server/`, `crates-proxy/`.
- **Ships real, signed desktop installers across every desktop platform** — release
  `v2026.7.0-beta.9` includes: `.dmg` (x64 + aarch64), `-setup.exe` and `-setup-machine.exe`
  (x64 + arm64), `.deb` (amd64 + arm64), `.rpm` (x86_64 + aarch64), `.AppImage` (amd64 + aarch64),
  `.tar.gz`, plus **`.sig` signature files for every artifact** and `latest.json` for the Tauri
  auto-updater. This is a textbook production Tauri distribution pipeline and is directly
  reusable as a model for the client's TODO app.
- **Mobile: NO** — no `gen/` directory (404).

### Hoppscotch desktop

- Tauri 2 + **Vue 3**, at `packages/hoppscotch-desktop/src-tauri/`.
- Notably uses custom Tauri plugins (`tauri-plugin-appload`, `tauri-plugin-relay`) to load
  web-app bundles and relay requests — evidence Tauri's plugin system is extensible enough for
  a nontrivial product.
- **Mobile: NO** — no `gen/` directory (404).

### 4 additional notable confirmed Tauri apps (dev-productivity relevant)

| App | Stars | Evidence | Why relevant |
|---|---|---|---|
| **Clash Verge Rev** | **141,076** | https://github.com/clash-verge-rev/clash-verge-rev/blob/main/src-tauri/Cargo.toml — `tauri-build = "2.6.3"`, `tauri = { workspace = true }`. Description: "A modern GUI client **based on Tauri**" | By far the highest-starred confirmed Tauri app found; proves Tauri at very large user scale |
| **ChatGPT Desktop (lencx)** | **54,468** | https://github.com/lencx/ChatGPT — repo language **Rust**, Tauri desktop app for Mac/Windows/Linux | Popular cross-platform Tauri desktop app |
| **Arnis** | **17,676** | https://github.com/louis-e/arnis — Rust, Tauri-based GUI | Shows Tauri used for compute-heavy Rust workloads with a GUI |
| **Kunkun** | 1,303 | https://github.com/KunkunSh/kunkun — "open source, cross-platform, **extensible app launcher**" (Tauri; listed in awesome-tauri Productivity) | Directly relevant: an extensible launcher/productivity tool, the closest category to a dev-focused TODO app |

Also worth noting for the client: **1Password is a listed sponsor on https://tauri.app/**
(visible in the homepage sponsor section). This is a credible enterprise signal, though the
Tauri site does not state *which* 1Password components use Tauri — treat "1Password uses Tauri"
as **unverified** at the product level.

---

## Does ANY notable Tauri app ship mobile? — the critical section

**Short answer: I could NOT find a single notable, verifiable production iOS/Android app built
with Tauri mobile. This is the weakest part of the Tauri story and the biggest risk for a client
who wants iOS/Android.**

### What IS confirmed

- Tauri 2.0 stable shipped **October 2, 2024**, and mobile (iOS/Android) was its headline feature:
  https://v2.tauri.app/blog/tauri-20/
- Official distribution guides exist and are live:
  **App Store** https://v2.tauri.app/distribute/app-store/ (HTTP 200) and
  **Google Play** https://v2.tauri.app/distribute/google-play/ (HTTP 200).
  So the *path* to shipping is officially documented.
- Mobile development docs are thorough: https://v2.tauri.app/develop/

### The Tauri team's own caveats (direct quotes from the 2.0 release post)

From https://v2.tauri.app/blog/tauri-20/ :

> "We are **not completely happy about the developer experience at the moment** but are actively
> improving to bring it up to par with the desktop experience."

> "**On mobile not all of the official plugins are supported.** Some are by design not a good fit
> for mobile and some are just not implemented to support mobile yet."

This is the framework authors themselves flagging mobile as behind desktop.

### Evidence that the big Tauri apps do NOT ship Tauri mobile

I checked for the `gen/apple` / `gen/android` directories that a Tauri mobile project requires:

| App | `gen/` mobile dirs | Verdict |
|---|---|---|
| GitButler | 404 — none | Desktop only |
| Yaak | 404 — none | Desktop only |
| Hoppscotch desktop | 404 — none | Desktop only |
| Spacedrive | `gen/schemas` only: `desktop-schema.json`, `linux-schema.json`, `macOS-schema.json` | Desktop only — **mobile is React Native/Expo instead** |

**Spacedrive is the most important data point:** a flagship Tauri project that genuinely wanted
iOS + Android chose **Expo / React Native 0.81** for mobile rather than Tauri mobile
(`apps/mobile/package.json`). If Tauri mobile were production-ready, this is exactly the project
that would have used it.

### Concrete, documented mobile pain

- **Open Tauri iOS bug — app crashes on physical iPhone:**
  https://github.com/tauri-apps/tauri/issues/14675 — "[bug] iOS Debug build fails when running on
  iPhone device". Opened 2025-12-19, **still open**, updated 2026-04-24. Reporter: works on the
  simulator, white screen + crash on a real device, crash inside
  `wry::wkwebview::platform_webview_version`. A commenter's workaround is to **downgrade**
  (`tauri-runtime-wry = "=2.6.0"`, `wry = "=0.37.0"`). Another comment (2026-03-13): "This issue
  still appears to be unresolved." Another (2026-04-24): "When will this issue be fixed?"
  There are **~45 open issues labelled `platform: iOS`** on tauri-apps/tauri.
- **Missing mobile plugin functionality:**
  https://github.com/tauri-apps/plugins-workspace/issues/933 — "Dialog Plugin `pick_folder` not
  implemented" — open since **2024-02-07**, still open. i.e. no native directory picker on mobile.
- **Independent research repo recommending against Tauri iOS:**
  https://github.com/CrazySwami/cross-platform-web-app-research — concludes "iOS support is
  experimental and has significant issues", "iOS Simulator works perfectly / physical devices
  consistently fail", and explicitly recommends **Tauri for desktop + Capacitor for mobile**.
  ⚠️ This is a third-party research repo, not an authoritative source — weight it as one
  practitioner's report, but it matches the official bug tracker.
- **Practitioner retrospective — built 4 Tauri mobile apps, shipped ZERO to a store:**
  https://blog.erikhorton.com/2025/10/05/4-mobile-apps-with-tauri-a-retrospective.html
  The author is *positive* about Tauri ("Building for Mac/Mobile is repeatable and easy",
  Android prototyping "a breeze") — but all four apps only ever sat on his own phone, he writes
  "I'd love to release an app to the world", "I'd love to test on iOS, so I might check out what
  that workflow looks like", and he hit the missing Android directory-selector above.
  **Enthusiastic user, still zero store releases, never even tried iOS.**
- **Community asking the same question and not getting answers:** the top search result for
  shipped Tauri store apps is developers *asking* for a list —
  https://www.reddit.com/r/tauri/comments/1fnhxtk/is_tauri_a_good_choice_for_a_mobile_app/
  ("Can someone link a repo or list of apps that have been built with Tauri in Google Play and app
  store?") and https://www.reddit.com/r/tauri/comments/1j3jetw/mobile_apps_with_tauri/
  ("I've come across various sources suggesting against using Tauri for mobile app development").

### Claimed mobile successes — treat as UNVERIFIED

I found blog/Medium claims of Tauri apps reaching the App Store, but **could not verify any of
them against a real store listing**, and I am flagging them as weak evidence:

- "Shadow Dash", a PixiJS + SvelteKit + Tauri 2 iOS game claimed shipped to the App Store:
  https://jangwook.net/en/blog/en/tauri-pixijs-ios-game-development/ — **no App Store URL given;
  a solo-dev casual game, not a notable production app.**
- "flow-like" claimed shipped to the iOS App Store with ~80% Rust core:
  https://medium.com/@trivajay259/what-happens-when-you-ship-a-rust-heavy-app-to-the-ios-app-store-with-tauri-2-ddea0584bae7
  — **I searched GitHub for this project and got zero results; no store listing verified.**
  Even this favourable article concedes: "Push notifications … may require custom plugin work",
  "Frontend safe-area issues on iOS can still be annoying", "Some dependencies will fail to
  compile for `aarch64-apple-ios`", and that debugging Rust panics on mobile is "rougher".
- A Japanese dev-agency marketing page claims local clients passed App Store review with Tauri
  apps: https://www.oflight.co.jp/en/columns/tauri-v2-mobile-ios-android — **vendor marketing,
  no named apps, not verifiable.**
- One secondary blog asserts "Apps like Spacedrive, AppFlowy, and Clash Verge run on it in
  production. 1Password moved parts of their app over to Tauri":
  https://www.mayhemcode.com/2026/07/build-mobile-and-desktop-apps-from-one.html
  ⚠️ **This source is partly WRONG — I verified AppFlowy is Flutter, not Tauri.** Do not cite it.
  Its mobile verdict is nonetheless consistent with everything else: "If your product is mobile
  first and mobile only, I'd still lean toward React Native or Flutter today."

### Bottom line for the client

| Question | Answer |
|---|---|
| Is Tauri 2.x proven for **desktop** dev tools? | **Yes, strongly.** GitButler (21.6k★, Tauri 2.11, Svelte, ~daily releases), Yaak (19.1k★, full signed installer matrix + auto-updater), Hoppscotch, Pake (61.2k★), Clash Verge Rev (141k★). |
| Is Tauri 2.x proven for **production iOS/Android**? | **No — unproven.** Zero notable production store apps verified. The flagship project that wanted mobile (Spacedrive) used React Native instead. Real, still-open iOS device-crash and missing-plugin bugs. The Tauri team itself says mobile DX is not on par with desktop. |

**Recommendation implication:** Tauri 2.x is a well-evidenced choice for a **desktop-first**
developer TODO app. If iOS/Android are **must-have launch requirements**, the evidence does not
support betting on Tauri mobile — plan for a separate mobile stack (React Native/Expo, as
Spacedrive did, or Capacitor) or re-evaluate the framework choice.

---

## What I could NOT verify

1. **Why awesome-tauri removed all applications** — PR #876 has an empty body; no stated rationale.
2. **Payload (`payload.app`)** and **Noor (`noor.to`)** — both closed source. Their Tauri usage
   rests solely on the archived awesome-tauri listing; I could not inspect any source or find a
   vendor statement.
3. **Clippy (`github.com/0-don/clippy`)** — repo returns 404 with no redirect; cannot verify at all.
4. **1Password's actual use of Tauri** — 1Password appears as a **sponsor** on https://tauri.app/,
   which is NOT the same as a statement that their product ships on Tauri. Unverified.
5. **Any Tauri mobile app's store listing** — I found no verifiable App Store or Google Play URL
   for a Tauri-built app. Absence of evidence here is itself meaningful, but I cannot prove a
   universal negative: it remains possible that a Tauri mobile app is shipping and simply is not
   publicly documented as such.
6. **Whether Spacedrive intends to migrate off Tauri to GPUI** — `apps/gpui-photo-grid` exists and
   depends on Zed's GPUI, but there is no announcement. Do not report this as a migration.
7. **Obsidian / Cursor exact Electron versions** — both are closed source; I confirmed the
   *framework* from official docs/blogs but not version details.

## Method note

Repo facts were verified by reading actual file contents through the GitHub API
(`Cargo.toml`, `package.json`, `tauri.conf.json`, `pubspec.yaml`) and by checking for the
existence of `src-tauri/` and `gen/apple`/`gen/android` directories, rather than relying on
descriptions or third-party lists. GitHub **code search** was unavailable for part of this
research (HTTP 403 rate limit), so directory/content reads were used instead — this is a stronger
form of evidence anyway. Mission Center was verified through the GitLab API.
