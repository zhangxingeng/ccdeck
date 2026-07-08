//! Named provider profiles (issue #21): let a user resume/fork a Claude Code
//! session against an alternate Anthropic-compatible provider (e.g. DeepSeek)
//! by injecting `ANTHROPIC_*` env vars around the existing launch command.
//!
//! Storage split (deliberate — keys are NEVER in the profiles list file):
//!   * `~/.claude/.ccstudio-providers.json` — the profile *metadata* list:
//!     `{ name, baseUrl, defaultModel?, keyBackend }`. No secrets.
//!   * The API key itself lives in the OS keychain (service `ccdeck-provider`,
//!     account = profile name) via the `keyring` crate — primary path.
//!   * Fallback ONLY when the keychain is unavailable AND the user explicitly
//!     opts in: `~/.claude/.ccstudio-providers-plaintext.json`, a
//!     `{ name -> key }` map, world-readable, written only on explicit consent.
//!
//! Every keyring / filesystem call is wrapped in a small function so the pure
//! logic — env-pair building ([`build_provider_env`]) and profile
//! (de)serialization — is unit-tested without touching a real keychain (which
//! CI can't provide).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Keychain service name under which every provider key is stored; the account
/// (username) is the profile name. Locked by the design.
const KEYCHAIN_SERVICE: &str = "ccdeck-provider";

/// Where the API key for a given profile currently lives. Persisted per-profile
/// so the UI badge is honest (🔒 keychain vs ⚠ plaintext).
///
/// Serialized as the bare strings `"keychain"` / `"plaintext"` (and `"none"`
/// for a profile that has no key yet) to match the frontend union type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyBackend {
    /// No key has been stored for this profile yet.
    None,
    /// Key is in the OS keychain (the good path).
    Keychain,
    /// Key is in the explicit-opt-in plaintext fallback file.
    Plaintext,
}

impl Default for KeyBackend {
    fn default() -> Self {
        KeyBackend::None
    }
}

/// A named provider profile. The API key is intentionally absent — it lives in
/// the keychain (or the plaintext fallback), never in this struct nor in the
/// profiles JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    /// User-visible name; also the keychain account key. Immutable once created
    /// (see module note on rename) — edits change base_url/model/key only.
    pub name: String,
    /// Anthropic-compatible base URL, e.g. `https://api.deepseek.com/anthropic`.
    pub base_url: String,
    /// Optional default model to export as `ANTHROPIC_MODEL` (e.g.
    /// `deepseek-chat`). `None`/empty ⇒ no `ANTHROPIC_MODEL` export.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Which store currently holds this profile's key. Authoritative — set by
    /// [`set_provider_key`], never trusted from the client on save.
    #[serde(default)]
    pub key_backend: KeyBackend,
}

// ---------------------------------------------------------------------------
// On-disk paths
// ---------------------------------------------------------------------------

/// `~/.claude/.ccstudio-providers.json` — the profile metadata list (NO keys).
fn profiles_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    Ok(home.join(".claude").join(".ccstudio-providers.json"))
}

/// `~/.claude/.ccstudio-providers-plaintext.json` — the explicit-opt-in
/// plaintext key fallback (`{ name -> key }`). Separate file so a key can never
/// leak into the profiles list by accident.
fn plaintext_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    Ok(home
        .join(".claude")
        .join(".ccstudio-providers-plaintext.json"))
}

// ---------------------------------------------------------------------------
// Profiles list (pure-ish: just file IO around serde)
// ---------------------------------------------------------------------------

/// Load the profiles list, falling back to empty on any error (missing file,
/// bad JSON) — the app must always launch.
pub fn load_profiles() -> Vec<ProviderProfile> {
    profiles_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist the profiles list (pretty-printed), creating `~/.claude/` if needed.
fn save_profiles(profiles: &[ProviderProfile]) -> Result<(), String> {
    let path = profiles_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut pretty = serde_json::to_string_pretty(profiles).map_err(|e| e.to_string())?;
    pretty.push('\n');
    std::fs::write(&path, pretty).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Plaintext fallback store (`{ name -> key }`)
// ---------------------------------------------------------------------------

fn plaintext_load() -> HashMap<String, String> {
    plaintext_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn plaintext_save(map: &HashMap<String, String>) -> Result<(), String> {
    let path = plaintext_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut pretty = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    pretty.push('\n');
    std::fs::write(&path, pretty).map_err(|e| e.to_string())?;
    // Harden the world-readable fallback to owner-only (0600) on Unix — the
    // keys are plaintext, so at least keep other local users out. Best-effort;
    // Windows keeps its default ACLs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn plaintext_set(name: &str, key: &str) -> Result<(), String> {
    let mut map = plaintext_load();
    map.insert(name.to_string(), key.to_string());
    plaintext_save(&map)
}

fn plaintext_get(name: &str) -> Option<String> {
    plaintext_load().get(name).cloned()
}

/// Remove a name from the plaintext file (no-op if absent). If the file becomes
/// empty it's rewritten as `{}` rather than deleted — simpler and harmless.
fn plaintext_delete(name: &str) -> Result<(), String> {
    let mut map = plaintext_load();
    if map.remove(name).is_some() {
        plaintext_save(&map)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Keychain wrappers (the only code that touches the `keyring` crate)
// ---------------------------------------------------------------------------

fn keychain_set(name: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, name).map_err(|e| e.to_string())?;
    entry.set_password(key).map_err(|e| e.to_string())
}

/// Get a key from the keychain. `NoEntry` maps to `Ok(None)` (never stored /
/// already deleted); any other keyring error propagates.
fn keychain_get(name: &str) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, name).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Delete a key from the keychain, treating `NoEntry` as success (idempotent —
/// used by the delete cascade, which must not fail on an already-absent key).
fn keychain_delete(name: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, name).map_err(|e| e.to_string())?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Runtime-probe the keychain: write → read-back-and-compare → delete a
/// throwaway value. Returns `true` only if the full round-trip succeeds.
///
/// This is the ONLY honest way to know a Secret Service is really present —
/// per the design we never guess by OS/distro. `keyring` v4's default features
/// select a real native backend on macOS/Windows and the zbus Secret Service
/// on Linux; if none is reachable (e.g. headless Linux / WSL without a Secret
/// Service) `Entry::new`/`set_password` errors and this returns `false`.
pub fn probe_keychain() -> bool {
    const PROBE_ACCOUNT: &str = "__ccdeck_probe__";
    const PROBE_VALUE: &str = "ccdeck-keychain-probe";

    let entry = match keyring::Entry::new(KEYCHAIN_SERVICE, PROBE_ACCOUNT) {
        Ok(e) => e,
        Err(_) => return false,
    };
    if entry.set_password(PROBE_VALUE).is_err() {
        return false;
    }
    let read_ok = matches!(entry.get_password(), Ok(v) if v == PROBE_VALUE);
    // Best-effort cleanup; don't let a delete failure flip the verdict.
    let _ = entry.delete_credential();
    read_ok
}

// ---------------------------------------------------------------------------
// Pure logic: env-pair building (unit-tested without any keychain)
// ---------------------------------------------------------------------------

/// Build the ordered `(NAME, value)` provider env pairs to export around the
/// launch command. Values are NOT quoted here — the script builders push them
/// through `shell_quote` / `windows_escape`, so a key containing a quote can
/// never break out of the export.
///
/// Order: base URL, auth token, then the optional model. `ANTHROPIC_MODEL` is
/// emitted only when `default_model` is `Some` and non-blank.
pub fn build_provider_env(
    base_url: &str,
    default_model: Option<&str>,
    key: &str,
) -> Vec<(String, String)> {
    let mut env = vec![
        ("ANTHROPIC_BASE_URL".to_string(), base_url.to_string()),
        ("ANTHROPIC_AUTH_TOKEN".to_string(), key.to_string()),
    ];
    if let Some(model) = default_model {
        if !model.trim().is_empty() {
            env.push(("ANTHROPIC_MODEL".to_string(), model.to_string()));
        }
    }
    env
}

/// Resolve a profile's stored key from whichever backend it records, returning
/// the ready-to-export env pairs. Errors if the profile is unknown or has no
/// key stored — callers (resume) turn that into a user-facing message rather
/// than silently launching against the default account.
pub fn provider_env_for(name: &str) -> Result<Vec<(String, String)>, String> {
    let profiles = load_profiles();
    let profile = profiles
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("Provider profile not found: {name}"))?;

    let key = match profile.key_backend {
        KeyBackend::Plaintext => plaintext_get(name),
        // Keychain or None: try the keychain (a None-backend profile simply has
        // no key and will fall through to the error below).
        KeyBackend::Keychain | KeyBackend::None => keychain_get(name).unwrap_or(None),
    };
    let key = key.ok_or_else(|| format!("No API key stored for provider: {name}"))?;

    Ok(build_provider_env(
        &profile.base_url,
        profile.default_model.as_deref(),
        &key,
    ))
}

/// Set `key_backend` on an existing profile (no-op if the profile isn't in the
/// list yet). Keeps the persisted metadata's badge in lock-step with where the
/// key actually landed.
fn update_key_backend(name: &str, backend: KeyBackend) -> Result<(), String> {
    let mut profiles = load_profiles();
    if let Some(p) = profiles.iter_mut().find(|p| p.name == name) {
        p.key_backend = backend;
        save_profiles(&profiles)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// List all provider profiles (metadata only — never keys).
#[tauri::command]
pub fn list_provider_profiles() -> Vec<ProviderProfile> {
    load_profiles()
}

/// Upsert a profile's metadata (name + base_url + optional default_model).
/// `name` is the identity and is immutable — an existing profile is matched by
/// name and only its base_url/default_model are updated; `key_backend` is
/// preserved from disk (authoritative, owned by [`set_provider_key`]) and never
/// taken from the client here. A never-seen name is appended as a new profile.
#[tauri::command]
pub fn save_provider_profile(profile: ProviderProfile) -> Result<(), String> {
    if profile.name.trim().is_empty() {
        return Err("Profile name is required".to_string());
    }
    if profile.base_url.trim().is_empty() {
        return Err("Base URL is required".to_string());
    }
    let mut profiles = load_profiles();
    if let Some(existing) = profiles.iter_mut().find(|p| p.name == profile.name) {
        existing.base_url = profile.base_url;
        existing.default_model = profile.default_model;
        // key_backend intentionally left as-is (owned by set_provider_key).
    } else {
        // New profile: it has no key yet regardless of what the client sent.
        profiles.push(ProviderProfile {
            key_backend: KeyBackend::None,
            ..profile
        });
    }
    save_profiles(&profiles)
}

/// Delete a profile and cascade-remove its secret from BOTH stores so no
/// orphaned key survives: drop it from the profiles list, delete the keychain
/// entry (ignoring `NoEntry`), and drop any plaintext entry. Keychain/plaintext
/// removal is best-effort — a keychain hiccup must not block the profile delete.
#[tauri::command]
pub fn delete_provider_profile(name: String) -> Result<(), String> {
    let mut profiles = load_profiles();
    profiles.retain(|p| p.name != name);
    save_profiles(&profiles)?;
    let _ = keychain_delete(&name);
    let _ = plaintext_delete(&name);
    Ok(())
}

/// Write-only key set. Explicit-opt-in plaintext:
///   * keychain probe passes ⇒ store in keychain, backend `Keychain`, and
///     scrub any stale plaintext copy of this key.
///   * probe fails AND `allow_plaintext` ⇒ store in the plaintext fallback,
///     backend `Plaintext`.
///   * probe fails AND NOT `allow_plaintext` ⇒ error `KEYCHAIN_UNAVAILABLE`
///     (the UI turns this into the "1% outlier" opt-in prompt).
///
/// Never writes plaintext without the flag. Returns the backend actually used
/// so the UI can update its badge; the profile's `key_backend` is also updated
/// server-side to match.
#[tauri::command]
pub fn set_provider_key(
    name: String,
    key: String,
    allow_plaintext: bool,
) -> Result<KeyBackend, String> {
    if key.is_empty() {
        return Err("Key must not be empty".to_string());
    }
    let backend = if probe_keychain() {
        keychain_set(&name, &key)?;
        // Keychain won — remove any stale plaintext copy so it can't linger.
        let _ = plaintext_delete(&name);
        KeyBackend::Keychain
    } else if allow_plaintext {
        plaintext_set(&name, &key)?;
        KeyBackend::Plaintext
    } else {
        return Err("KEYCHAIN_UNAVAILABLE".to_string());
    };
    update_key_backend(&name, backend)?;
    Ok(backend)
}

/// Whether a key is currently stored for this profile (in either backend).
/// Used to render `•••• set ✓` vs an empty write-only field. Never returns the
/// key itself.
#[tauri::command]
pub fn provider_key_status(name: String) -> bool {
    let in_keychain = keychain_get(&name).unwrap_or(None).is_some();
    in_keychain || plaintext_get(&name).is_some()
}

/// Expose the keychain probe to the UI (drives the "keychain available?"
/// affordance and the plaintext opt-in gating).
#[tauri::command]
pub fn provider_probe_keychain() -> bool {
    probe_keychain()
}

// ---------------------------------------------------------------------------
// Tests — pure logic only (no real keychain touched)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_provider_env_includes_base_url_and_token_always() {
        let env = build_provider_env("https://api.deepseek.com/anthropic", None, "sk-abc");
        assert_eq!(
            env,
            vec![
                (
                    "ANTHROPIC_BASE_URL".to_string(),
                    "https://api.deepseek.com/anthropic".to_string()
                ),
                ("ANTHROPIC_AUTH_TOKEN".to_string(), "sk-abc".to_string()),
            ]
        );
    }

    #[test]
    fn build_provider_env_adds_model_only_when_present_and_nonblank() {
        let with = build_provider_env("https://x", Some("deepseek-chat"), "k");
        assert_eq!(with.len(), 3);
        assert_eq!(
            with[2],
            ("ANTHROPIC_MODEL".to_string(), "deepseek-chat".to_string())
        );

        // Blank/whitespace model ⇒ no ANTHROPIC_MODEL export.
        assert_eq!(build_provider_env("https://x", Some("   "), "k").len(), 2);
        assert_eq!(build_provider_env("https://x", Some(""), "k").len(), 2);
        assert_eq!(build_provider_env("https://x", None, "k").len(), 2);
    }

    #[test]
    fn build_provider_env_carries_key_verbatim_for_the_quoting_layer() {
        // The env-pair builder must NOT mangle the key — quoting is the script
        // builder's job. An adversarial key with an embedded quote is carried
        // through byte-for-byte here (its safe escaping is asserted in
        // appconfig's build_resume_script tests).
        let evil = "sk-'; rm -rf ~ #";
        let env = build_provider_env("https://x", None, evil);
        assert_eq!(env[1].1, evil);
    }

    #[test]
    fn provider_profile_round_trips_with_camel_case_and_omits_absent_model() {
        let profile = ProviderProfile {
            name: "DeepSeek".to_string(),
            base_url: "https://api.deepseek.com/anthropic".to_string(),
            default_model: Some("deepseek-chat".to_string()),
            key_backend: KeyBackend::Keychain,
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("\"baseUrl\""));
        assert!(json.contains("\"defaultModel\""));
        assert!(json.contains("\"keyBackend\":\"keychain\""));
        // Never any key field.
        assert!(!json.contains("key\":\"sk"));

        let back: ProviderProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "DeepSeek");
        assert_eq!(back.base_url, "https://api.deepseek.com/anthropic");
        assert_eq!(back.default_model.as_deref(), Some("deepseek-chat"));
        assert_eq!(back.key_backend, KeyBackend::Keychain);
    }

    #[test]
    fn provider_profile_omits_default_model_when_none() {
        let profile = ProviderProfile {
            name: "P".to_string(),
            base_url: "https://x".to_string(),
            default_model: None,
            key_backend: KeyBackend::None,
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(!json.contains("defaultModel"));
        assert!(json.contains("\"keyBackend\":\"none\""));
    }

    #[test]
    fn key_backend_deserializes_from_lowercase_strings() {
        let list: Vec<ProviderProfile> = serde_json::from_str(
            r#"[
                {"name":"A","baseUrl":"https://a","keyBackend":"keychain"},
                {"name":"B","baseUrl":"https://b","keyBackend":"plaintext","defaultModel":"m"},
                {"name":"C","baseUrl":"https://c"}
            ]"#,
        )
        .unwrap();
        assert_eq!(list[0].key_backend, KeyBackend::Keychain);
        assert_eq!(list[1].key_backend, KeyBackend::Plaintext);
        // Missing keyBackend defaults to None.
        assert_eq!(list[2].key_backend, KeyBackend::None);
        assert_eq!(list[2].default_model, None);
    }
}
