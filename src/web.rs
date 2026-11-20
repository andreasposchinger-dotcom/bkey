//! Local web UI and token API. Unlocked human sessions keep the full vault
//! session in memory (so the browser extension can save logins via /api/set);
//! agent sessions stay read-only by vault design (agent blocs are absent).
//! The UI answers on 127.0.0.1 (no CORS needed); external web pages can
//! authenticate with agent tokens when started with --origin.

use std::collections::HashMap;
use std::path::Path;

use tiny_http::{Header, Request, Response, Server};

use crate::vault::{self, Result};
use rand::RngCore;

struct WebSession {
    sess: vault::Session,
}

pub fn serve(args: &[String]) -> Result<()> {
    let path = crate::vault_path_arg(args)?;
    if !path.exists() {
        return Err(format!(
            "no vault at {} - run 'bkey init' first",
            path.display()
        )
        .into());
    }
    let port: u16 = crate::flag_value(args, "--port")
        .and_then(|v| v.parse().ok())
        .unwrap_or(8642);
    let bind = crate::flag_value(args, "--bind").unwrap_or("127.0.0.1");
    let mut origins: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--origin" {
            match it.next() {
                Some(v) if !v.starts_with("--") => origins.push(v.clone()),
                _ => return Err("--origin needs a URL (e.g. --origin https://app.example.com)".into()),
            }
        }
    }

    let addr = format!("{bind}:{port}");
    let server = Server::http(addr.as_str())
        .map_err(|e| format!("cannot bind {addr}: {e}"))?;
    let mut sessions: HashMap<String, WebSession> = HashMap::new();

    println!("bkey web UI:  http://{addr}/");
    println!("vault:         {}", path.display());
    if !origins.is_empty() {
        println!("CORS origins:  {}", origins.join(", "));
    }
    println!("Ctrl-C to stop");

    for req in server.incoming_requests() {
        handle(&mut sessions, &path, &origins, req);
    }
    Ok(())
}

fn new_sid() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    hex::encode(b)
}

fn hdr(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("static valid header")
}

fn cors_header(req: &Request, origins: &[String]) -> String {
    req.headers()
        .iter()
        .find(|h| h.field.equiv("Origin"))
        .map(|h| h.value.as_str().to_string())
        .filter(|o| {
            !o.is_empty()
                && (origins.iter().any(|a| a == o)
                    || o.starts_with("http://127.0.0.1")
                    || o.starts_with("http://localhost"))
        })
        .unwrap_or_default()
}

fn handle(
    sessions: &mut HashMap<String, WebSession>,
    path: &Path,
    origins: &[String],
    mut req: Request,
) {
    let method = req.method().as_str().to_string();
    let url_raw = req.url().to_string();
    let (url, _query) = url_raw.split_once('?').unwrap_or((url_raw.as_str(), ""));
    let mut body = String::new();
    if method == "POST" {
        let _ = req.as_reader().read_to_string(&mut body);
    }
    let cors = cors_header(&req, origins);

    match (method.as_str(), url) {
        ("OPTIONS", _) => {
            respond(req, 204, "text/plain", Vec::new(), &cors, true);
        }
        ("GET", "/") => {
            let b = include_str!("static/index.html").as_bytes().to_vec();
            respond_csp(req, "text/html; charset=utf-8", b, &cors);
        }
        ("GET", "/app.js") => {
            let b = include_str!("static/app.js").as_bytes().to_vec();
            respond_csp(req, "text/javascript; charset=utf-8", b, &cors);
        }
        ("GET", "/style.css") => {
            let b = include_str!("static/style.css").as_bytes().to_vec();
            respond_csp(req, "text/css; charset=utf-8", b, &cors);
        }
        ("POST", "/api/unlock") => {
            let pass = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["passphrase"].as_str().map(|s| s.to_string()));
            match pass {
                None => respond_json(req, 400, r#"{"error":"passphrase missing"}"#, &cors),
                Some(pass) => match vault::Session::open_human(path, &pass) {
                    Ok(s) => {
                        let sid = new_sid();
                        sessions.insert(sid.clone(), WebSession { sess: s });
                        let resp = serde_json::json!({ "sid": sid }).to_string();
                        respond_json(req, 200, &resp, &cors);
                    }
                    Err(_) => respond_json(req, 401, r#"{"error":"invalid passphrase or no vault"}"#, &cors),
                },
            }
        }
        ("POST", "/api/auth") => {
            let v: serde_json::Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(_) => {
                    respond_json(req, 400, r#"{"error":"bad json"}"#, &cors);
                    return;
                }
            };
            let agent = v["agent"].as_str().unwrap_or_default().to_string();
            let token = v["token"].as_str().unwrap_or_default().to_string();
            match vault::Session::open_agent(path, &agent, &token) {
                Ok(s) => {
                    let principal = s.principal.clone();
                    let sid = new_sid();
                    sessions.insert(sid.clone(), WebSession { sess: s });
                    let resp =
                        serde_json::json!({ "sid": sid, "principal": principal }).to_string();
                    respond_json(req, 200, &resp, &cors);
                }
                Err(e) => {
                    let resp = serde_json::json!({ "error": e.to_string() }).to_string();
                    respond_json(req, 401, &resp, &cors);
                }
            }
        }
        ("POST", "/api/lock") => {
            let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            match v["sid"].as_str() {
                Some(sid) => {
                    sessions.remove(sid);
                    respond_json(req, 200, "{}", &cors);
                }
                None => respond_json(req, 400, r#"{"error":"sid missing"}"#, &cors),
            }
        }
        ("GET", "/api/entries") => {
            let Some(sess) = auth_session(sessions, &req) else {
                respond_json(req, 401, r#"{"error":"not authenticated"}"#, &cors);
                return;
            };
            let s = &sess.sess;
            let items: Vec<serde_json::Value> = s
                .store
                .entries
                .iter()
                .filter(|(_, e)| s.is_human || e.allows(&s.principal))
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
            let resp = serde_json::json!({
                "principal": s.principal,
                "entries": items,
            })
            .to_string();
            respond_json(req, 200, &resp, &cors);
        }
        ("GET", u) if u.starts_with("/api/entry/") => {
            let Some(sess) = auth_session(sessions, &req) else {
                respond_json(req, 401, r#"{"error":"not authenticated"}"#, &cors);
                return;
            };
            let s = &sess.sess;
            let name = url_decode(&u["/api/entry/".len()..]);
            let Some(e) = s.store.entries.get(&name) else {
                respond_json(req, 404, r#"{"error":"not found or no access"}"#, &cors);
                return;
            };
            if !(s.is_human || e.allows(&s.principal)) {
                respond_json(req, 403, r#"{"error":"denied for this principal"}"#, &cors);
                return;
            }
            let resp =
                serde_json::json!({ "name": name, "secret": e.secret, "username": e.username })
                    .to_string();
            respond_json(req, 200, &resp, &cors);
        }
        // Save a login (or any secret) from the browser extension.
        // Only fully unlocked human sessions may write; agents cannot.
        ("POST", "/api/set") => {
            let v: serde_json::Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(_) => {
                    respond_json(req, 400, r#"{"error":"bad json"}"#, &cors);
                    return;
                }
            };
            let Some(sess) = auth_session_mut(sessions, &req) else {
                respond_json(req, 401, r#"{"error":"not authenticated"}"#, &cors);
                return;
            };
            if !sess.sess.is_human {
                respond_json(
                    req,
                    403,
                    r#"{"error":"saving requires an unlocked human session"}"#,
                    &cors,
                );
                return;
            }
            let name = v["name"].as_str().unwrap_or_default().trim().to_string();
            let secret = v["secret"].as_str().unwrap_or_default().to_string();
            if name.is_empty() || secret.is_empty() {
                respond_json(req, 400, r#"{"error":"name and secret are required"}"#, &cors);
                return;
            }
            if let Err(e) = vault::validate_name(&name) {
                let resp = serde_json::json!({ "error": e.to_string() }).to_string();
                respond_json(req, 400, &resp, &cors);
                return;
            }
            let allow: Vec<String> = v["allow"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            let acl = match crate::parse_allow(if allow.is_empty() {
                None
            } else {
                Some(allow.as_str())
            }) {
                Ok(a) => a,
                Err(_) => {
                    respond_json(req, 400, r#"{"error":"bad allow list"}"#, &cors);
                    return;
                }
            };
            let username = v["username"].as_str().unwrap_or_default().to_string();
            let url = v["url"].as_str().unwrap_or_default().to_lowercase();
            let note = v["note"].as_str().unwrap_or_default().to_string();
            let existed = sess.sess.store.entries.contains_key(&name);
            sess.sess.store.entries.insert(
                name.clone(),
                vault::Entry {
                    secret,
                    acl,
                    note,
                    username,
                    url,
                    created: vault::now(),
                },
            );
            match sess.sess.save() {
                Ok(()) => {
                    let resp = serde_json::json!({ "ok": true, "existed": existed }).to_string();
                    respond_json(req, 200, &resp, &cors);
                }
                Err(e) => {
                    let resp = serde_json::json!({ "error": e.to_string() }).to_string();
                    respond_json(req, 500, &resp, &cors);
                }
            }
        }
        _ => respond_json(req, 404, r#"{"error":"not found"}"#, &cors),
    }
}

fn auth_session<'a>(
    sessions: &'a HashMap<String, WebSession>,
    req: &Request,
) -> Option<&'a WebSession> {
    let sid = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())?;
    sessions.get(&sid)
}

fn auth_session_mut<'a>(
    sessions: &'a mut HashMap<String, WebSession>,
    req: &Request,
) -> Option<&'a mut WebSession> {
    let sid = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())?;
    sessions.get_mut(&sid)
}

fn respond(req: Request, status: u16, ctype: &str, body: Vec<u8>, cors: &str, preflight: bool) {
    let mut r = Response::from_data(body).with_status_code(status);
    r.add_header(hdr("Content-Type", ctype));
    r.add_header(hdr("Cache-Control", "no-store"));
    r.add_header(hdr("X-Content-Type-Options", "nosniff"));
    r.add_header(hdr("Referrer-Policy", "no-referrer"));
    if !cors.is_empty() {
        r.add_header(hdr("Access-Control-Allow-Origin", cors));
        r.add_header(hdr("Vary", "Origin"));
        if preflight {
            r.add_header(hdr("Access-Control-Allow-Methods", "GET, POST, OPTIONS"));
            r.add_header(hdr("Access-Control-Allow-Headers", "Authorization, Content-Type"));
        }
    }
    let _ = req.respond(r);
}

fn respond_json(req: Request, status: u16, body: &str, cors: &str) {
    respond(req, status, "application/json", body.as_bytes().to_vec(), cors, false);
}

fn respond_csp(req: Request, ctype: &str, body: Vec<u8>, cors: &str) {
    let mut r = Response::from_data(body).with_status_code(200);
    r.add_header(hdr("Content-Type", ctype));
    r.add_header(hdr("Cache-Control", "no-store"));
    r.add_header(hdr("X-Content-Type-Options", "nosniff"));
    r.add_header(hdr("Referrer-Policy", "no-referrer"));
    r.add_header(hdr(
        "Content-Security-Policy",
        "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'",
    ));
    if !cors.is_empty() {
        r.add_header(hdr("Access-Control-Allow-Origin", cors));
        r.add_header(hdr("Vary", "Origin"));
    }
    let _ = req.respond(r);
}

/// Minimal %-decoding for entry names in API paths.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hexstr = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz");
            if let Ok(b) = u8::from_str_radix(hexstr, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(test)]
mod tests {
    use super::url_decode;

    #[test]
    fn decodes_percent() {
        assert_eq!(url_decode("plain"), "plain");
        assert_eq!(url_decode("a%20b"), "a b");
        assert_eq!(url_decode("trailing%"), "trailing%");
        assert_eq!(url_decode("bad%2"), "bad%2");
        assert_eq!(url_decode("%41%42"), "AB");
    }
}
