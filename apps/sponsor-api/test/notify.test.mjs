// Unit tests for the async-notification handler (POST /api/sponsor/notify).
//
// The sandbox can never push notifications to localhost, so the signature
// side is exercised here: the test generates its own RSA keypair and configures
// the server to trust the PUBLIC half as the "Alipay public key". Requests
// signed with the private half then verify exactly the way real notifications
// do; anything else must be rejected. (Alipay signs with its own key we never
// see — the trust direction is what matters.)
//
//   node --test test/notify.test.mjs
import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, generateKeyPairSync, sign as rsaSign, createPrivateKey } from "node:crypto";
import { existsSync, readFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = join(dirname(fileURLToPath(import.meta.url)), "..");
const ENV_FILE = join(HERE, ".dev", "sponsor-api.env");
const PORT = 8792;
const BASE = `http://localhost:${PORT}`;

// Skip (not fail) when there are no sandbox credentials to reuse for app_id
// and the gateway guard — the test overrides the key material itself.
const skipReason = !existsSync(ENV_FILE) ? `needs ${ENV_FILE}` : null;
const t = (name, fn, opts = {}) =>
  test(name, skipReason ? { skip: skipReason, ...opts } : opts, fn);

const raw = Object.fromEntries(
  readFileSync(ENV_FILE, "utf8")
    .split("\n")
    .map((l) => l.match(/^\s*([A-Z0-9_]+)=(.*)$/))
    .filter(Boolean)
    .map((m) => [m[1], m[2].trim()]),
);

// Test keypair standing in for Alipay's signing key.
const { publicKey, privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
const alipayPublicKeyPem = publicKey.export({ type: "spki", format: "pem" });
const privObj = createPrivateKey(privateKey.export({ type: "pkcs1", format: "pem" }));

// Canonical Alipay notify string: params sorted by key, k=v joined with &,
// sign (and sign_type per checkNotifySignV2) excluded, values decoded.
function signParams(params) {
  const content = Object.keys(params)
    .filter((k) => k !== "sign" && k !== "sign_type")
    .sort()
    .map((k) => `${k}=${params[k]}`)
    .join("&");
  return rsaSign("sha256", Buffer.from(content, "utf8"), privObj).toString("base64");
}

function formEncode(params) {
  return Object.entries(params)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
    .join("&");
}

const serverDir = mkdtempSync(join(tmpdir(), "sponsor-notify-"));
let server;

before(async () => {
  if (skipReason) return;
  server = spawn(process.execPath, [join(HERE, "src", "server.mjs")], {
    cwd: HERE,
    env: {
      ...process.env,
      SPONSOR_PORT: String(PORT),
      SPONSOR_DB_PATH: join(serverDir, "test.sqlite"),
      ALIPAY_GATEWAY: "https://openapi-sandbox.dl.alipaydev.com/gateway.do",
      ALIPAY_APP_ID: raw.ALIPAY_APP_ID,
      ALIPAY_PRIVATE_KEY: raw.ALIPAY_PRIVATE_KEY,
      // Trust the test keypair instead of the real sandbox Alipay key.
      ALIPAY_PUBLIC_KEY: alipayPublicKeyPem,
      ALIPAY_SELLER_PID: raw.ALIPAY_SELLER_PID || "",
    },
    stdio: ["ignore", "inherit", "inherit"],
  });
  const t0 = Date.now();
  for (;;) {
    try {
      if ((await fetch(`${BASE}/api/healthz`)).ok) break;
    } catch {}
    if (Date.now() - t0 > 15_000) throw new Error("server did not start");
    await new Promise((r) => setTimeout(r, 300));
  }
});

after(() => server?.kill());

async function notify(params) {
  const withSign = { ...params, sign: signParams(params) };
  return fetch(`${BASE}/api/sponsor/notify`, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: formEncode(withSign),
  });
}

async function createOrder(amount = 6) {
  const res = await fetch(`${BASE}/api/sponsor/pay`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ amount }),
  });
  return (await res.json()).orderId;
}

const baseNotify = (over) => ({
  notify_time: "2026-10-30 12:00:00",
  notify_type: "trade_status_sync",
  notify_id: "test-notification-id",
  charset: "utf-8",
  version: "1.0",
  sign_type: "RSA2",
  app_id: raw.ALIPAY_APP_ID,
  trade_no: "2026103022001417370509TEST01",
  out_trade_no: "TBD",
  total_amount: "6.00",
  seller_id: raw.ALIPAY_SELLER_PID || "",
  trade_status: "TRADE_SUCCESS",
  gmt_payment: "2026-10-30 12:00:01",
  ...over,
});

t("valid TRADE_SUCCESS notification marks the order paid and answers success", async () => {
  const orderId = await createOrder(6);
  const res = await notify(baseNotify({ out_trade_no: orderId }));
  assert.equal(res.status, 200);
  assert.equal(await res.text(), "success");
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "paid");
});

t("replayed notification is idempotent and still answers success", async () => {
  const orderId = await createOrder(6);
  const params = baseNotify({ out_trade_no: orderId });
  assert.equal((await notify(params)).status, 200);
  // Same notify_id again (Alipay retries keep notify_id) — must not error.
  const res = await notify({ ...params, notify_id: params.notify_id });
  assert.equal(res.status, 200);
  assert.equal(await res.text(), "success");
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "paid");
});

t("notification signed by a foreign key is rejected", async () => {
  const orderId = await createOrder(6);
  const { privateKey: evilKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
  const content = `out_trade_no=${orderId}&total_amount=6.00&trade_status=TRADE_SUCCESS`;
  const evilSign = rsaSign("sha256", Buffer.from(content), createPrivateKey(evilKey.export({ type: "pkcs1", format: "pem" }))).toString("base64");
  const res = await fetch(`${BASE}/api/sponsor/notify`, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: formEncode({ out_trade_no: orderId, total_amount: "6.00", trade_status: "TRADE_SUCCESS", sign: evilSign }),
  });
  assert.equal(res.status, 400);
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "created", "unsigned notification must not mark paid");
});

t("amount mismatch between notification and order is rejected", async () => {
  const orderId = await createOrder(6);
  const res = await notify(baseNotify({ out_trade_no: orderId, total_amount: "0.01" }));
  assert.equal(res.status, 400);
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "created", "tampered amount must not mark paid");
});

t("notification for a foreign app_id is rejected", async () => {
  const orderId = await createOrder(6);
  const res = await notify(baseNotify({ out_trade_no: orderId, app_id: "2021000000000000" }));
  assert.equal(res.status, 400);
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "created");
});

t("notification for an unknown order is rejected", async () => {
  const res = await notify(baseNotify({ out_trade_no: "SPNdoesnotexist000000" }));
  assert.equal(res.status, 400);
});

t("WAIT_BUYER_PAY does not mark the order paid", async () => {
  const orderId = await createOrder(6);
  const res = await notify(baseNotify({ out_trade_no: orderId, trade_status: "WAIT_BUYER_PAY" }));
  assert.equal(res.status, 200);
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "created");
});

t("seller_id mismatch is rejected when a seller PID is configured", async () => {
  if (!raw.ALIPAY_SELLER_PID) return; // no PID configured — check not active
  const orderId = await createOrder(6);
  const res = await notify(baseNotify({ out_trade_no: orderId, seller_id: "2088000000000000" }));
  assert.equal(res.status, 400);
  const order = await (await fetch(`${BASE}/api/sponsor/order/${orderId}`)).json();
  assert.equal(order.status, "created");
});
