# OpenCode Setup

OpenCode runs its shell commands in your user environment, so bkey agent
variables are picked up the same way as in a normal terminal.

1. Create the token on your machine:

```bash
bkey agent add opencode
# prints:
#   BKEY_AGENT=opencode
#   BKEY_TOKEN=bka_....
```

2. Make the variables available to the `opencode` process. Either export
   them in the shell/launcher you start OpenCode from:

```bash
export BKEY_AGENT=opencode
export BKEY_TOKEN=bka_....
opencode
```

or per project with direnv (`.envrc`), or in your `.opencode` project
config's command wrappers if you prefer scoped access.

3. Allow access to a secret:

```bash
bkey set project-api-key --allow opencode
```

4. Test it inside the OpenCode session:

```bash
bkey get project-api-key   # OK
bkey get other-key         # -> denied
```

Caveat: any process running with these environment variables inherits the
agent's rights - scope the token to one machine/user.
