# Generische Agenten (Vibe, Bots, CI, Skripte)

bkey identifiziert Agenten über zwei Umgebungsvariablen:

```bash
BKEY_AGENT=<name>
BKEY_TOKEN=<token>
```

Wie die in den Agent-Prozess kommen, ist dem Tool egal - es müssen am Ende
Umgebungsvariablen des Prozesses sein, der `bkey` aufruft.

## Variante A: Shell-Profil / pro Projekt

```bash
# pro Projekt, z.B. in deinem Projekt-Env-File oder direnv (.envrc):
export BKEY_AGENT=vibe
export BKEY_TOKEN=bka_....

bkey set project-api-key --allow vibe
bkey get project-api-key
```

## Variante B: Wrapper-Skript

```bash
#!/usr/bin/env bash
# /usr/local/bin/vault-get NAME
BKEY_AGENT=cloud-bot BKEY_TOKEN="$(cat ~/.config/cloud-bot/token)" \
  exec bkey get "$1"
```

## Variante C: CI/CD

Den Token als CI-Secret speichern und als Umgebungsvariable an den Job
reichen; Secrets im Vault mit `--allow ci` freigeben.

## Namenskonventionen

- Agentennamen: Buchstaben, Ziffern, `-`, `_`, `.` (1-64 Zeichen)
- ACL-Prinzipal ist immer `agent:<name>`, im `--allow` reicht der nackte Name
- `--allow '*'` (Sternchen) erlaubt ALLE Agenten - sparsam einsetzen
