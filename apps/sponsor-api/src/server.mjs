// Sumery Virgo sponsorship payments — Alipay 电脑网站支付 (alipay.trade.page.pay).
//
// The sponsor screen is a static site; payments need a server: the app
// private key never leaves this process, and async notifications must be
// verified server-side. Orders live in a local SQLite database (node:sqlite,
// no native addon) — the sponsor ledger is append-only and idempotent.
//
// Endpoints:
//   POST /api/sponsor/pay           { amount } → { orderId, paymentHtml }
//   POST /api/sponsor/notify        Alipay async notification (form-encoded)
//   GET  /api/sponsor/return        Alipay sync return — re-queries the trade
//                                   (sync params are advisory only) and
//                                   redirects to the site's sponsor section
//   GET  /api/sponsor/order/:id     { orderId, status } for polling
//   GET  /api/healthz
//
// Configuration comes from the environment (see .dev/sponsor-api.env for the
// sandbox values; that file is git-ignored). The gateway is validated at
// startup so a sandbox key can never answer production traffic.
import express from "express";
import { AlipaySdk } from "alipay-sdk";
import { DatabaseSync } from "node:sqlite";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

const GATEWAY_SANDBOX = "https://openapi-sandbox.dl.alipaydev.com/gateway.do";
const GATEWAY_PROD = "https://openapi.alipay.com/gateway.do";

const env = process.env;
const config = {
  port: Number(env.SPONSOR_PORT || 8791),
  // Where this API is reachable from the outside — used as notify_url.
  publicBase: env.SPONSOR_PUBLIC_BASE || `http://localhost:${env.SPONSOR_PORT || 8791}`,
  // Where the landing site lives — return_url redirects back here.
  siteOrigin: env.SPONSOR_SITE_ORIGIN || "http://localhost:4173",
  // Allowed Origin for the static site's fetches ("" = same-origin).
  corsOrigin: env.SPONSOR_CORS_ORIGIN || "",
  appId: env.ALIPAY_APP_ID || "",
  // PKCS#1 value as returned by the sandbox console — used raw, no PEM
  // surgery (the SDK handles armouring itself).
  privateKey: env.ALIPAY_PRIVATE_KEY || "",
  alipayPublicKey: env.ALIPAY_PUBLIC_KEY || "",
  gateway: env.ALIPAY_GATEWAY || GATEWAY_SANDBOX,
  // Seller PID, when set, rejects notifications whose seller_id mismatches.
  sellerPid: env.ALIPAY_SELLER_PID || "",
  dbPath: env.SPONSOR_DB_PATH || join(HERE, "..", "data", "sponsor.sqlite"),
};

for (const key of ["appId", "privateKey", "alipayPublicKey"]) {
  if (!config[key]) {
    console.error(`sponsor-api: missing ${key} — set it via the environment (.dev/sponsor-api.env)`);
    process.exit(1);
  }
}
if (config.gateway !== GATEWAY_SANDBOX && config.gateway !== GATEWAY_PROD) {
  console.error(`sponsor-api: refusing unknown gateway ${config.gateway}`);
  process.exit(1);
}
export const isSandbox = config.gateway === GATEWAY_SANDBOX;

const alipay = new AlipaySdk({
  appId: config.appId,
  privateKey: config.privateKey,
  alipayPublicKey: config.alipayPublicKey,
  gateway: config.gateway,
});

// --- order store -----------------------------------------------------------

mkdirSync(dirname(config.dbPath), { recursive: true });
const db = new DatabaseSync(config.dbPath);
db.exec(`
  CREATE TABLE IF NOT EXISTS orders (
    order_id     TEXT PRIMARY KEY,
    amount_cents INTEGER NOT NULL,
    status       TEXT NOT NULL DEFAULT 'created', -- created|paid|closed
    trade_no     TEXT,
    buyer_id     TEXT,
    created_at   TEXT NOT NULL,
    paid_at      TEXT,
    updated_at   TEXT NOT NULL
  )
`);

const nowBeijing = () => {
  // Alipay wants yyyy-MM-dd HH:mm:ss; the account runs on UTC+8.
  const pad = (n) => String(n).padStart(2, "0");
  const d = new Date(Date.now() + 8 * 3600_000);
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
};

const newOrderId = () => {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  const stamp = `${d.getUTCFullYear()}${pad(d.getUTCMonth() + 1)}${pad(d.getUTCDate())}` +
    `${pad(d.getUTCHours())}${pad(d.getUTCMinutes())}${pad(d.getUTCSeconds())}`;
  const rand = crypto.randomUUID().replaceAll("-", "").slice(0, 10);
  return `SPN${stamp}${rand}`;
};

function markPaid(orderId, tradeNo, buyerId) {
  const stmt = db.prepare(
    `UPDATE orders SET status = 'paid', trade_no = ?, buyer_id = ?, paid_at = ?, updated_at = ?
     WHERE order_id = ? AND status != 'paid'`,
  );
  const info = stmt.run(tradeNo, buyerId ?? null, nowBeijing(), nowBeijing(), orderId);
  return info.changes > 0; // false = replayed notification — already handled
}

// --- HTTP surface ----------------------------------------------------------

const app = express();
app.disable("x-powered-by");
app.use(express.json({ limit: "8kb" }));
app.use(express.urlencoded({ extended: false, limit: "64kb" }));
if (config.corsOrigin) {
  app.use((req, res, next) => {
    res.setHeader("Access-Control-Allow-Origin", config.corsOrigin);
    res.setHeader("Access-Control-Allow-Methods", "GET,POST,OPTIONS");
    res.setHeader("Access-Control-Allow-Headers", "Content-Type");
    if (req.method === "OPTIONS") return res.sendStatus(204);
    next();
  });
}

const TIERS = new Set([1, 6, 18, 30, 68, 88]);
const MAX_AMOUNT = 100_000;

app.post("/api/sponsor/pay", async (req, res) => {
  const amount = Number(req.body?.amount);
  if (!Number.isFinite(amount) || amount <= 0) {
    return res.status(400).json({ error: "amount must be a positive number" });
  }
  // Tier amounts must match the advertised tiers exactly; custom amounts are
  // rounded to cents. Either way the cashier sees two decimals.
  const cents = Math.round(amount * 100);
  if (cents < 100 || cents > MAX_AMOUNT * 100) {
    return res.status(400).json({ error: `amount must be between 1 and ${MAX_AMOUNT} CNY` });
  }
  // Advertised tiers pass as-is; anything else must be a whole-yuan custom
  // amount (the sponsor screen's 自定义金额 field).
  if (!TIERS.has(amount) && !Number.isInteger(amount)) {
    return res.status(400).json({ error: "amount must be an advertised tier or a whole-yuan custom value" });
  }
  const orderId = newOrderId();
  db.prepare(
    `INSERT INTO orders (order_id, amount_cents, created_at, updated_at) VALUES (?, ?, ?, ?)`,
  ).run(orderId, cents, nowBeijing(), nowBeijing());
  try {
    const paymentHtml = alipay.pageExec("alipay.trade.page.pay", "POST", {
      // Public params — must be part of the signed request or the gateway
      // answers invalid-signature (the most common integration failure).
      notify_url: `${config.publicBase}/api/sponsor/notify`,
      return_url: `${config.publicBase}/api/sponsor/return`,
      bizContent: {
        out_trade_no: orderId,
        total_amount: (cents / 100).toFixed(2),
        subject: "Sumery Virgo 赞助",
        product_code: "FAST_INSTANT_TRADE_PAY",
      },
    });
    res.json({ orderId, paymentHtml });
  } catch (err) {
    db.prepare(`UPDATE orders SET status = 'closed', updated_at = ? WHERE order_id = ?`)
      .run(nowBeijing(), orderId);
    console.error("sponsor-api: page.pay failed:", err.message);
    res.status(502).json({ error: "alipay gateway rejected the request" });
  }
});

app.post("/api/sponsor/notify", async (req, res) => {
  const params = req.body;
  // 1. Verify the notification really came from Alipay.
  const ok = alipay.checkNotifySignV2(params);
  if (!ok) {
    console.warn("sponsor-api: notify failed signature verification");
    return res.status(400).send("fail");
  }
  // 2. The signed payload must describe OUR order, not somebody else's.
  if (params.app_id !== config.appId) return res.status(400).send("fail");
  if (config.sellerPid && params.seller_id && params.seller_id !== config.sellerPid) {
    return res.status(400).send("fail");
  }
  const order = db.prepare(`SELECT * FROM orders WHERE order_id = ?`).get(params.out_trade_no);
  if (!order) return res.status(400).send("fail");
  const paidCents = Math.round(Number(params.total_amount) * 100);
  if (paidCents !== order.amount_cents) {
    console.warn(`sponsor-api: notify amount mismatch for ${order.order_id}`);
    return res.status(400).send("fail");
  }
  // 3. Only these two statuses mean money actually arrived.
  if (params.trade_status === "TRADE_SUCCESS" || params.trade_status === "TRADE_FINISHED") {
    markPaid(order.order_id, params.trade_no, params.buyer_id);
  }
  // Exactly this string, or Alipay retries (4m/10m/10m/1h/2h/6h/15h).
  res.send("success");
});

app.get("/api/sponsor/return", async (req, res) => {
  // Sync return params are advisory; ask the gateway for the truth.
  const orderId = String(req.query.out_trade_no || "");
  const order = db.prepare(`SELECT * FROM orders WHERE order_id = ?`).get(orderId);
  let paid = false;
  if (order) {
    if (order.status === "paid") {
      paid = true;
    } else {
      // alipay-sdk v4 returns camelCase fields (tradeStatus), the notify
      // POST uses snake_case (trade_status) — handle both shapes. The
      // sandbox (and sometimes prod) lags a moment behind the cashier, so
      // retry the query before concluding "pending".
      for (let attempt = 0; attempt < 4 && !paid; attempt++) {
        if (attempt > 0) await new Promise((r) => setTimeout(r, 1500));
        try {
          const result = await alipay.exec("alipay.trade.query", {
            bizContent: { out_trade_no: orderId },
          });
          const status = result?.tradeStatus ?? result?.trade_status;
          if (status === "TRADE_SUCCESS" || status === "TRADE_FINISHED") {
            markPaid(order.order_id, result.tradeNo ?? result.trade_no, result.buyerUserId ?? result.buyer_user_id);
            paid = true;
          } else if (status !== "WAIT_BUYER_PAY") {
            break; // closed/finished-not-success — stop retrying
          }
        } catch (err) {
          // ACQ.TRADE_NOT_EXIST etc. — retry once or twice, then give up.
          console.warn(`sponsor-api: return query for ${orderId}: ${err.message}`);
        }
      }
    }
  }
  const lang = req.query.lang === "en" ? "en" : "zh";
  res.redirect(302, `${config.siteOrigin}/${lang}/?sponsor=${paid ? "thanks" : "pending"}#free`);
});

app.get("/api/sponsor/order/:id", (req, res) => {
  const order = db.prepare(`SELECT order_id, status, amount_cents FROM orders WHERE order_id = ?`)
    .get(req.params.id);
  if (!order) return res.status(404).json({ error: "unknown order" });
  res.json({ orderId: order.order_id, status: order.status, amount: order.amount_cents / 100 });
});

app.get("/api/healthz", (_req, res) => res.json({ ok: true, sandbox: isSandbox }));

if (process.env.SPONSOR_SERVE_SITE) {
  // Convenience for local E2E: also serve the built landing site.
  const siteDist = process.env.SPONSOR_SERVE_SITE;
  app.use(express.static(siteDist));
}

const server = app.listen(config.port, () => {
  console.log(
    `sponsor-api listening on :${config.port} (${isSandbox ? "SANDBOX" : "PRODUCTION"} gateway ${config.gateway})`,
  );
});

for (const sig of ["SIGINT", "SIGTERM"]) {
  process.on(sig, () => server.close(() => process.exit(0)));
}
// Unexpected errors must be visible — a silently dead payment server looks
// exactly like a connection-refused payment server.
process.on("uncaughtException", (err) => console.error("sponsor-api: uncaught:", err));
process.on("unhandledRejection", (err) => console.error("sponsor-api: unhandled rejection:", err));
