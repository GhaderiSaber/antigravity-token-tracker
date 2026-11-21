use crate::shield::models::EgressGeo;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const RESTRICTED_COUNTRIES: [&str; 7] = ["IR", "RU", "BY", "KP", "CU", "SY", "CN"];
pub const CACHE_TTL_SECS: f64 = 60.0;

static RESTRICTED_SET: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    for c in RESTRICTED_COUNTRIES {
        set.insert(c);
    }
    set
});

struct GeoCache {
    data: Option<EgressGeo>,
    timestamp: f64,
}

static GEO_CACHE: Mutex<GeoCache> = Mutex::new(GeoCache {
    data: None,
    timestamp: 0.0,
});

pub fn get_flag_emoji(country_code: &str) -> String {
    let code = country_code.trim().to_uppercase();
    if code.len() != 2 {
        return "🌐".to_string();
    }
    let chars: Vec<char> = code.chars().collect();
    if chars[0].is_ascii_alphabetic() && chars[1].is_ascii_alphabetic() {
        let base = 0x1F1E6_u32;
        let c1 = char::from_u32(base + (chars[0] as u32 - 'A' as u32)).unwrap_or('🌐');
        let c2 = char::from_u32(base + (chars[1] as u32 - 'A' as u32)).unwrap_or('🌐');
        format!("{c1}{c2}")
    } else {
        "🌐".to_string()
    }
}

pub fn is_restricted_country(country_code: &str) -> bool {
    let upper = country_code.trim().to_uppercase();
    RESTRICTED_SET.contains(upper.as_str())
}

pub fn format_ip_summary(geo: &EgressGeo, include_ip: bool) -> String {
    let flag = if geo.flag.is_empty() {
        get_flag_emoji(&geo.country_code)
    } else {
        geo.flag.clone()
    };

    let cc = if geo.country_code.is_empty() {
        "Unknown"
    } else {
        &geo.country_code
    };

    let loc_part = if !geo.city.is_empty() {
        format!("{}, {}", geo.city, cc)
    } else {
        cc.to_string()
    };

    if include_ip && geo.ip != "Unknown" && !geo.ip.is_empty() {
        format!("{} {} ({})", flag, loc_part, geo.ip)
    } else {
        format!("{flag} {loc_part}")
    }
}

pub fn now_epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

pub async fn query_ip_geo() -> Option<EgressGeo> {
    let endpoints = ["https://ipwho.is/", "https://ipapi.co/json/"];

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(3500))
        .user_agent("antigravity-token-tracker/1.0")
        .build()
        .ok()?;

    for url in endpoints {
        let Ok(resp) = client.get(url).send().await else {
            continue;
        };

        let Ok(val) = resp.json::<Value>().await else {
            continue;
        };

        let ip = val
            .get("ip")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if ip.is_empty() {
            continue;
        }

        let mut country_code = val
            .get("country_code")
            .or_else(|| val.get("country"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_uppercase();

        let mut country_name = val
            .get("country")
            .or_else(|| val.get("country_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        if country_code.len() > 2 && country_name.len() <= 2 {
            std::mem::swap(&mut country_code, &mut country_name);
        }
        if country_code.len() != 2 {
            country_code = country_code
                .chars()
                .take(2)
                .collect::<String>()
                .to_uppercase();
        }

        let city = val
            .get("city")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let region = val
            .get("region")
            .or_else(|| val.get("region_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let conn = val.get("connection");
        let isp = conn
            .and_then(|c| c.get("isp"))
            .or_else(|| val.get("org"))
            .or_else(|| conn.and_then(|c| c.get("org")))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let org = conn
            .and_then(|c| c.get("org"))
            .or_else(|| val.get("org"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let flag_from_val = val
            .get("flag")
            .and_then(|f| f.get("emoji"))
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        let flag = flag_from_val.unwrap_or_else(|| get_flag_emoji(&country_code));
        let is_restricted = is_restricted_country(&country_code);
        let checked_at = now_epoch();

        let mut geo = EgressGeo {
            ip,
            country_code,
            country_name,
            region,
            city,
            isp,
            org,
            flag,
            is_restricted,
            checked_at,
            summary: String::new(),
        };
        geo.summary = format_ip_summary(&geo, true);
        return Some(geo);
    }

    None
}

pub async fn get_ip_geo(force_refresh: bool) -> EgressGeo {
    let now = now_epoch();

    if !force_refresh {
        let cache = GEO_CACHE.lock().unwrap();
        if let Some(cached) = &cache.data
            && now - cache.timestamp < CACHE_TTL_SECS
        {
            return cached.clone();
        }
    }

    if let Some(res) = query_ip_geo().await {
        let mut cache = GEO_CACHE.lock().unwrap();
        cache.data = Some(res.clone());
        cache.timestamp = now;
        return res;
    }

    // Return cached if network failed
    {
        let cache = GEO_CACHE.lock().unwrap();
        if let Some(cached) = &cache.data {
            return cached.clone();
        }
    }

    // Default fallback
    EgressGeo {
        ip: "Unknown".into(),
        country_code: "".into(),
        country_name: "Checking...".into(),
        region: "".into(),
        city: "".into(),
        isp: "".into(),
        org: "".into(),
        flag: "🌐".into(),
        is_restricted: false,
        checked_at: now,
        summary: "🌐 Egress IP Checking...".into(),
    }
}
