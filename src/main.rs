//! bkey - tiny secrets vault for humans, AI agents and the browser.

mod vault;
mod web;

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};

use rand::RngCore;
use zeroize::Zeroizing;

use vault::{Entry, Result, Session};

const USAGE: &str = r#"bkey - tiny secrets vault for humans, AI agents and the browser

USAGE:
  bkey init [--vault PATH] [--force]
      Create a new vault (asks for a master passphrase).

  bkey set NAME [--allow LIST] [--note TEXT] [--generate [N] | --secret-file F]
                [--username USER] [--url HOST]
      Store a secret. --allow is a comma list of principals:
        claude-code,vibe          ->  agent:claude-code, agent:vibe
        *                         ->  every agent may read it
      No --allow means: only the human owner (master passphrase) can read it.
      Secret is read hidden from the terminal, piped via stdin,
      from a file (--secret-file) or generated (--generate, default 32 bytes hex).
      For website logins set --username and --url (e.g. --url github.com);
      the browser extension then autofills them on that site.

  bkey get NAME [--out plain|json|env]
      Read a secret. Caller is detected automatically:
        BKEY_AGENT + BKEY_TOKEN set  ->  agent access (ACL enforced)
        otherwise                    ->  human (master passphrase, full access)

  bkey list [--out json]
      List entries (agents only see their own).

  bkey rm NAME
      Delete an entry (human only).

  bkey agent add NAME          Print a new token for an agent (human only).
  bkey agent rm NAME           Revoke an agent token (human only).
  bkey agent ls                Show configured agent names.

  bkey passwd                  Change the master passphrase (agent tokens stay valid).

  bkey serve [--port 8642] [--bind 127.0.0.1] [--origin URL]...
      Local web UI (browser, no plugin needed). Optional --origin adds a
      CORS allowlist so external web pages can authenticate via /api/auth
      with an agent token.

ENVIRONMENT:
  BKEY_AGENT / BKEY_TOKEN      agent identity injected by the agent's config
  BKEY_VAULT                   vault path override (also: --vault)

VAULT FILE: ~/.config/bkey/vault.bkey (XChaCha20-Poly1305 + Argon2id)
"#;

fn main() {
    if let Err(e) = run() {
        eprintln!("bkey: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print!("{USAGE}");
        return Ok(());
    }
    let rest = &args[1..];
    match args[0].as_str() {
        "init" => cmd_init(rest),
        "set" | "add" => cmd_set(rest),
        "get" => cmd_get(rest),
        "list" | "ls" => cmd_list(rest),
        "rm" => cmd_rm(rest),
        "passwd" => cmd_passwd(rest),
        "agent" => cmd_agent(rest),
        "serve" => web::serve(rest),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command '{other}' - try 'bkey help'").into()),
    }
}

// ---------------------------------------------------------- arg helpers

fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .filter(|s| !s.starts_with("--"))
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn positionals(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            if flag_value(args, &args[i]).is_some() {
                i += 1;
            }
        } else {
            out.push(args[i].as_str());
        }
        i += 1;
    }
    out
}

fn vault_path_arg(args: &[String]) -> Result<PathBuf> {
    match flag_value(args, "--vault") {
        Some(p) => Ok(PathBuf::from(p)),
        None => vault::default_vault_path(),
    }
}

fn need_name(args: &[String], what: &str) -> Result<String> {
    positionals(args)
        .first()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("missing {what} - see 'bkey help'").into())
}

// ---------------------------------------------------------- input helpers

fn read_stdin_line() -> Result<String> {
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim_end_matches(['\r', '\n']).to_string())
}

fn ask_pass(prompt: &str) -> Result<Zeroizing<String>> {
    if std::io::stdin().is_terminal() {
        Ok(Zeroizing::new(rpassword::prompt_password(prompt)?))
    } else {
        Ok(Zeroizing::new(read_stdin_line()?))
    }
}

fn ask_new_pass() -> Result<Zeroizing<String>> {
    if !std::io::stdin().is_terminal() {
        return ask_pass("new master passphrase (stdin): ");
    }
    loop {
        let p = ask_pass("new master passphrase: ")?;
        if p.len() < 8 {
            println!("-> weak passphrase - should be at least 8 characters");
            continue;
        }
        let p2 = ask_pass("repeat passphrase: ")?;
        if *p == *p2 {
            return Ok(p);
        }
        println!("-> passphrases do not match, try again");
    }
}

fn read_secret(args: &[String]) -> Result<Zeroizing<String>> {
    if let Some(f) = flag_value(args, "--secret-file") {
        let s = std::fs::read_to_string(f)?;
        return Ok(Zeroizing::new(s.trim_end_matches(['\r', '\n']).to_string()));
    }
    if has_flag(args, "--generate") {
        let n: usize = flag_value(args, "--generate")
            .and_then(|v| v.parse().ok())
            .unwrap_or(32);
        let n = n.clamp(8, 128);
        let mut b = vec![0u8; n];
        rand::rngs::OsRng.fill_bytes(&mut b);
        println!("bkey: generated random secret (--generate)");
        return Ok(Zeroizing::new(format!("bkg_{}", hex::encode(b))));
    }
    if std::io::stdin().is_terminal() {
        Ok(Zeroizing::new(rpassword::prompt_password("secret value: ")?))
    } else {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(Zeroizing::new(s.trim_end_matches(['\r', '\n']).to_string()))
    }
}

/// Split --allow list into principals: bare names become agent:<name>.
fn parse_allow(raw: Option<&str>) -> Result<Vec<String>> {
    let mut acl = Vec::new();
    for part in raw.unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let p = match part {
            "*" => "*".to_string(),
            "human" => continue, // implicit: the owner can always read
            p if p.starts_with("agent:") => p.to_string(),
            p => vault::agent_principal(p),
        };
        if !acl.contains(&p) {
            acl.push(p);
        }
    }
    Ok(acl)
}

fn caller_session(path: &Path) -> Result<Session> {
    let agent = std::env::var("BKEY_AGENT").unwrap_or_default();
    let token = std::env::var("BKEY_TOKEN").unwrap_or_default();
    if !agent.is_empty() || !token.is_empty() {
        if agent.is_empty() || token.is_empty() {
            return Err("BKEY_AGENT and BKEY_TOKEN must be set together".into());
        }
        return Session::open_agent(path, &agent, &token);
    }
    let pass = ask_pass("master passphrase: ")?;
    Session::open_human(path, &pass)
}

// ---------------------------------------------------------- commands

fn cmd_init(args: &[String]) -> Result<()> {
    let path = vault_path_arg(args)?;
    if path.exists() && !has_flag(args, "--force") {
        return Err(format!(
            "vault already exists at {} (use --force to overwrite)",
            path.display()
        )
        .into());
    }
    let pass = ask_new_pass()?;
    Session::init(&path, &pass)?;
    println!("bkey: vault created at {}", path.display());
    println!("next steps:");
    println!("  bkey agent add claude-code   # token per agent");
    println!("  bkey set my-api-key --allow claude-code");
    Ok(())
}

fn cmd_set(args: &[String]) -> Result<()> {
    let name = need_name(args, "NAME")?;
    vault::validate_name(&name)?;
    let secret = read_secret(args)?;
    let acl = parse_allow(flag_value(args, "--allow"))?;
    let note = flag_value(args, "--note").unwrap_or("").to_string();
    let username = flag_value(args, "--username").unwrap_or("").to_string();
    let url = flag_value(args, "--url").unwrap_or("").to_lowercase();
    let path = vault_path_arg(args)?;
    let pass = ask_pass("master passphrase: ")?;
    let mut sess = Session::open_human(&path, &pass)?;
    sess.store.entries.insert(
        name.clone(),
        Entry {
            secret: secret.to_string(),
            acl: acl.clone(),
            note,
            username,
            url,
            created: vault::now(),
        },
    );
    sess.save()?;
    let who = if acl.is_empty() {
        "human only".to_string()
    } else {
        acl.join(", ")
    };
    println!("bkey: stored '{name}' (readable by: {who})");
    Ok(())
}

fn cmd_get(args: &[String]) -> Result<()> {
    let name = need_name(args, "NAME")?;
    let out = flag_value(args, "--out").unwrap_or("plain");
    let path = vault_path_arg(args)?;
    let sess = caller_session(&path)?;
    let allowed = match sess.store.entries.get(&name) {
        Some(e) => sess.is_human || e.allows(&sess.principal),
        None => false,
    };
    if !allowed {
        return Err(format!(
            "denied: '{}' may not read '{name}' (or it does not exist)",
            sess.principal
        )
        .into());
    }
    let e = sess.store.entries.get(&name).unwrap();
    match out {
        "plain" => println!("{}", e.secret),
        "env" => println!("export {}='{}'", name.to_uppercase().replace('-', "_"), e.secret),
        "json" => println!(
            "{}",
            serde_json::json!({
                "name": name,
                "secret": e.secret,
                "username": e.username,
                "url": e.url,
            })
        ),
        other => return Err(format!("unknown --out '{other}'").into()),
    }
    Ok(())
}

fn cmd_list(args: &[String]) -> Result<()> {
    let path = vault_path_arg(args)?;
    let sess = caller_session(&path)?;
    let json_out = flag_value(args, "--out") == Some("json");
    let visible: Vec<(&String, &vault::Entry)> = sess
        .store
        .entries
        .iter()
        .filter(|(_, e)| sess.is_human || e.allows(&sess.principal))
        .collect();

    if json_out {
        let items: Vec<serde_json::Value> = visible
            .iter()
            .map(|(name, e)| {
                serde_json::json!({
                    "name": name,
                    "acl": e.acl,
                    "note": e.note,
                    "username": e.username,
                    "url": e.url,
                    "created": e.created,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "principal": sess.principal, "entries": items })
        );
        return Ok(());
    }

    if visible.is_empty() {
        println!("bkey: no entries visible for {}", sess.principal);
        return Ok(());
    }
    let w = visible.iter().map(|(k, _)| k.len()).max().unwrap_or(4).max(4);
    println!("{:<w$}  {:<16}  {}", "NAME", "READABLE BY", "NOTE");
    for (name, e) in visible {
        let who = if e.acl.is_empty() {
            "(human only)".to_string()
        } else {
            e.acl.join(",")
        };
        // detail column: login user@site first, then the free-text note
        let mut detail = String::new();
        if !e.username.is_empty() || !e.url.is_empty() {
            detail.push_str(&format!("{}@{} ", e.username, e.url));
        }
        detail.push_str(&e.note);
        println!("{:<w$}  {:<16}  {}", name, who, detail.trim_end());
    }
    Ok(())
}

fn cmd_rm(args: &[String]) -> Result<()> {
    let name = need_name(args, "NAME")?;
    let path = vault_path_arg(args)?;
    let pass = ask_pass("master passphrase: ")?;
    let mut sess = Session::open_human(&path, &pass)?;
    if sess.store.entries.remove(&name).is_none() {
        return Err(format!("'{name}' not found").into());
    }
    sess.save()?;
    println!("bkey: removed '{name}'");
    Ok(())
}

fn cmd_passwd(args: &[String]) -> Result<()> {
    let path = vault_path_arg(args)?;
    let pass = ask_pass("current master passphrase: ")?;
    let mut sess = Session::open_human(&path, &pass)?;
    let new = ask_new_pass()?;
    sess.set_master_pass(&new)?;
    sess.save()?;
    println!("bkey: master passphrase changed (agent tokens remain valid)");
    Ok(())
}

fn cmd_agent(args: &[String]) -> Result<()> {
    let sub = positionals(args).first().map(|s| s.to_string());
    match sub.as_deref() {
        Some("ls" | "list") => {
            let path = vault_path_arg(args)?;
            let names = vault::list_agents(&path)?;
            if names.is_empty() {
                println!("bkey: no agents configured");
            }
            for n in names {
                println!("{n}");
            }
            Ok(())
        }
        Some("add" | "new") => {
            let name = positionals(args).get(1).map(|s| s.to_string());
            let Some(name) = name else {
                return Err("usage: bkey agent add NAME".into());
            };
            vault::validate_name(&name)?;
            let path = vault_path_arg(args)?;
            let pass = ask_pass("master passphrase: ")?;
            let mut sess = Session::open_human(&path, &pass)?;
            let token = vault::generate_token();
            sess.add_agent(&name, &token)?;
            sess.save()?;
            println!("bkey: agent '{name}' created. Set these env vars for the agent:");
            println!();
            println!("  BKEY_AGENT={name}");
            println!("  BKEY_TOKEN={token}");
            println!();
            println!("then allow access, e.g.:  bkey set my-api-key --allow {name}");
            println!("agent config examples: examples/ in the bkey repo");
            Ok(())
        }
        Some("rm" | "remove" | "del") => {
            let name = positionals(args).get(1).map(|s| s.to_string());
            let Some(name) = name else {
                return Err("usage: bkey agent rm NAME".into());
            };
            let path = vault_path_arg(args)?;
            let pass = ask_pass("master passphrase: ")?;
            let mut sess = Session::open_human(&path, &pass)?;
            sess.remove_agent(&name)?;
            sess.save()?;
            println!("bkey: agent '{name}' revoked (its token is now useless)");
            Ok(())
        }
        _ => Err("usage: bkey agent add|rm|ls".into()),
    }
}
