# Claude Code Setup

1. Create the token on your machine:

```bash
bkey agent add claude-code
```

2. Put the token into the Claude Code settings (`~/.claude/settings.json`):

```json
{
  "env": {
    "BKEY_AGENT": "claude-code",
    "BKEY_TOKEN": "bka_....your-token...."
  }
}
```

3. Allow access to a secret:

```bash
bkey set anthropic-key --allow claude-code
```

4. Test it inside a Claude Code session:

```bash
bkey get anthropic-key           # print the secret
bkey get anthropic-key --out env # print an export line, e.g. to source
bkey get other-key               # -> denied
```

Caveat: any process running with these environment variables inherits the
agent's rights. Restrict the file permissions of `~/.claude/settings.json`
(e.g. `chmod 600`).
