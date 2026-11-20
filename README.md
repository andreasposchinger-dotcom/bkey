# bkey

Ein winziger Secrets-Vault (Passwörter, API-Keys, Tokens) für **Menschen,
KI-Agents und den Browser**. Ein Binary, eine Datei, keine Plattform, kein
Plugin.

```
Mensch   ── Master-Passphrase ──► sieht alles
Agent    ── eigener Token ──────► sieht nur seine erlaubten Secrets
Browser  ── localhost Web-UI ───► ohne Plugin, ohne Extension
Login    ── Extension (optional) ► AutoFill + Logins speichern
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
- **Browser-Passwortmanager**: kleine MV3-Extension (`extension/`) füllt
  Username/Passwort auf Login-Seiten und speichert neue Logins im Vault
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

Website-Logins speichern (für die Browser-Extension):

```bash
bkey set github.com --username benedikt --url github.com --allow browser
# --url = Hostname der Seite (Subdomains matchen automatisch)
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

## Browser-Passwortmanager (Extension)

Wie ein klassischer Passwort-Manager: Logins pro Website speichern,
Username/Passwort per Klick ins Login-Formular füllen – alles im bkey-Vault.

**Setup:**

```bash
bkey serve                                 # 1. Server läuft lassen
bkey agent add browser                     # 2. Agent für die Extension
```

3. Chrome/Edge: `chrome://extensions` → *Entpackt laden* → Ordner `extension/`
   auswählen. Firefox: `about:debugging` → *This Firefox* → *Load Temporary
   Add-on* → `extension/manifest.json`.
4. Extension-Popup öffnen: Server-URL (Default `127.0.0.1:8642`), Agent-Name
   `browser` + Token aus Schritt 2 eintragen, speichern.
5. Ebenfalls im Popup: mit **Master-Passphrase entsperren** – nur der
   entsperrte Vollmodus kann Logins speichern.

**Benutzung:**

- Passwortfeld anklicken → neben dem Feld erscheint der kleine **bkey**-Chip
  → Klick zeigt alle passenden Logins (Subdomains matchen: ein Eintrag
  `--url github.com` füllt auch auf `gist.github.com`)
- Eintrag anklicken → Username + Passwort werden eingesetzt
- Unten im Menü: **Login speichern** (Name = Hostname, Username wird
  geraten) – speichert das aktuell eingegebene Passwort im Vault

**Modi:**

| Modus | Was geht |
|---|---|
| Agent-Token (`agent:browser`) | nur füllen, sieht nur per ACL freigegebene Logins |
| Popup-Unlock (Master-Passphrase) | alles: füllen + speichern |
| Gesperrt | nichts |

Gespeicherte Web-Logins bekommen automatisch `allow agent:browser`.
Mehrere Accounts pro Seite: einfach mehrere Einträge mit `--url`
anlegen, z. B. `github-work` und `github-private`.

Technik: MV3, kein Build-Schritt, ~400 Zeilen reines JS ohne Abhängigkeiten.
Fetches laufen im Background-Worker mit `host_permissions` auf
`127.0.0.1` – die besuchten Seiten selbst haben keinerlei Zugriff auf den
Vault. Das In-Page-Menü lebt in einem geschlossenen Shadow-DOM, damit
Seiten-Styles und -Skripte es nicht manipulieren können.

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
Kryptofluss, `src/web.rs` = HTTP-API), 3 Inline-Dateien für die Web-UI und
eine build-lose MV3-Extension in `extension/` (5 kleine Dateien, kein npm).
Forken, umbenennen, aufladen, Produktschlüssel dazubauen.

> Verändertes Template und veröffentlicht dieser unter **bAL-1.0**:
> Nenne Benedikt Durlesser (rasportiy) als Original-Autor, siehe [Lizenz](LICENSE).

## Tests

```bash
cargo test
```
