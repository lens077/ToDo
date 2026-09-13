(function () {
  var channel = "todo-dsh-firefox-v1";
  window.addEventListener("message", function (event) {
    if (event.source !== window || event.origin !== window.location.origin) return;
    var data = event.data;
    if (!data || data.channel !== channel || data.type !== "open-session") return;
    browser.runtime.sendMessage({ channel: channel, type: "open-session", requestId: data.requestId, url: data.url, expiresAt: Date.now() + 1500 });
  });
})();
