# OpenAI Codex CLI Setup

Codex decides which environment variables reach the commands it runs via
`[shell_environment_policy]` in `~/.codex/config.toml`. The default
(`inherit = "all"`) passes your whole shell environment through, so the
simple setup works out of the box.

1. Create the token on your machine:

```bash
bkey agent add codex
# prints:
#   BKEY_AGENT=codex
#   BKEY_TOKEN=bka_....
```

2. **Simple way** - export before launching codex:

```bash
export BKEY_AGENT=codex
export BKEY_TOKEN=bka_....
codex
```

3. **Robust way (recommended)** - set the variables explicitly in
   `~/.codex/config.toml`. `set` survives a strict policy, and
   `BKEY_TOKEN` would otherwise be dropped by the automatic
   KEY/SECRET/TOKEN name filter when `ignore_default_excludes = false`:

```toml
[shell_environment_policy]
inherit = "all"                  # keep your existing policy
ignore_default_excludes = true   # keep; `set` below wins anyway
set = { BKEY_AGENT = "codex", BKEY_TOKEN = "bka_...." }
```

4. Allow access to a secret:

```bash
bkey set project-api-key --allow codex
```

5. Test it inside the Codex session:

```bash
bkey get project-api-key   # OK
bkey get other-key         # -> denied
```

Caveat: any command Codex runs can read the token from its environment.
Keep the policy restrictive (`core` + explicit `set`) if the project is
untrusted.
