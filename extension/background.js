// bkey extension background: talks to the local bkey server (`bkey serve`).
// Two auth modes, used automatically:
//   1. human session  (popup unlock, full access incl. saving logins)
//   2. agent token    (fill-only, sees only entries its ACL allows)
// Fetches run here, not in the content script, so the pages themselves
// never touch the vault and no CORS setup is needed (host_permissions).

const DEFAULT_SERVER = "http://127.0.0.1:8642";

async function getCfg() {
  const c = await chrome.storage.local.get(["serverUrl", "agentName", "agentToken", "humanSid"]);
  return {
    serverUrl: c.serverUrl || DEFAULT_SERVER,
    agentName: c.agentName || "",
    agentToken: c.agentToken || "",
    humanSid: c.humanSid || null,
  };
}

let agentSid = null; // in-memory; re-authenticated lazily

async function serverFetch(path, method, payload, sid) {
  const cfg = await getCfg();
  const opts = { method, headers: { "Content-Type": "application/json" } };
  if (sid) opts.headers["Authorization"] = "Bearer " + sid;
  if (payload !== undefined && payload !== null) opts.body = JSON.stringify(payload);
  const r = await fetch(cfg.serverUrl + path, opts);
  let body = {};
  try { body = await r.json(); } catch (_) { /* empty body */ }
  return { status: r.status, body };
}

async function obtainAgentSid(cfg) {
  if (agentSid || !cfg.agentName || !cfg.agentToken) return null;
  const r = await serverFetch("/api/auth", "POST", {
    agent: cfg.agentName,
    token: cfg.agentToken,
  });
  if (r.status === 200 && r.body.sid) agentSid = r.body.sid;
  return agentSid;
}

// Human session first, agent token as fallback.
async function anySid() {
  const cfg = await getCfg();
  return cfg.humanSid || (await obtainAgentSid(cfg));
}

async function dropStaleHumanSid() {
  await chrome.storage.local.remove("humanSid");
}

// Does this entry (url = site hostname) match the visited host?
function matchHost(pageHost) {
  const h = String(pageHost).toLowerCase();
  return (e) => {
    if (!e.url) return false;
    let u = String(e.url).toLowerCase();
    u = u.replace(/^https?:\/\//, "").replace(/\/.*$/, "").replace(/:\d+$/, "");
    return h === u || h.endsWith("." + u);
  };
}

async function handle(msg) {
  switch (msg.type) {
    case "status": {
      const cfg = await getCfg();
      return {
        serverUrl: cfg.serverUrl,
        agentConfigured: !!(cfg.agentName && cfg.agentToken),
        unlocked: !!cfg.humanSid,
      };
    }
    case "settings": {
      await chrome.storage.local.set({
        serverUrl: msg.serverUrl,
        agentName: msg.agentName,
        agentToken: msg.agentToken,
      });
      agentSid = null; // force re-auth with the new settings
      return { ok: true };
    }
    case "unlock": {
      await dropStaleHumanSid();
      const r = await serverFetch("/api/unlock", "POST", {
        passphrase: msg.passphrase,
      });
      if (r.status !== 200) {
        return { ok: false, error: r.body.error || "unlock failed (is bkey serve running?)" };
      }
      await chrome.storage.local.set({ humanSid: r.body.sid });
      return { ok: true };
    }
    case "lock": {
      const cfg = await getCfg();
      if (cfg.humanSid) {
        await serverFetch("/api/lock", "POST", { sid: cfg.humanSid });
      }
      await dropStaleHumanSid();
      agentSid = null;
      return { ok: true };
    }
    case "lookup": {
      const sid = await anySid();
      if (!sid) return { needsAuth: true };
      const cfg = await getCfg();
      const r = await serverFetch("/api/entries", "GET", null, sid);
      if (r.status === 401) {
        if (sid === cfg.humanSid) await dropStaleHumanSid();
        agentSid = null;
        return { needsAuth: true };
      }
      if (r.status !== 200) return { error: r.body.error || "entries request failed" };
      const entries = (r.body.entries || []).filter(matchHost(msg.host));
      return {
        entries: entries.map((e) => ({
          name: e.name,
          username: e.username,
          url: e.url,
        })),
      };
    }
    case "reveal": {
      const sid = await anySid();
      if (!sid) return { needsAuth: true };
      const cfg = await getCfg();
      const r = await serverFetch(
        "/api/entry/" + encodeURIComponent(msg.name),
        "GET",
        null,
        sid
      );
      if (r.status === 401) {
        if (sid === cfg.humanSid) await dropStaleHumanSid();
        agentSid = null;
        return { needsAuth: true };
      }
      if (r.status !== 200) return { error: r.body.error || "no access to this entry" };
      return { secret: r.body.secret, username: r.body.username || "" };
    }
    case "save": {
      const cfg = await getCfg();
      if (!cfg.humanSid) return { needsUnlock: true };
      const r = await serverFetch(
        "/api/set",
        "POST",
        {
          name: msg.name,
          secret: msg.secret,
          username: msg.username || "",
          url: msg.url || "",
          allow: msg.allow || ["agent:browser"],
        },
        cfg.humanSid
      );
      if (r.status === 401) {
        await dropStaleHumanSid();
        return { needsUnlock: true };
      }
      if (r.status !== 200) return { error: r.body.error || "save failed" };
      return { ok: true };
    }
    default:
      return { error: "unknown message type" };
  }
}

chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  handle(msg)
    .then(sendResponse)
    .catch((e) => sendResponse({ error: String((e && e.message) || e) }));
  return true; // keep the message channel open for the async response
});
