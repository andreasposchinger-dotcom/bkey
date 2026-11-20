// bkey content script: password-field detection, autofill and save UI.
// UI lives in a closed shadow root so page styles cannot break it and
// page scripts cannot trivially scrape the menu contents.

(() => {
  if (window.__bkeyLoaded) return;
  window.__bkeyLoaded = true;

  const send = (m) =>
    new Promise((resolve) => {
      try {
        chrome.runtime.sendMessage(m, (r) => resolve(r || { error: "no response" }));
      } catch (e) {
        resolve({ error: "extension reloading - reload the page" });
      }
    });

  // ------------------------------------------------------------ fields

  function visible(el) {
    if (!el) return false;
    if (el.disabled || el.readOnly) return false;
    const t = (el.type || "").toLowerCase();
    if (t === "hidden") return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  }

  function pwFields() {
    return [...document.querySelectorAll("input[type=password]")].filter(visible);
  }

  // Best guess for the username input near a password field.
  function guessUserField(pw) {
    const scope = pw.closest("form") || document;
    const candidates = [...scope.querySelectorAll("input")].filter(visible);
    const idx = candidates.indexOf(pw);
    for (let i = idx - 1; i >= 0; i--) {
      const f = candidates[i];
      const t = (f.type || "text").toLowerCase();
      if (["text", "email", "tel"].includes(t)) return f;
    }
    const hinted = ["autocomplete=username", "autocomplete=email", "name*=user i",
      "id*=user i", "name*=email i", "name*=mail i", "name*=login i"].join(",");
    const h = scope.querySelector(hinted);
    return visible(h) ? h : null;
  }

  function setValue(el, v) {
    if (!el || v === null || v === undefined) return;
    // use the native value setter so framework overrides notice the change
    const proto = Object.getPrototypeOf(el);
    const desc = Object.getOwnPropertyDescriptor(proto, "value");
    try {
      if (desc && desc.set) desc.set.call(el, v);
      else el.value = v;
    } catch (_) {
      el.value = v;
    }
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }

  // ------------------------------------------------------------ ui

  const host = document.createElement("div");
  host.style.cssText = "all:initial;position:static;z-index:0;";
  const shadow = host.attachShadow({ mode: "closed" });
  shadow.innerHTML = `
<style>
  :host { all: initial; }
  * { box-sizing: border-box; font: 13px/1.35 system-ui, sans-serif; }
  #btn {
    position: fixed; z-index: 2147483646; cursor: pointer;
    padding: 2px 8px; border-radius: 6px; border: 1px solid #567;
    background: #16181d; color: #fff; font-size: 12px; font-weight: 600;
    opacity: .92; user-select: none;
  }
  #btn:hover { opacity: 1; }
  #menu {
    position: fixed; z-index: 2147483647; width: 280px; max-height: 320px;
    overflow: auto; border-radius: 10px; padding: 10px;
    background: #16181d; color: #e8e8ec; box-shadow: 0 8px 30px rgba(0,0,0,.45);
  }
  #menu h3 { margin: 0 0 8px; font-size: 12px; text-transform: uppercase;
    letter-spacing: .08em; color: #9aa3b2; font-weight: 700; }
  .e { display: block; width: 100%; text-align: left; margin: 4px 0;
    padding: 7px 9px; border-radius: 7px; border: 1px solid #343946;
    background: #1e222b; color: #fff; cursor: pointer; }
  .e:hover { background: #2a3040; }
  .e .u { color: #9fd0ff; font-weight: 600; }
  .e small { display: block; color: #8b93a3; font-size: 11px; margin-top: 2px; }
  input { width: 100%; margin: 3px 0; padding: 6px 8px; border-radius: 6px;
    border: 1px solid #343946; background: #101318; color: #fff; }
  button { padding: 6px 10px; margin-top: 5px; border-radius: 6px; border: 0;
    background: #3b74e0; color: #fff; font-weight: 600; cursor: pointer; }
  button.sec { background: #2a3040; }
  .msg { margin-top: 6px; font-size: 12px; color: #ffd28a; }
  hr { border: 0; border-top: 1px solid #2c313c; margin: 9px 0; }
  .dim { color: #8b93a3; font-size: 12px; }
</style>
<div id="btn" hidden>bkey</div>
<div id="menu" hidden></div>`;
  document.documentElement.appendChild(host);

  const btn = shadow.getElementById("btn");
  const menu = shadow.getElementById("menu");
  let currentPw = null;

  function placeBtn(pw) {
    currentPw = pw;
    const r = pw.getBoundingClientRect();
    btn.hidden = false;
    btn.style.top = Math.max(2, r.top + 2) + "px";
    btn.style.left = Math.max(2, r.right - btn.offsetWidth - 30) + "px";
  }

  function hideMenu() {
    menu.hidden = true;
    menu.innerHTML = "";
  }

  function placeMenu() {
    const r = btn.getBoundingClientRect();
    menu.hidden = false;
    // menu appears in view; btn.offsetWidth is 0 while hidden -> measure now
    const w = 290, h = Math.min(320, menu.scrollHeight + 20);
    let x = Math.min(window.innerWidth - w - 8, r.right - w / 2);
    x = Math.max(8, x);
    let y = r.bottom + 6;
    if (y + h > window.innerHeight) y = Math.max(8, r.top - h - 6);
    menu.style.left = x + "px";
    menu.style.top = y + "px";
  }

  // ------------------------------------------------------------ menu

  function el(tag, attrs = {}, text = "") {
    const n = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
    if (text) n.textContent = text;
    return n;
  }

  function msg(text) {
    menu.appendChild(el("div", { class: "msg" }, text));
  }

  async function menuFor(pw) {
    menu.innerHTML = "";
    menu.appendChild(el("h3", {}, "bkey - " + location.hostname));

    const r = await send({ type: "lookup", host: location.hostname });
    if (r.needsAuth || r.error) {
      if (r.needsAuth) {
        msg("No access: agent token missing/wrong or bkey serve is not running.");
        msg("Open the bkey popup in the toolbar and configure it / unlock.");
      } else {
        msg("Fehler: " + r.error);
      }
      placeMenu();
      return;
    }

    if (r.entries.length) {
      for (const e of r.entries) {
        const b = el("button", {
          class: "e",
          title: "fill " + e.name,
        });
        const u = el("span", { class: "u" }, e.username || e.name);
        b.appendChild(u);
        b.appendChild(el("small", {}, e.url ? e.url : e.name));
        b.addEventListener("click", () => fillFrom(e, pw));
        menu.appendChild(b);
      }
    } else {
      menu.appendChild(el("div", { class: "dim" }, "No logins saved for this site."));
    }

    menu.appendChild(el("hr"));
    saveSection();
    placeMenu();
  }

  function saveSection() {
    menu.appendChild(el("h3", {}, "Save login"));
    const nameIn = el("input", {
      value: location.hostname,
      placeholder: "name, e.g. " + location.hostname,
    });
    const userIn = el("input", { placeholder: "username" });
    const guessed = guessUserField(currentPw);
    if (guessed) userIn.value = guessed.value || "";
    const saveBtn = el("button", {}, "Save password from field");
    const hint = el("div", { class: "dim" }, "Stores the current password field.");
    saveBtn.addEventListener("click", async () => {
      if (!currentPw || !currentPw.value) {
        msg("Password field is empty.");
        return;
      }
      const r = await send({
        type: "save",
        name: nameIn.value.trim() || location.hostname,
        username: userIn.value.trim(),
        url: location.hostname,
        secret: currentPw.value,
      });
      if (r.ok) {
        menu.innerHTML = "";
        menu.appendChild(el("h3", {}, "bkey"));
        msg("Saved.");
      } else if (r.needsUnlock) {
        msg("Saving requires the master passphrase: unlock in the bkey popup.");
      } else {
        msg("Fehler: " + (r.error || "save failed"));
      }
    });
    menu.appendChild(nameIn);
    menu.appendChild(userIn);
    menu.appendChild(saveBtn);
    menu.appendChild(hint);
  }

  async function fillFrom(e, pw) {
    const r = await send({ type: "reveal", name: e.name });
    if (r.needsAuth) {
      msg("No access (check token / bkey serve, open the popup).");
      return;
    }
    if (r.error) {
      msg("Fehler: " + r.error);
      return;
    }
    setValue(pw, r.secret);
    const uf = guessUserField(pw);
    if (uf && !uf.value) setValue(uf, e.username || r.username || "");
    hideMenu();
    pw.focus();
  }

  // ------------------------------------------------------------ wiring

  document.addEventListener(
    "focusin",
    (ev) => {
      const t = ev.target;
      if (
        t &&
        t.tagName === "INPUT" &&
        (t.type || "").toLowerCase() === "password" &&
        pwFields().includes(t)
      ) {
        placeBtn(t);
      } else if (t && t.tagName === "INPUT") {
        // focus moved to a non-password input: hide the chip.
        // (focus on body/menu keeps it visible so the click still works)
        btn.hidden = true;
      }
    },
    true
  );

  btn.addEventListener("click", () => {
    if (currentPw) menuFor(currentPw);
  });

  document.addEventListener(
    "mousedown",
    (ev) => {
      if (!menu.hidden && !ev.composedPath().includes(host)) hideMenu();
    },
    true
  );

  window.addEventListener("resize", () => {
    if (!btn.hidden && currentPw) placeBtn(currentPw);
  });

  hideMenu();
})();
