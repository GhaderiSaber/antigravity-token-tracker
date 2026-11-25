use super::models::KeyringTokenRecord;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use serde_json::Value;
use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as ZValue};

const SECRET_SERVICE_DEST: &str = "org.freedesktop.secrets";
const SECRET_SERVICE_PATH: &str = "/org/freedesktop/secrets";
const SECRET_SERVICE_IFACE: &str = "org.freedesktop.Secret.Service";
const SECRET_COLLECTION_IFACE: &str = "org.freedesktop.Secret.Collection";
const SECRET_ITEM_IFACE: &str = "org.freedesktop.Secret.Item";
const DESKTOP_KEYRING_SERVICE: &str = "gemini";
const DESKTOP_KEYRING_USERNAME: &str = "antigravity";
const DESKTOP_KEYRING_LABEL: &str = "Password for 'antigravity' on 'gemini'";

/// Errors returned when interacting with the Linux Secret Service desktop keyring.
#[must_use]
#[derive(Debug, thiserror::Error)]
pub enum KeyringError {
    /// Failed to communicate with `org.freedesktop.secrets` over D-Bus.
    #[error("D-Bus secret service error: {0}")]
    Dbus(#[from] zbus::Error),
    /// Stored keyring secret could not be decoded as UTF-8 or JSON.
    #[error("invalid keyring payload: {0}")]
    InvalidPayload(String),
}

/// Returns the `(algorithm, input)` arguments required by `org.freedesktop.Secret.Service.OpenSession` (`(sv)`).
#[must_use]
pub(crate) fn open_plain_session_args() -> (&'static str, ZValue<'static>) {
    ("plain", ZValue::from(""))
}

fn desktop_search_attributes() -> HashMap<&'static str, &'static str> {
    let mut attrs = HashMap::new();
    attrs.insert("service", DESKTOP_KEYRING_SERVICE);
    attrs.insert("username", DESKTOP_KEYRING_USERNAME);
    attrs
}

async fn open_plain_session(
    connection: &zbus::Connection,
) -> Result<(zbus::Proxy<'static>, OwnedObjectPath), KeyringError> {
    let service_proxy = zbus::Proxy::new(
        connection,
        SECRET_SERVICE_DEST,
        SECRET_SERVICE_PATH,
        SECRET_SERVICE_IFACE,
    )
    .await?;

    let (_empty_variant, session_path): (OwnedValue, OwnedObjectPath) = service_proxy
        .call("OpenSession", &open_plain_session_args())
        .await?;

    Ok((service_proxy, session_path))
}

async fn search_desktop_items(
    service_proxy: &zbus::Proxy<'_>,
) -> Result<Vec<OwnedObjectPath>, KeyringError> {
    let (unlocked, _locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) = service_proxy
        .call("SearchItems", &(desktop_search_attributes(),))
        .await?;
    Ok(unlocked)
}

/// Parses a raw Antigravity desktop keyring JSON secret into a [`KeyringTokenRecord`].
///
/// Prefers `id_token_email` (Antigravity v2.35+) and falls back to extracting the `email`
/// claim from a legacy `id_token` JWT payload.
#[must_use]
pub fn parse_keyring_token_record(raw_val: &str) -> Option<KeyringTokenRecord> {
    let data: Value = serde_json::from_str(raw_val).ok()?;

    let mut email = data
        .get("id_token_email")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    if email.is_empty()
        && let Some(id_tok) = data.get("id_token").and_then(|v| v.as_str())
    {
        let parts: Vec<&str> = id_tok.split('.').collect();
        if parts.len() >= 2
            && let Ok(dec) = BASE64_URL.decode(parts[1])
            && let Ok(claims) = serde_json::from_slice::<Value>(&dec)
            && let Some(em) = claims.get("email").and_then(|v| v.as_str())
        {
            email = em.trim().to_lowercase();
        }
    }

    Some(KeyringTokenRecord {
        email,
        data,
        raw_secret: raw_val.to_string(),
    })
}

/// Verifies that `org.freedesktop.secrets` accepts `OpenSession("plain", "")` (`(sv)`) and `SearchItems`.
///
/// # Errors
///
/// Returns [`KeyringError`] if the Secret Service D-Bus daemon is unreachable or rejects the protocol call.
pub async fn probe_keyring_session() -> Result<(), KeyringError> {
    let connection = zbus::Connection::session().await?;
    let (service_proxy, _session_path) = open_plain_session(&connection).await?;
    let _ = search_desktop_items(&service_proxy).await?;
    Ok(())
}

/// Reads the active Antigravity Desktop secret from `org.freedesktop.secrets`.
pub async fn desktop_keyring_token() -> Option<KeyringTokenRecord> {
    let connection = zbus::Connection::session().await.ok()?;
    let (service_proxy, session_path) = open_plain_session(&connection).await.ok()?;
    let unlocked = search_desktop_items(&service_proxy).await.ok()?;
    let item_path = unlocked.first()?;

    let item_proxy = zbus::Proxy::new(
        &connection,
        SECRET_SERVICE_DEST,
        item_path.as_str(),
        SECRET_ITEM_IFACE,
    )
    .await
    .ok()?;

    let secret_res: (OwnedObjectPath, Vec<u8>, Vec<u8>, String) =
        item_proxy.call("GetSecret", &(session_path,)).await.ok()?;

    let raw_val = String::from_utf8(secret_res.2).ok()?;
    parse_keyring_token_record(&raw_val)
}

/// Stores or replaces the Antigravity Desktop credential payload in `org.freedesktop.secrets`.
///
/// # Errors
///
/// Returns [`KeyringError`] if the Secret Service D-Bus call fails.
pub async fn set_desktop_keyring_secret(secret_json_str: &str) -> Result<(), KeyringError> {
    let connection = zbus::Connection::session().await?;
    let (service_proxy, session_path) = open_plain_session(&connection).await?;
    let unlocked = search_desktop_items(&service_proxy).await?;

    let secret_struct = (
        session_path,
        Vec::<u8>::new(),
        secret_json_str.as_bytes().to_vec(),
        "text/plain; charset=utf-8",
    );

    if let Some(item_path) = unlocked.first() {
        let item_proxy = zbus::Proxy::new(
            &connection,
            SECRET_SERVICE_DEST,
            item_path.as_str(),
            SECRET_ITEM_IFACE,
        )
        .await?;
        let (): () = item_proxy.call("SetSecret", &(secret_struct,)).await?;
        return Ok(());
    }

    let default_col: OwnedObjectPath = service_proxy.call("ReadAlias", &("default",)).await?;
    let col_proxy = zbus::Proxy::new(
        &connection,
        SECRET_SERVICE_DEST,
        default_col.as_str(),
        SECRET_COLLECTION_IFACE,
    )
    .await?;

    let mut item_props = HashMap::new();
    item_props.insert(
        "org.freedesktop.Secret.Item.Label",
        ZValue::from(DESKTOP_KEYRING_LABEL),
    );
    item_props.insert(
        "org.freedesktop.Secret.Item.Attributes",
        ZValue::from(desktop_search_attributes()),
    );

    let (_item_path, _prompt): (OwnedObjectPath, OwnedObjectPath) = col_proxy
        .call("CreateItem", &(item_props, secret_struct, true))
        .await?;
    Ok(())
}

/// Deletes the Antigravity Desktop credential item from `org.freedesktop.secrets` if present.
///
/// # Errors
///
/// Returns [`KeyringError`] if communicating with `org.freedesktop.secrets` fails.
pub async fn clear_desktop_keyring_secret() -> Result<bool, KeyringError> {
    let connection = zbus::Connection::session().await?;
    let service_proxy = zbus::Proxy::new(
        &connection,
        SECRET_SERVICE_DEST,
        SECRET_SERVICE_PATH,
        SECRET_SERVICE_IFACE,
    )
    .await?;

    let unlocked = search_desktop_items(&service_proxy).await?;
    let Some(item_path) = unlocked.first() else {
        return Ok(false);
    };

    let item_proxy = zbus::Proxy::new(
        &connection,
        SECRET_SERVICE_DEST,
        item_path.as_str(),
        SECRET_ITEM_IFACE,
    )
    .await?;
    let _prompt: OwnedObjectPath = item_proxy.call("Delete", &()).await?;
    Ok(true)
}

#[inline]
pub async fn get_desktop_keyring_token() -> Option<KeyringTokenRecord> {
    desktop_keyring_token().await
}
