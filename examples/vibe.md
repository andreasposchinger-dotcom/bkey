# Mistral Vibe Code CLI Setup

The Vibe Code CLI (`vibe`) inherits the environment of the shell it is
started from, and its shell commands see the same variables.

1. Create the token on your machine:

```bash
bkey agent add vibe
# prints:
#   BKEY_AGENT=vibe
#   BKEY_TOKEN=bka_....
```

2. Export the variables before launching the CLI:

```bash
export BKEY_AGENT=vibe
export BKEY_TOKEN=bka_....
vibe
```

   or per project with direnv (`.envrc`):

```bash
export BKEY_AGENT=vibe
export BKEY_TOKEN=bka_....
```

   `~/.vibe/.env` is documented for API keys and provider credentials; if
   you put `BKEY_AGENT`/`BKEY_TOKEN` there, verify they actually reach the
   CLI's shell commands - the shell export is the documented, guaranteed
   way.

3. Allow access to a secret:

```bash
bkey set project-api-key --allow vibe
```

4. Test it inside the Vibe Code session:

```bash
bkey get project-api-key   # OK
bkey get other-key         # -> denied
```

Caveat: any process running with these environment variables inherits the
agent's rights - scope the token to one machine/user.
