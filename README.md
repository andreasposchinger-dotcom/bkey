# bkey

Ein winziger Secrets-Vault (Passwörter, API-Keys, Tokens) für **Menschen,
KI-Agents und den Browser**. Ein Binary, eine Datei, keine Plattform, kein
Plugin.

```
Mensch   ── Master-Passphrase ──► sieht alles
Agent    ── eigener Token ──────► sieht nur seine erlaubten Secrets
Browser  ── localhost Web-UI ───► ohne Plugin, ohne Extension
```

## Warum

Agenten wie Claude Code, Vibe oder Bots brauchen API-Keys und Passwörter,
 aber dem Claude-Key soll Vibe nicht lesen können und umgekehrt. bkey macht
genau das: **pro Agent ein Token, pro Secret eine ACL.**

## Features

- Ein einziges Binary (Rust, ~keine Weiteren Abhängigkeiten zur Laufzeit)
- Verschlüsselte Datei: `XChaCha20-Poly1305`, Schlüssel aus `Argon2id`
- Pro Agent ein Token: Agent entschlüsselt den Vault **ohne** Master-Passphrase
  (der Data-Key ist pro Token gewrappt), darf aber nur ACL-Erlaubtes lesen
- Master-Passphrase-Wechselung hält Agenten-Tokens gültig
- Browser-UI über `bkey serve` (localhost, ohne Plugin)
- Optional: `--origin` erlaubt externen Webseiten die Token-Auth via CORS
- `zeroize` für Passwörter im RAM, atomic writes

## Installieren

```bash
cargo install --git https://github.com/rasportiy/bkey
# oder aus dem Checkout:
cargo install --path .
```

## Schnellstart (Mensch)

```bash
bkey init
bkey set openai-key --allow vibe --note "prod key"   # Secret wird versteckt abgefragt
bkey set anthropic-key --allow claude-code
bkey get openai-key        # fragt Master-Passphrase, sieht alles
bkey list
```

## Agent einrichten

**1. Token erstellen:**

```bash
bkey agent add claude-code
# druckt:
#   BKEY_AGENT=claude-code
#   BKEY_TOKEN=bka_....
```

**2. Token in die Agenten-Config** (Beispiele: [`examples/claude-code.md`](examples/claude-code.md),
[`examples/agents-generic.md`](examples/agents-generic.md)):

```json
{ "env": { "BKEY_AGENT": "claude-code", "BKEY_TOKEN": "bka_...." } }
```

**3. Secret freigeben**:

```bash
bkey set my-api-key --allow claude-code    # oder --allow claude-code,vibe
```

**4. Agent nutzt:**

```bash
bkey get my-api-key        # OK  (Token + ACL stimmen)
bkey get other-key         # denied
```

CLI-Ausgabe für Skripte: `bkey get my-api-key --out env`, `--out json` oder
`--generate` beim `set` für frisch generierte Secrets.

## Browser

```bash
bkey serve                  # http://127.0.0.1:8642
bkey serve --port 9000 --origin https://app.example.com
```

- Lokale Web-UI entsperren: Master-Passphrase → über alles volle Kontrolle
- Externe Webseiten (nur aus `--origin`-Allowlist) authentifizieren sich per
  Agent-Token gegen `/api/auth` und bekommen nur ihre ACL-Einträge:
  damit darf `https://app.example.com` z.B. nur `agent:app-example` Secrets
  lesen - ohne Browser-Plugin.

Sicherheits-Header: CSP, `no-store`, `nosniff`, Session-IDs nur im RAM des
Servers (beim Beenden weg). `sessionStorage` hält die Session nur im Tab.

## Sicherheitsmodell (ehrlich)

- **Agent-Identität = Token-Besitz.** Ob "Claude Code" wirklich der Aufrufer
  ist, kann ein lokal ausführbares Tool nur bedingt beweisen; Faktisch gilt:
  möchten Sie verhindern, dass ein Agent X das Secret liest → geben Sie ihm
  kein Token auf den Vault bzw. setzen Sie die ACL entsprechend. Jeder
  Agent sieht nur seine eigenen Einträge.
- Der Token liegt in der Agenten-Config (z.B. `~/.claude/settings.json`),
  also dort schützen wie andere Credentials (Dateisystemrechte!)
- Agentennamen stehen in Klartext-Metadaten der Vaultdatei; Secrets,
  Passwörter und Tokens selbst nie.
- Ehrliche Limits: kein RAM-Only-Store (Passphrasen werden `zeroize()`t,
  Klartext-Secrets im RAM während einer Session), keine Rate-Limits,
  Browser-Tabs mit entsperrter Session offen lassen.
- Vault-Format `BKEY2` (siehe `src/vault.rs`-Dokublock im Code).

## Befehle

`bkey help` – alle Befehle, Flags und Konventionen.

## Als Template nutzen

Das Repo ist als Ausgangspunkt für eigene Vault-Tools gedacht und bewusst
klein gehalten: 3 Quellcode-Dateien (`src/main.rs` = CLI, `src/vault.rs` =
Kryptofluss, `src/web.rs` = HTTP-API) und 3 Inline-Dateien für die Web-UI.
Forken, umbenennen, aufladen, Produktschlüssel dazubauen.

> Verändertes Template und veröffentlicht dieser unter **bAL-1.0**:
> Nenne Benedikt Durlesser (rasportiy) als Original-Autor, siehe [Lizenz](LICENSE).

## Tests

```bash
cargo test
```
