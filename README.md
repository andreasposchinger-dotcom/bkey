# bkey

A tiny secrets vault (passwords, API keys, tokens) for **humans, AI agents
and the browser**. One binary, one file, no platform, no plugin required.

```
Human   ── master passphrase ──► sees everything
Agent   ── own token ──────────► sees only its allowed secrets
Browser ── localhost web UI ───► no plugin, no extension needed
Logins  ── extension (optional)─► autofill + save logins
```

## Why

Agents like Claude Code, Vibe or bots need API keys and passwords, but Vibe
must not be able to read Claude's key and vice versa. bkey does exactly that:
**one token per agent, one ACL per secret.**

## Features

- Single static binary (Rust, no runtime dependencies)
- Encrypted file: `XChaCha20-Poly1305`, key derived with `Argon2id`
- One token per agent: an agent decrypts the vault **without** the master
  passphrase (the data key is wrapped per token), but may only read what
  its ACL allows
- Changing the master passphrase keeps agent tokens valid
- Browser UI via `bkey serve` (localhost, no plugin)
- Optional: `--origin` lets external web pages authenticate via CORS
- **Browser password manager**: a small MV3 extension (`extension/`) fills
  username/password on login pages and saves new logins to the vault
- `zeroize` for passwords in RAM, atomic writes

## Install

```bash
cargo install --git https://github.com/andreasposchinger-dotcom/bkey
# or from a checkout:
cargo install --path .
```

## Quickstart (human)

```bash
bkey init
bkey set openai-key --allow vibe --note "prod key"  # secret is read hidden
bkey set anthropic-key --allow claude-code
bkey get openai-key        # asks for the master passphrase, sees everything
bkey list
```

Store website logins (for the browser extension):

```bash
bkey set github.com --username benedikt --url github.com --allow browser
# --url = hostname of the site (subdomains match automatically)
```

## Set up an agent

**1. Create a token:**

```bash
bkey agent add claude-code
# prints:
#   BKEY_AGENT=claude-code
#   BKEY_TOKEN=bka_....
```

**2. Put the token into the agent config** (examples:
[`examples/claude-code.md`](examples/claude-code.md),
[`examples/codex.md`](examples/codex.md),
[`examples/opencode.md`](examples/opencode.md),
[`examples/vibe.md`](examples/vibe.md) for Mistral Vibe Code CLI,
[`examples/agents-generic.md`](examples/agents-generic.md) for bots,
CI and scripts):

```json
{ "env": { "BKEY_AGENT": "claude-code", "BKEY_TOKEN": "bka_...." } }
```

**3. Allow access to a secret:**

```bash
bkey set my-api-key --allow claude-code    # or --allow claude-code,vibe
```

**4. The agent uses it:**

```bash
bkey get my-api-key        # OK  (token + ACL match)
bkey get other-key         # denied
```

Script-friendly output: `bkey get my-api-key --out env`, `--out json`, or
`--generate` during `set` for freshly generated secrets.

## Browser

```bash
bkey serve                  # http://127.0.0.1:8642
bkey serve --port 9000 --origin https://app.example.com
```

- Unlock the local web UI with the master passphrase → full control
- External web pages (only from the `--origin` allowlist) authenticate with
  an agent token against `/api/auth` and only receive their ACL entries:
  `https://app.example.com` may for example only read `agent:app-example`
  secrets - no browser plugin needed for that.

Security headers: CSP, `no-store`, `nosniff`, session IDs live only in the
server's RAM (gone on exit). `sessionStorage` keeps the session in the tab.

## Browser password manager (extension)

Works like a classic password manager: save logins per website, fill
username/password into login forms with one click - everything inside the
bkey vault.

**Setup:**

```bash
bkey serve                                 # 1. keep the server running
bkey agent add browser                     # 2. agent for the extension
```

3. Chrome/Edge: `chrome://extensions` → *Load unpacked* → select the
   `extension/` folder. Firefox: `about:debugging` → *This Firefox* →
   *Load Temporary Add-on* → `extension/manifest.json`.
4. Open the extension popup: enter server URL (default `127.0.0.1:8642`),
   agent name `browser` + the token from step 2, save.
5. Also in the popup: **unlock with the master passphrase** - only the
   unlocked full mode can save logins.

**Usage:**

- Click a password field → a small **bkey** chip appears next to the field
  → click it to see all matching logins (subdomains match: an entry saved
  with `--url github.com` also fills on `gist.github.com`)
- Click an entry → username + password are filled in
- At the bottom of the menu: **Save login** (name = hostname, username is
  guessed) - stores the currently entered password in the vault

**Modes:**

| Mode | What it can do |
|---|---|
| Agent token (`agent:browser`) | fill only; sees only logins allowed by its ACL |
| Popup unlock (master passphrase) | everything: fill + save |
| Locked | nothing |

Saved web logins automatically get `allow agent:browser`.
Multiple accounts per site: just create several entries with `--url`,
e.g. `github-work` and `github-private`.

Technical notes: MV3, no build step, ~400 lines of plain JS without
dependencies. Fetches run in the background worker with `host_permissions`
for `127.0.0.1` - the visited pages themselves have no access to the vault.
The in-page menu lives in a closed shadow DOM so page styles and scripts
cannot break or scrape it.

## Security model (honest)

- **Agent identity = token possession.** A locally running tool cannot truly
  prove that "Claude Code" is the caller; practically: if you want to
  prevent agent X from reading a secret → do not give it a token for the
  vault or set the ACL accordingly. Each agent only sees its own entries.
- The token lives in the agent config (e.g. `~/.claude/settings.json`) -
  protect it like other credentials (file permissions!)
- Agent names are stored as plaintext metadata in the vault file; secrets,
  passwords and tokens never are.
- Honest limits: no RAM-only store (passphrases are `zeroize()`d, plaintext
  secrets exist in RAM during a session), no rate limits, do not leave
  browser tabs with an unlocked session open.
- Vault format `BKEY2` (see the doc comment in `src/vault.rs`).

## Commands

`bkey help` - all commands, flags and conventions.

## Use as a template

The repo is meant as a starting point for your own vault tools and is
deliberately small: 3 source files (`src/main.rs` = CLI, `src/vault.rs` =
crypto flow, `src/web.rs` = HTTP API), 3 inline files for the web UI and a
build-less MV3 extension in `extension/` (5 small files, no npm). Fork,
rename, publish, add your product keys.

> Modified template published under **bAL-1.0**: credit Andreas Poschinger
> (andreasposchinger-dotcom) as the original author, see the [license](LICENSE).

## Tests

```bash
cargo test
```
