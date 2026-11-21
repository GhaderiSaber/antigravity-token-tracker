use super::keyring::get_desktop_keyring_token;
use super::oauth::ensure_valid_token;
use super::process_discovery::{discover_antigravity_desktop_app, discover_antigravity_ide};
use super::sqlite::{
    default_ide_state_db, extract_tokens_from_state_db, extract_user_email_from_state_db,
    extract_user_tier,
};
use crate::storage::{AccountProfile, StorageManager};
use std::collections::HashMap;

pub async fn sync_from_antigravity(
    storage: &StorageManager,
) -> Result<
    (Option<String>, HashMap<String, AccountProfile>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut registry = storage.load_accounts().unwrap_or_default();
    let mut active_email: Option<String> = None;

    // 1. Discover Desktop App
    let desktop_session = discover_antigravity_desktop_app().await;
    let desktop_email = desktop_session.as_ref().map(|s| s.email.to_lowercase());

    if let Some(ref d_email) = desktop_email {
        active_email = Some(d_email.clone());
        let existing = registry.accounts.entry(d_email.clone()).or_default();

        let mut access_tok = existing.access_token.clone().unwrap_or_default();
        let mut refresh_tok = existing.refresh_token.clone().unwrap_or_default();

        if let Some((_, kr_data, _)) = get_desktop_keyring_token().await
            && let Some(tok_obj) = kr_data.get("token")
        {
            if let Some(acc) = tok_obj.get("access_token").and_then(|v| v.as_str()) {
                access_tok = acc.to_string();
            }
            if let Some(ref_t) = tok_obj.get("refresh_token").and_then(|v| v.as_str()) {
                refresh_tok = ref_t.to_string();
            }
        }

        existing.email = d_email.clone();
        if let Some(s) = &desktop_session {
            if s.name.is_some() {
                existing.name = s.name.clone();
            }
            if s.tier.is_some() {
                existing.tier = s.tier.clone();
            }
            existing.app_port = s.port;
            existing.app_csrf_token = s.csrf_token.clone();
        }

        if !access_tok.is_empty() {
            existing.access_token = Some(access_tok);
        }
        if !refresh_tok.is_empty() {
            existing.refresh_token = Some(refresh_tok);
        }

        existing.active = Some(true);
        existing.is_current_desktop_session = Some(true);
        existing.last_synced = Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0);
    }

    // 2. Discover IDE Session via process
    let ide_proc_session = discover_antigravity_ide().await;
    let ide_proc_email = ide_proc_session.as_ref().map(|s| s.email.to_lowercase());

    if let Some(ref i_email) = ide_proc_email {
        if active_email.is_none() {
            active_email = Some(i_email.clone());
        }
        let existing = registry.accounts.entry(i_email.clone()).or_default();
        existing.email = i_email.clone();
        if let Some(s) = &ide_proc_session {
            if s.name.is_some() && existing.name.is_none() {
                existing.name = s.name.clone();
            }
            if s.tier.is_some() {
                existing.tier = s.tier.clone();
            }
        }
        existing.active = Some(true);
        existing.is_current_ide_session = Some(true);
        existing.last_synced = Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0);
    }

    // 3. Fallback: discover IDE session via state.vscdb
    let db_path = default_ide_state_db();
    if db_path.exists()
        && let Some(sqlite_email) = extract_user_email_from_state_db(&db_path)
    {
        let s_email = sqlite_email.to_lowercase();
        if active_email.is_none() {
            active_email = Some(s_email.clone());
        }

        let existing = registry.accounts.entry(s_email.clone()).or_default();
        existing.email = s_email.clone();

        let tier = extract_user_tier(&db_path);
        if existing.tier.is_none() {
            existing.tier = Some(tier);
        }

        if let Some(tokens) = extract_tokens_from_state_db(&db_path) {
            if !tokens.access_token.is_empty() {
                existing.access_token = Some(tokens.access_token);
            }
            if !tokens.refresh_token.is_empty() {
                existing.refresh_token = Some(tokens.refresh_token);
            }
        }

        existing.active = Some(true);
        existing.is_current_ide_session = Some(true);
        existing.last_synced = Some(chrono::Utc::now().timestamp_millis() as f64 / 1000.0);
    }

    // Update active surface flags for all accounts
    let d_active = desktop_email.unwrap_or_default();
    let i_active = ide_proc_email.unwrap_or_default();

    for (email, acc) in registry.accounts.iter_mut() {
        let is_d = !d_active.is_empty() && email.to_lowercase() == d_active;
        let is_i = !i_active.is_empty() && email.to_lowercase() == i_active;
        acc.is_current_desktop_session = Some(is_d);
        acc.is_current_ide_session = Some(is_i);

        // Ensure token valid
        let (_, updated) = ensure_valid_token(acc.clone()).await;
        *acc = updated;
    }

    registry.active_email = active_email.clone();
    storage.save_accounts(&registry)?;

    Ok((active_email, registry.accounts))
}
