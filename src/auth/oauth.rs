use super::models::UserInfo;
use crate::storage::AccountProfile;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

const B_CID: &[u8] = &[
    115, 114, 117, 115, 114, 114, 116, 114, 116, 114, 119, 123, 115, 111, 54, 47, 42, 49, 49, 43,
    44, 112, 42, 112, 115, 46, 33, 48, 39, 112, 113, 119, 52, 54, 45, 46, 45, 40, 42, 118, 37, 118,
    114, 113, 39, 50, 108, 35, 50, 50, 49, 108, 37, 45, 45, 37, 46, 39, 55, 49, 39, 48, 33, 45, 44,
    54, 39, 44, 54, 108, 33, 45, 47,
];

const B_SEC1: &[u8] = &[
    5, 13, 1, 17, 18, 26, 111, 9, 119, 122, 4, 21, 16, 118, 122, 116, 14, 38, 14, 8, 115, 47, 14,
    0, 122, 49, 26, 1, 118, 56, 116, 51, 6, 3, 36,
];

const B_SEC2: &[u8] = &[
    5, 13, 1, 17, 18, 26, 111, 123, 27, 19, 21, 50, 4, 117, 16, 21, 6, 1, 114, 19, 22, 38, 40, 111,
    27, 58, 9, 15, 53, 16, 114, 24, 54, 49, 26,
];

fn xor_dec(bytes: &[u8], key: u8) -> String {
    bytes.iter().map(|&b| (b ^ key) as char).collect()
}

pub static CLIENT_ID: LazyLock<String> = LazyLock::new(|| {
    std::env::var("ANTIGRAVITY_CLIENT_ID").unwrap_or_else(|_| xor_dec(B_CID, 0x42))
});

pub static CLIENT_SECRETS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let s1 = std::env::var("ANTIGRAVITY_CLIENT_SECRET").unwrap_or_else(|_| xor_dec(B_SEC1, 0x42));
    let s2 = xor_dec(B_SEC2, 0x42);
    vec![s1, s2]
});

pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct GoogleUserInfoResponse {
    email: Option<String>,
    name: Option<String>,
    picture: Option<String>,
}

pub async fn refresh_access_token(refresh_token: &str) -> Option<(String, i64)> {
    if refresh_token.trim().is_empty() {
        return None;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("antigravity/2.16.0")
        .build()
        .ok()?;

    for secret in CLIENT_SECRETS.iter() {
        let mut params = HashMap::new();
        params.insert("client_id", CLIENT_ID.as_str());
        params.insert("client_secret", secret.as_str());
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", refresh_token);

        let Ok(resp) = client.post(TOKEN_URL).form(&params).send().await else {
            continue;
        };

        if resp.status().is_success()
            && let Ok(res) = resp.json::<TokenResponse>().await
            && let Some(tok) = res.access_token
        {
            let expires = res.expires_in.unwrap_or(3600);
            return Some((tok, expires));
        }
    }

    None
}

pub async fn fetch_user_info(access_token: &str) -> Option<UserInfo> {
    if access_token.trim().is_empty() {
        return None;
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("antigravity/2.15.1")
        .build()
        .ok()?;

    let resp = client
        .get(USERINFO_URL)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .ok()?;

    if resp.status().is_success() {
        let data: GoogleUserInfoResponse = resp.json().await.ok()?;
        if let Some(email) = data.email {
            return Some(UserInfo {
                email,
                name: data.name,
                picture: data.picture,
            });
        }
    }

    None
}

pub async fn ensure_valid_token(mut account: AccountProfile) -> (Option<String>, AccountProfile) {
    if let Some(access) = &account.access_token
        && !access.is_empty()
        && let Some(info) = fetch_user_info(access).await
    {
        if account.email.is_empty() {
            account.email = info.email;
        }
        if account.name.is_none() {
            account.name = info.name;
        }
        return (Some(access.clone()), account);
    }

    // Refresh if refresh_token exists
    if let Some(ref_tok) = &account.refresh_token
        && !ref_tok.is_empty()
        && let Some((new_tok, expires_in)) = refresh_access_token(ref_tok).await
    {
        account.access_token = Some(new_tok.clone());
        account.token_expires_in = Some(expires_in);

        if let Some(info) = fetch_user_info(&new_tok).await {
            if account.email.is_empty() {
                account.email = info.email;
            }
            if account.name.is_none() {
                account.name = info.name;
            }
            if account.picture.is_none() {
                account.picture = info.picture;
            }
        }
        return (Some(new_tok), account);
    }

    (None, account)
}
