// Sponsor tier buttons → sponsor-api → Alipay 电脑网站支付 cashier.
//
// The pay endpoint returns an auto-submitting HTML form (pageExec output).
// It MUST be rendered into the page and submitted as a form — navigating to
// it as a URL only shows the raw parameters and never reaches Alipay.
// Without a configured API (meta[name=sponsor-api] empty) this script does
// nothing: the buttons only render when the build configures them anyway.
(function () {
  "use strict";
  var meta = document.querySelector('meta[name="sponsor-api"]');
  var base = (meta && meta.content.replace(/\/$/, "")) || "";

  function msg(text) {
    var el = document.querySelector("[data-sponsor-msg]");
    if (!el) return;
    el.textContent = text;
    el.hidden = !text;
  }

  function pay(amount) {
    msg("");
    fetch(base + "/api/sponsor/pay", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ amount: amount }),
    })
      .then(function (r) {
        return r.json().then(function (body) { return { ok: r.ok, body: body }; });
      })
      .then(function (res) {
        if (!res.ok || !res.body.paymentHtml) {
          msg(res.body && res.body.error ? res.body.error : "支付启动失败，请稍后再试。");
          return;
        }
        var container = document.createElement("div");
        container.innerHTML = res.body.paymentHtml;
        container.style.position = "fixed";
        container.style.inset = "0";
        container.style.zIndex = "9999";
        container.style.background = "#fff";
        document.body.appendChild(container);
        var form = container.querySelector("form");
        if (form) form.submit();
      })
      .catch(function () {
        msg("网络错误，请稍后再试。");
      });
  }

  document.addEventListener("click", function (e) {
    var btn = e.target.closest && e.target.closest("[data-sponsor-amount]");
    if (!btn) return;
    pay(Number(btn.getAttribute("data-sponsor-amount")));
  });

  var custom = document.querySelector(".tiers li:last-child");
  if (custom) {
    custom.addEventListener("click", function () {
      var input = window.prompt("请输入赞助金额（元，整数）：", "18");
      if (input === null) return;
      var amount = Number(input);
      if (!Number.isInteger(amount) || amount < 1) {
        msg("请输入一个正整数金额。");
        return;
      }
      pay(amount);
    });
  }

  // Sync return lands on ?sponsor=thanks|pending#free — say what happened.
  // The server has already verified the real status via trade.query; the URL
  // flag is only display text, never the payment decision.
  var params = new URLSearchParams(location.search);
  var sponsor = params.get("sponsor");
  if (sponsor === "thanks") msg("赞助已到账，真的非常感谢。这将直接支付 Apple 公证与服务器开销。");
  else if (sponsor === "pending") msg("订单还没确认完成，稍后如果到账了会再次谢谢你。");
})();
