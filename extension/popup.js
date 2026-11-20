// bkey popup: settings storage, unlock/lock via the background worker.

const $ = (id) => document.getElementById(id);
const status = $("status");

function setErr(text) {
  status.textContent = text;
  status.className = "err";
}
function setOk(text) {
  status.textContent = text;
  status.className = "ok";
}

const send = (m) => new Promise((res) => chrome.runtime.sendMessage(m, res));

async function refresh() {
  const r = await send({ type: "status" });
  $("serverUrl").value = r.serverUrl || "";
  const st = await chrome.storage.local.get(["agentName", "agentToken"]);
  $("agentName").value = st.agentName || "";
  $("agentToken").value = st.agentToken || "";
  status.className = "";
  status.textContent =
    (r.unlocked ? "unlocked (full mode: fill + save)" : "locked") +
    (r.agentConfigured ? " | agent token configured" : " | no agent token");
}

$("save").addEventListener("click", async () => {
  const r = await send({
    type: "settings",
    serverUrl: $("serverUrl").value.trim() || "http://127.0.0.1:8642",
    agentName: $("agentName").value.trim(),
    agentToken: $("agentToken").value.trim(),
  });
  if (r && r.ok) setOk("settings saved");
  else setErr("could not save settings");
  refresh();
});

$("unlock").addEventListener("click", async () => {
  const pass = $("passphrase").value;
  if (!pass) return setErr("enter the master passphrase");
  const r = await send({ type: "unlock", passphrase: pass });
  $("passphrase").value = "";
  if (r && r.ok) setOk("unlocked - saving is now possible");
  else setErr((r && r.error) || "unlock failed");
  refresh();
});

$("lock").addEventListener("click", async () => {
  await send({ type: "lock" });
  setOk("locked");
  refresh();
});

refresh();
