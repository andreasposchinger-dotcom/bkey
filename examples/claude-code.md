# Claude Code Setup

1. Token auf deinem Rechner erstellen:

```bash
bkey agent add claude-code
```

2. Token in die Claude-Code-Settings (`~/.claude/settings.json`) eintragen:

```json
{
  "env": {
    "BKEY_AGENT": "claude-code",
    "BKEY_TOKEN": "bka_....hier-dein-token...."
  }
}
```

3. Secret freigeben:

```bash
bkey set anthropic-key --allow claude-code
```

4. In der Claude-Code-Session testen:

```bash
bkey get anthropic-key          # Secret ausgeben
bkey get anthropic-key --out env # export-Zeile ausgeben, z.B. zum Sourcen
bkey get other-key               # -> denied
```

Einschränkung: Jeder Prozess, der mit diesen Umgebungsvariablen läuft, hat
die Rechte des Agents. Dateirechte von `~/.claude/settings.json` einschränken
(z.B. `chmod 600`).
