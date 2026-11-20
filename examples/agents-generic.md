# Generic agents (Vibe, bots, CI, scripts)

bkey identifies agents through two environment variables:

```bash
BKEY_AGENT=<name>
BKEY_TOKEN=<token>
```

How they end up in the agent process does not matter to the tool - in the
end they must be environment variables of the process that calls `bkey`.

## Variant A: shell profile / per project

```bash
# per project, e.g. in your project env file or direnv (.envrc):
export BKEY_AGENT=vibe
export BKEY_TOKEN=bka_....

bkey set project-api-key --allow vibe
bkey get project-api-key
```

## Variant B: wrapper script

```bash
#!/usr/bin/env bash
# /usr/local/bin/vault-get NAME
BKEY_AGENT=cloud-bot BKEY_TOKEN="$(cat ~/.config/cloud-bot/token)" \
  exec bkey get "$1"
```

## Variant C: CI/CD

Store the token as a CI secret and pass it to the job as an environment
variable; allow the vault secrets with `--allow ci`.

## Naming conventions

- Agent names: letters, digits, `-`, `_`, `.` (1-64 characters)
