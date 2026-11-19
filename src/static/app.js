/* bkey web ui - vanilla JS, no deps */
const $ = (id) => document.getElementById(id);
let sid = sessionStorage.getItem('bkey-sid') || null;

async function api(path, opts = {}) {
  opts.headers = Object.assign({ 'Content-Type': 'application/json' }, opts.headers);
  if (sid) opts.headers['Authorization'] = 'Bearer ' + sid;
  const r = await fetch(path, opts);
  let j = {};
  try { j = await r.json(); } catch (_) {}
  if (!r.ok) throw new Error(j.error || ('HTTP ' + r.status));
  return j;
}

function showErr(msg) {
  const e = $('err');
  e.textContent = msg;
  e.hidden = false;
}

function showVault(who) {
  $('login').hidden = true;
  $('vault').hidden = false;
  $('who').textContent = who;
}

async function load() {
  const ul = $('list');
  ul.textContent = '';
  const { principal, entries } = await api('/api/entries');
  $('who').textContent = principal;
  const q = $('q').value.trim().toLowerCase();
  for (const e of entries) {
    if (q && !(e.name + ' ' + e.note).toLowerCase().includes(q)) continue;
    const li = document.createElement('li');
    li.className = 'entry';

    const name = document.createElement('span');
    name.className = 'name';
    name.textContent = e.name;

    const acl = document.createElement('span');
    acl.className = 'acl';
    acl.textContent = e.acl.length ? e.acl.join(' ') : '(human only)';

    const note = document.createElement('span');
    note.className = 'note';
    note.textContent = e.note || '';

    const value = document.createElement('code');
    value.className = 'secret';
    value.hidden = true;

    const reveal = document.createElement('button');
    reveal.textContent = 'reveal';
    reveal.onclick = async () => {
      if (!value.hidden) { value.hidden = true; reveal.textContent = 'reveal'; return; }
      try {
        const j = await api('/api/entry/' + encodeURIComponent(e.name));
        value.textContent = j.secret;
        value.hidden = false;
        reveal.textContent = 'hide';
      } catch (ex) { showErr(ex.message); }
    };

    const copy = document.createElement('button');
    copy.textContent = 'copy';
    copy.onclick = async () => {
      try {
        const j = await api('/api/entry/' + encodeURIComponent(e.name));
        await navigator.clipboard.writeText(j.secret);
        copy.textContent = 'copied';
        setTimeout(() => (copy.textContent = 'copy'), 1200);
      } catch (ex) { showErr(ex.message); }
    };

    const row = document.createElement('div');
    row.className = 'row';
    row.append(reveal, copy);
    li.append(name, acl, note, row, value);
    ul.append(li);
  }
}

$('unlock').onclick = async () => {
  try {
    const j = await api('/api/unlock', {
      method: 'POST',
      body: JSON.stringify({ passphrase: $('pass').value }),
    });
    sid = j.sid;
    sessionStorage.setItem('bkey-sid', sid);
    $('pass').value = '';
    showVault('human');
    await load();
  } catch (ex) { showErr(ex.message); }
};

$('auth').onclick = async () => {
  try {
    const j = await api('/api/auth', {
      method: 'POST',
      body: JSON.stringify({ agent: $('agent').value.trim(), token: $('token').value }),
    });
    sid = j.sid;
    sessionStorage.setItem('bkey-sid', sid);
    $('token').value = '';
    showVault(j.principal);
    await load();
  } catch (ex) { showErr(ex.message); }
};

$('lock').onclick = async () => {
  try { await api('/api/lock', { method: 'POST', body: JSON.stringify({ sid }) }); } catch (_) {}
  sid = null;
  sessionStorage.removeItem('bkey-sid');
  location.reload();
};

$('q').oninput = () => { if (sid) load().catch((e) => showErr(e.message)); };

(async () => {
  if (!sid) return;
  try {
    const j = await api('/api/entries');
    showVault(j.principal);
    load().catch(() => {});
  } catch (_) {
    sid = null;
    sessionStorage.removeItem('bkey-sid');
  }
})();
