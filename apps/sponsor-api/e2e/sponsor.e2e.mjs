// Real-browser E2E against the REAL Alipay sandbox gateway. Local run only —
// CI has neither a browser nor a human to type the sandbox pay password, and
// it skips cleanly when its prerequisites are missing.
//
//   pnpm --filter @todo/sponsor-api e2e
//
// What the machine does: build the landing site, load the sandbox keys from
// .dev/sponsor-api.env (git-ignored), start sponsor-api with the site served
// same-origin, launch a dedicated-profile browser, click the ¥6 tier button
// and verify the sandbox cashier loads — the real gateway has then verified
// our RSA2 signature. After that it waits while YOU complete the sandbox
// payment with the buyer account in the opened window; when the cashier
// redirects back it asserts the thanks banner and the order marked paid.
//
// The async-notification path cannot be exercised here (the sandbox never
// pushes to localhost); it is covered by the notify handler's verification
// rules and must be re-checked in production per docs.
//
// Prerequisites (all missing ⇒ skip):
//   - Playwright's chromium (npx playwright install chromium)
//   - .dev/sponsor-api.env with the sandbox credentials
//   - web/dist (built automatically when missing)
//   - SPONSOR_E2E_PAY_TIMEOUT_MS overrides how long the human gets (default 10 min)
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn, execSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);

const HERE = join(fileURLToPath(import.meta.url), "..", "..");
const ROOT = join(HERE, "..", "..");
const DIST = join(ROOT, "web", "dist");
const ENV_FILE = join(HERE, ".dev", "sponsor-api.env");
const PORT = 8791;
const BASE = `http://localhost:${PORT}`;
const PAY_TIMEOUT = Number(process.env.SPONSOR_E2E_PAY_TIMEOUT_MS || 10 * 60_000);

function loadEnv() {
  if (!existsSync(ENV_FILE)) return null;
  const env = {};
  for (const line of readFileSync(ENV_FILE, "utf8").split("\n")) {
    const m = line.match(/^\s*([A-Z0-9_]+)=(.*)$/);
    if (m) env[m[1]] = m[2].trim();
  }
  return env;
}

const sandbox = loadEnv();
const skipReason =
  !sandbox ? `needs ${ENV_FILE} with the sandbox credentials`
  : !/openapi-sandbox\.dl\.alipaydev\.com/.test(sandbox.ALIPAY_GATEWAY ?? "")
    ? "refusing to run: ALIPAY_GATEWAY is not the sandbox gateway"
  : !existsSync(join(ROOT, "web", "node_modules")) ? "needs web deps (pnpm install)"
  : null;

// skip must be ABSENT when there is no reason: an explicit skip: null makes
// node:test report the test as skipped — swallowing real failures as green.
const t = (name, fn) =>
  test(name, skipReason ? { timeout: 15 * 60_000, skip: skipReason } : { timeout: 15 * 60_000 }, fn);

let server, browser, context, page, orderId;

before(async () => {
  if (skipReason) return;
  if (!existsSync(join(DIST, "zh", "index.html"))) {
    execSync("node build.mjs", { cwd: join(ROOT, "web"), stdio: "inherit" });
  }
  const { chromium } = await import("playwright");
  server = spawn(process.execPath, [join(HERE, "src", "server.mjs")], {
    cwd: HERE,
    env: {
      ...process.env,
      ...sandbox,
      SPONSOR_SERVE_SITE: DIST,
      SPONSOR_DB_PATH: DB_PATH,
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  await waitFor(async () => {
    const r = await fetch(`${BASE}/api/healthz`);
    return r.ok;
  }, 15_000, "sponsor-api to start");
  browser = await chromium.launch({ headless: false });
  context = await browser.newContext({ locale: "zh-CN" });
  page = await context.newPage();
});

after(async () => {
  if (browser) await browser.close();
  if (server) server.kill();
});

async function waitFor(fn, ms, label) {
  const t0 = Date.now();
  for (;;) {
    let v;
    try { v = await fn(); } catch {} // transient navigation/network errors retry
    if (v) return v;
    if (Date.now() - t0 > ms) throw new Error(`timed out after ${Math.round(ms / 1000)}s: ${label}`);
    await sleep(500);
  }
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const DB_PATH = join(tmpdir(), `sponsor-e2e-${Date.now()}.sqlite`);
function latestOrderId() {
  try {
    const { DatabaseSync } = require("node:sqlite");
    const row = new DatabaseSync(DB_PATH, { readOnly: true })
      .prepare(`SELECT order_id FROM orders ORDER BY created_at DESC LIMIT 1`)
      .get();
    return row?.order_id ?? null;
  } catch (err) {
    console.warn(`latestOrderId: ${err.message}`);
    return null;
  }
}

t("sandbox cashier loads — the gateway accepted our RSA2 signature", async () => {
  await page.goto(`${BASE}/zh/`, { waitUntil: "domcontentloaded" });
  const tier = page.locator('[data-sponsor-amount="6"]');
  await tier.scrollIntoViewIfNeeded();
  await tier.click();
  // The sponsor.js injects the page.pay form and submits it; the gateway
  // then forwards to the sandbox cashier (excashier/cashier subdomains).
  await page.waitForURL(/(excashier|cashier)-sandbox\.dl\.alipaydev\.com/, { timeout: 30_000 });
  const cashier = await page.content();
  assert.match(cashier, /支付宝|sandbox|收银台|确认|登录|pay/i, "sandbox cashier did not render");
  // The cashier URL carries payOrderId, not our order id — take the newest.
  orderId = latestOrderId();
  assert.ok(orderId, "an order was created");
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "created");

  console.log(`
──────────────────────────────────────────────────────────
沙箱收银台已打开。请用买家账号完成支付：
  账号   xxsujf8113@sandbox.com
  登录密码 111111   支付密码 111111
等待支付完成（最长 ${Math.round(PAY_TIMEOUT / 60_000)} 分钟）……
──────────────────────────────────────────────────────────`);
});

t("paid order syncs back: thanks banner + order status", async () => {
  await page.waitForURL(/\/(zh|en)\/\?sponsor=/, { timeout: PAY_TIMEOUT });
  const url = new URL(page.url());
  assert.equal(url.searchParams.get("sponsor"), "thanks");
  assert.equal(url.hash, "#free");
  const banner = await page.locator("[data-sponsor-msg]").textContent();
  assert.match(banner, /感谢|到账/);
  // The return endpoint queried the gateway for the truth — not the sync params.
  await waitFor(async () => {
    const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
    return order.status === "paid";
  }, 15_000, "order marked paid");
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.amount, 6);
  console.log(`✅ 全链路通过：订单 ${orderId} 已在沙箱完成支付并回写为 paid`);
});
