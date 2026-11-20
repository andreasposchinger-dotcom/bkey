//! bkey vault: file format, crypto and session logic.
//!
//! Vault file layout (binary, little-endian):
//! ```text
//! "BKEY2"                       5 bytes magic (v2)
//! bloc_count                    1 byte
//! bloc_count x key-blocs:       each wraps the random 32-byte data key
//!   tag_len                     1 byte
//!   tag                         tag_len bytes ("master" or "agent:<name>")
//!   salt                        32 bytes (Argon2id, per bloc)
//!   nonce                       24 bytes (XChaCha20-Poly1305)
//!   wrapped_key                 48 bytes (32 + 16 AEAD tag)
//! store_nonce                   24 bytes
//! store_ct                      rest (XChaCha20-Poly1305 over the JSON store)
//! ```
//! The data key is wrapped once for the human (master passphrase) and once
//! per agent token. Agents therefore never need the master passphrase.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const MAGIC: &[u8; 5] = b"BKEY2";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SALT_LEN: usize = 32;
const TAG_LEN: u8 = 255;
pub const MASTER_TAG: &str = "master";

// ---------------------------------------------------------------- errors

#[derive(Debug)]
pub struct BkeyError(pub String);

impl fmt::Display for BkeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BkeyError {}

impl From<String> for BkeyError {
    fn from(s: String) -> Self {
        BkeyError(s)
    }
}

pub fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(BkeyError(msg.into()).into())
}

// ---------------------------------------------------------------- store

/// The decrypted content of the vault.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Store {
    #[serde(default)]
    pub entries: BTreeMap<String, Entry>,
}

/// One secret. `acl` lists additional principals besides the human owner:
/// `agent:<name>` entries, or `*` for every agent.
/// For website logins, `username` and `url` hold the login user and the
/// site hostname (e.g. "github.com") so the browser extension can autofill.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Entry {
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub acl: Vec<String>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub created: u64,
}

impl Entry {
    /// May `principal` (e.g. "agent:claude-code") read this entry?
    /// The human owner is always allowed and never checks this.
    pub fn allows(&self, principal: &str) -> bool {
        self.acl.iter().any(|a| a == principal || a == "*")
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn agent_principal(name: &str) -> String {
    format!("agent:{name}")
}

pub fn agent_tag(name: &str) -> String {
    format!("agent:{name}")
}

/// Validate an agent or entry name (ASCII, no separators, no tag collisions).
pub fn validate_name(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 64 {
        return err("name must be 1..64 characters");
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return err("only letters, digits, '-', '_' and '.' allowed in names");
    }
    if s.eq_ignore_ascii_case(MASTER_TAG) {
        return err("'master' is reserved");
    }
    Ok(())
}

pub fn generate_token() -> String {
    let mut b = [0u8; 24];
    OsRng.fill_bytes(&mut b);
    format!("bka_{}", hex::encode(b))
}

pub fn default_vault_path() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("BKEY_VAULT") {
        if !p.is_empty() {
            return Ok(PathBuf::from(p));
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "neither HOME nor USERPROFILE is set")?;
    #[cfg(windows)]
    let p = PathBuf::from(&home).join("AppData").join("Roaming").join("bkey").join("vault.bkey");
    #[cfg(not(windows))]
    let p = PathBuf::from(&home).join(".config").join("bkey").join("vault.bkey");
    Ok(p)
}

// ---------------------------------------------------------------- crypto

fn kdf(secret: &str, salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let params = Params::new(65536, 3, 4, Some(KEY_LEN))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    argon.hash_password_into(secret.as_bytes(), salt, out.as_mut())?;
    Ok(out)
}

fn cipher(key: &[u8]) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(Key::from_slice(key))
}

/// Wrap `dk` for `secret`, producing one full bloc (with leading tag_len byte).
fn wrap_key(dk: &[u8], secret: &str, tag: &str) -> Result<Vec<u8>> {
    if tag.len() > TAG_LEN as usize {
        return err("tag too long");
    }
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let key = kdf(secret, &salt)?;
    let ct = cipher(key.as_ref())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: dk,
                aad: format!("BKEY2:wrap:{tag}").as_bytes(),
            },
        )
        .map_err(|_| "key wrap failed")?;
    let mut raw = vec![tag.len() as u8];
    raw.extend_from_slice(tag.as_bytes());
    raw.extend_from_slice(&salt);
    raw.extend_from_slice(&nonce);
    raw.extend_from_slice(&ct);
    Ok(raw)
}

/// Unwrap the data key from one bloc; fails if the secret does not match.
fn unwrap_key(bloc: &Bloc, secret: &str) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let raw = &bloc.raw;
    if raw.len() < 1 + bloc.tag.len() + SALT_LEN + NONCE_LEN + KEY_LEN + 16 {
        return err("corrupted bloc");
    }
    let mut off = 1 + bloc.tag.len();
    let salt = &raw[off..off + SALT_LEN];
    off += SALT_LEN;
    let nonce = &raw[off..off + NONCE_LEN];
    off += NONCE_LEN;
    let ct = &raw[off..];
    let key = kdf(secret, salt)?;
    let mut dk = cipher(key.as_ref())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: format!("BKEY2:wrap:{}", bloc.tag).as_bytes(),
            },
        )
        .map_err(|_| "invalid passphrase or token".to_string())?;
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    out.copy_from_slice(&dk);
    dk.zeroize();
    Ok(out)
}

fn encrypt_store(store: &Store, dk: &[u8]) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(store)?;
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    let ct = cipher(dk)
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &json,
                aad: b"BKEY2:store",
            },
        )
        .map_err(|_| "store encryption failed")?;
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ct);
    Ok(out)
}

fn decrypt_store(dk: &[u8], data: &[u8]) -> Result<Store> {
    if data.len() < NONCE_LEN + 16 {
        return err("store payload too short");
    }
    let json = cipher(dk)
        .decrypt(
            XNonce::from_slice(&data[..NONCE_LEN]),
            Payload {
                msg: &data[NONCE_LEN..],
                aad: b"BKEY2:store",
            },
        )
        .map_err(|_| "store decryption failed")?;
    Ok(serde_json::from_slice(&json)?)
}

// ---------------------------------------------------------------- file

/// One key-bloc of the vault file, kept as raw bytes so it can be rewritten
/// byte-identically when the human saves changes.
#[derive(Clone)]
pub struct Bloc {
    pub tag: String,
    pub raw: Vec<u8>,
}

pub struct VaultFile {
    pub blocs: Vec<Bloc>,
    pub store: Vec<u8>,
}

pub fn parse(data: &[u8]) -> Result<VaultFile> {
    if data.len() < 6 + NONCE_LEN + 16 || &data[..5] != MAGIC.as_slice() {
        return err("not a bkey vault (bad magic)");
    }
    let nblocs = data[5] as usize;
    let mut off = 6;
    let mut blocs = Vec::with_capacity(nblocs);
    for _ in 0..nblocs {
        if off >= data.len() {
            return err("corrupted vault: bloc header truncated");
        }
        let tlen = data[off] as usize;
        if tlen == 0 || off + 1 + tlen + SALT_LEN + NONCE_LEN + KEY_LEN + 16 > data.len() {
            return err("corrupted vault: bloc truncated");
        }
        let tag = String::from_utf8_lossy(&data[off + 1..off + 1 + tlen]).to_string();
        let end = off + 1 + tlen + SALT_LEN + NONCE_LEN + KEY_LEN + 16;
        blocs.push(Bloc {
            tag,
            raw: data[off..end].to_vec(),
        });
        off = end;
    }
    if data.len() < off + NONCE_LEN + 16 {
        return err("corrupted vault: store truncated");
    }
    Ok(VaultFile {
        blocs,
        store: data[off..].to_vec(),
    })
}

pub fn assemble(blocs: &[Bloc], store_ct: &[u8]) -> Result<Vec<u8>> {
    if blocs.len() > 255 {
        return err("too many blocs (max 255)");
    }
    let mut out = MAGIC.to_vec();
    out.push(blocs.len() as u8);
    for b in blocs {
        out.extend_from_slice(&b.raw);
    }
    out.extend_from_slice(store_ct);
    Ok(out)
}

fn read_vault(path: &Path) -> Result<VaultFile> {
    let data = fs::read(path)
        .map_err(|e| format!("cannot read vault {}: {e}", path.display()))?;
    parse(&data)
}

/// List agent names without unlocking (bloc tags are visible metadata).
pub fn list_agents(path: &Path) -> Result<Vec<String>> {
    let v = read_vault(path)?;
    Ok(v.blocs
        .iter()
        .filter_map(|b| b.tag.strip_prefix("agent:").map(|s| s.to_string()))
        .collect())
}

// ---------------------------------------------------------------- session

/// An unlocked vault. `open_human` requires the master passphrase, agents
/// open the vault with their token alone (wrapped key, no passphrase needed).
pub struct Session {
    pub path: PathBuf,
    pub store: Store,
    pub dk: Zeroizing<[u8; KEY_LEN]>,
    pub blocs: Vec<Bloc>,
    pub principal: String,
    pub is_human: bool,
}

impl Session {
    /// Create a new vault with a fresh random data key.
    pub fn init(path: &Path, pass: &str) -> Result<Session> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut dk = Zeroizing::new([0u8; KEY_LEN]);
        OsRng.fill_bytes(dk.as_mut());
        let master = wrap_key(dk.as_ref(), pass, MASTER_TAG)?;
        let store_ct = encrypt_store(&Store::default(), dk.as_ref())?;
        let blocs = vec![Bloc {
            tag: MASTER_TAG.to_string(),
            raw: master,
        }];
        fs::write(path, assemble(&blocs, &store_ct)?)?;
        Ok(Session {
            path: path.to_path_buf(),
            store: Store::default(),
            dk,
            blocs,
            principal: "human".to_string(),
            is_human: true,
        })
    }

    pub fn open_human(path: &Path, pass: &str) -> Result<Session> {
        let v = read_vault(path)?;
        let master = v
            .blocs
            .iter()
            .find(|b| b.tag == MASTER_TAG)
            .ok_or_else(|| BkeyError("vault has no master bloc".into()))?;
        let dk = unwrap_key(master, pass)?;
        let store = decrypt_store(dk.as_ref(), &v.store)?;
        Ok(Session {
            path: path.to_path_buf(),
            store,
            dk,
            blocs: v.blocs,
            principal: "human".to_string(),
            is_human: true,
        })
    }

    pub fn open_agent(path: &Path, name: &str, token: &str) -> Result<Session> {
        validate_name(name)?;
        let v = read_vault(path)?;
        let tag = agent_tag(name);
        let bloc = v
            .blocs
            .iter()
            .find(|b| b.tag == tag)
            .ok_or_else(|| BkeyError(format!("unknown agent '{name}'")))?;
        let dk = unwrap_key(bloc, token)?;
        let store = decrypt_store(dk.as_ref(), &v.store)?;
        Ok(Session {
            path: path.to_path_buf(),
            store,
            dk,
            blocs: Vec::new(),
            principal: agent_principal(name),
            is_human: false,
        })
    }

    /// Persist the (possibly modified) store. Human-only; agent blocs are
    /// preserved byte-identically, the master bloc only changes on passwd.
    pub fn save(&self) -> Result<()> {
        if !self.is_human {
            return err("sessions opened with an agent token are read-only");
        }
        let store_ct = encrypt_store(&self.store, self.dk.as_ref())?;
        let data = assemble(&self.blocs, &store_ct)?;
        write_atomic(&self.path, &data)
    }

    /// Human-only: create an agent and print/wrap its token.
    pub fn add_agent(&mut self, name: &str, token: &str) -> Result<()> {
        if !self.is_human {
            return err("only the human owner can manage agents");
        }
        validate_name(name)?;
        let tag = agent_tag(name);
        if self.blocs.iter().any(|b| b.tag == tag) {
            return err(format!("agent '{name}' already exists"));
        }
        self.blocs.push(Bloc {
            tag: tag.clone(),
            raw: wrap_key(self.dk.as_ref(), token, &tag)?,
        });
        Ok(())
    }

    pub fn remove_agent(&mut self, name: &str) -> Result<()> {
        if !self.is_human {
            return err("only the human owner can manage agents");
        }
        let tag = agent_tag(name);
        let before = self.blocs.len();
        self.blocs.retain(|b| b.tag != tag);
        if self.blocs.len() == before {
            return err(format!("agent '{name}' not found"));
        }
        Ok(())
    }

    /// Human-only: change the master passphrase; agent tokens stay valid.
    pub fn set_master_pass(&mut self, new_pass: &str) -> Result<()> {
        if !self.is_human {
            return err("only the human owner can change the passphrase");
        }
        let raw = wrap_key(self.dk.as_ref(), new_pass, MASTER_TAG)?;
        match self.blocs.iter_mut().find(|b| b.tag == MASTER_TAG) {
            Some(b) => b.raw = raw,
            None => self.blocs.push(Bloc {
                tag: MASTER_TAG.to_string(),
                raw,
            }),
        }
        Ok(())
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    // best-effort sync: keep simple, rename on same filesystem is atomic
    fs::rename(&tmp, path)?;
    Ok(())
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_vault(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("bkey-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn init_unlock_and_roundtrip() {
        let p = tmp_vault("roundtrip.bkey");
        {
            let mut s = Session::init(&p, "correct horse battery").unwrap();
            s.store.entries.insert(
                "api-key".into(),
                Entry {
                    secret: "s3cret".into(),
                    acl: vec![agent_principal("claude-code")],
                    note: "test".into(),
                    created: now(),
                    ..Entry::default()
                },
            );
            s.save().unwrap();
        }
        let s = Session::open_human(&p, "correct horse battery").unwrap();
        assert_eq!(s.store.entries["api-key"].secret, "s3cret");
        assert!(!Session::open_human(&p, "wrong").is_ok());
    }

    #[test]
    fn agent_token_grants_only_acl() {
        let p = tmp_vault("agent.bkey");
        let mut s = Session::init(&p, "master-pass-123").unwrap();
        s.store.entries.insert(
            "claude-key".into(),
            Entry {
                secret: "for-claude".into(),
                acl: vec![agent_principal("claude-code")],
                note: String::new(),
                created: 0,
                ..Entry::default()
            },
        );
        s.store.entries.insert(
            "vibe-key".into(),
            Entry {
                secret: "for-vibe".into(),
                acl: vec![agent_principal("vibe")],
                note: String::new(),
                created: 0,
                ..Entry::default()
            },
        );
        let token = generate_token();
        s.add_agent("claude-code", &token).unwrap();
        s.save().unwrap();

        let a = Session::open_agent(&p, "claude-code", &token).unwrap();
        assert_eq!(a.principal, "agent:claude-code");
        assert!(a.store.entries["claude-key"].allows(&a.principal));
        assert!(!a.store.entries["vibe-key"].allows(&a.principal));
        assert!(!Session::open_agent(&p, "claude-code", "bka_dead").is_ok());
        assert!(!Session::open_agent(&p, "vibe", &token).is_ok());
    }

    #[test]
    fn passwd_keeps_agent_tokens() {
        let p = tmp_vault("passwd.bkey");
        let mut s = Session::init(&p, "old-pass-123").unwrap();
        let token = generate_token();
        s.add_agent("bot", &token).unwrap();
        s.save().unwrap();
        let mut s = Session::open_human(&p, "old-pass-123").unwrap();
        s.set_master_pass("new-pass-456").unwrap();
        s.save().unwrap();
        assert!(!Session::open_human(&p, "old-pass-123").is_ok());
        assert!(Session::open_human(&p, "new-pass-456").is_ok());
        assert!(Session::open_agent(&p, "bot", &token).is_ok());
    }

    #[test]
    fn agent_list_metadata() {
        let p = tmp_vault("list.bkey");
        let mut s = Session::init(&p, "list-pass-123").unwrap();
        s.add_agent("a1", &generate_token()).unwrap();
        s.add_agent("a2", &generate_token()).unwrap();
        s.save().unwrap();
        let names = list_agents(&p).unwrap();
        assert_eq!(names, vec!["a1".to_string(), "a2".to_string()]);
    }

    /// Vaults written before the username/url fields existed must still load.
    #[test]
    fn legacy_entry_deserializes() {
        let e: Entry = serde_json::from_str(
            r#"{"secret":"x","acl":["agent:old"],"note":"n","created":1}"#,
        )
        .unwrap();
        assert_eq!(e.username, "");
        assert_eq!(e.url, "");
        assert_eq!(e.secret, "x");
        let s: Store =
            serde_json::from_str(r#"{"entries":{"a":{"secret":"y"}}}"#).unwrap();
        assert_eq!(s.entries["a"].secret, "y");
        assert!(s.entries["a"].acl.is_empty());
    }
}
