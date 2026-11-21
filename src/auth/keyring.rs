use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as BASE64_URL;
use serde_json::Value;
use std::collections::HashMap;

pub async fn get_desktop_keyring_token() -> Option<(String, Value, String)> {
    let connection = zbus::Connection::session().await.ok()?;

    // 1. Open plain session
    let service_proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.secrets",
        "/org/freedesktop/secrets",
        "org.freedesktop.Secret.Service",
    )
    .await
    .ok()?;

    let (empty_variant, session_path): (
        zbus::zvariant::OwnedValue,
        zbus::zvariant::OwnedObjectPath,
    ) = service_proxy
        .call("OpenSession", &("plain", ""))
        .await
        .ok()?;
    let _ = empty_variant;

    // 2. Search items for service='gemini', username='antigravity'
    let mut search_attrs = HashMap::new();
    search_attrs.insert("service", "gemini");
    search_attrs.insert("username", "antigravity");

    let (unlocked, _locked): (
        Vec<zbus::zvariant::OwnedObjectPath>,
        Vec<zbus::zvariant::OwnedObjectPath>,
    ) = service_proxy
        .call("SearchItems", &(search_attrs,))
        .await
        .ok()?;

    if unlocked.is_empty() {
        return None;
    }

    let item_path = &unlocked[0];

    // 3. GetSecret
    let item_proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.secrets",
        item_path.as_str(),
        "org.freedesktop.Secret.Item",
    )
    .await
    .ok()?;

    // Secret structure: (session, parameters, value, content_type)
    let secret_res: (zbus::zvariant::OwnedObjectPath, Vec<u8>, Vec<u8>, String) =
        item_proxy.call("GetSecret", &(session_path,)).await.ok()?;

    let secret_bytes = secret_res.2;
    let raw_val = String::from_utf8(secret_bytes).ok()?;
    let data: Value = serde_json::from_str(&raw_val).ok()?;

    let mut email = String::new();
    if let Some(id_tok) = data.get("id_token").and_then(|v| v.as_str()) {
        let parts: Vec<&str> = id_tok.split('.').collect();
        if parts.len() >= 2 {
            // JWT payload is the second part
            let payload = parts[1];
            if let Ok(dec) = BASE64_URL.decode(payload)
                && let Ok(claims) = serde_json::from_slice::<Value>(&dec)
                && let Some(em) = claims.get("email").and_then(|v| v.as_str())
            {
                email = em.trim().to_lowercase();
            }
        }
    }

    Some((email, data, raw_val))
}
