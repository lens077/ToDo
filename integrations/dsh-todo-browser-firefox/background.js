/* Firefox MV2 background; pure helpers are exported for node tests. */
(function (root) {
  var CHANNEL = "todo-dsh-firefox-v1";
  function validUrl(raw) {
    try {
      var u = new URL(raw);
      if ((u.origin !== "http://127.0.0.1:3080" && u.origin !== "http://localhost:3080") || u.pathname !== "/" || u.hash || u.searchParams.size !== 1 || !u.searchParams.has("session")) return false;
      var s = u.searchParams.get("session");
      return !!s && /^[A-Za-z0-9_-]+$/.test(s) && new TextEncoder().encode(s).length <= 128;
    } catch (_) { return false; }
  }
  function senderAllowed(sender) {
    if (!sender || !sender.tab || sender.frameId !== 0) return false;
    var o; try { o = new URL(sender.url).origin; } catch (_) { return false; }
    return o === "http://127.0.0.1:3080" || o === "http://localhost:3080" || o === (typeof DSH_TODO_APP_ORIGIN === "string" ? DSH_TODO_APP_ORIGIN : "http://localhost:1420");
  }
  function choose(tabs, target) {
    var matches = tabs.filter(function (t) { return t.url && new URL(t.url).origin === target.origin; });
    return matches.sort(function (a,b) { return (b.active ? 1 : 0) - (a.active ? 1 : 0) || a.id - b.id; })[0];
  }
  function install(api) {
    var busy = Promise.resolve();
    api.runtime.onMessage.addListener(function (msg, sender) {
      if (!msg || msg.channel !== CHANNEL || msg.type !== "open-session" || !senderAllowed(sender)) return;
      var reply = function () { if (!validUrl(msg.url)) return {channel:CHANNEL,requestId:msg.requestId,type:"open-session-result",ok:false,error:"invalid url"}; if (msg.expiresAt && Date.now() > msg.expiresAt) return {channel:CHANNEL,requestId:msg.requestId,type:"open-session-result",ok:false,error:"expired"}; var target=new URL(msg.url); return api.tabs.query({}).then(function(tabs){var hit=choose(tabs,target); if(hit) return api.tabs.update(hit.id,{url:msg.url,active:true}).then(function(){return api.windows.update(hit.windowId,{focused:true});}).then(function(){return {channel:CHANNEL,requestId:msg.requestId,type:"open-session-result",ok:true,action:"reused"};}); return api.tabs.create({url:msg.url,active:true}).then(function(t){return api.windows.update(t.windowId,{focused:true});}).then(function(){return {channel:CHANNEL,requestId:msg.requestId,type:"open-session-result",ok:true,action:"created"};}); }); };
      busy = busy.then(reply, reply); return busy.then(function (r) { return r; });
    });
  }
  root.DSHFirefox = { validUrl: validUrl, senderAllowed: senderAllowed, choose: choose, install: install };
  if (typeof browser !== "undefined") install(browser);
})(typeof globalThis !== "undefined" ? globalThis : this);
