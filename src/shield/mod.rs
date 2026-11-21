pub mod geo;
pub mod models;
#[allow(clippy::module_inception)]
pub mod shield;

pub use geo::*;
pub use models::*;
pub use shield::*;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_shield_enums_serde_and_from_str() {
        let mode: ShieldMode = serde_json::from_str("\"whitelist\"").unwrap();
        assert_eq!(mode, ShieldMode::Whitelist);
        assert_eq!(serde_json::to_string(&mode).unwrap(), "\"whitelist\"");

        let mode_re: ShieldMode = serde_json::from_str("\"restricted\"").unwrap();
        assert_eq!(mode_re, ShieldMode::Restricted);
        assert_eq!(serde_json::to_string(&mode_re).unwrap(), "\"restricted\"");

        let action: ShieldAction = serde_json::from_str("\"kill_process\"").unwrap();
        assert_eq!(action, ShieldAction::KillProcess);
        assert_eq!(action.to_string(), "kill_process");

        assert_eq!(
            "kill_network".parse::<ShieldAction>().unwrap(),
            ShieldAction::KillNetwork
        );
        assert_eq!("both".parse::<ShieldAction>().unwrap(), ShieldAction::Both);
        assert_eq!(
            "KILL_PROCESS".parse::<ShieldAction>().unwrap(),
            ShieldAction::KillProcess
        );
        assert_eq!(
            "Kill_Network".parse::<ShieldAction>().unwrap(),
            ShieldAction::KillNetwork
        );
        assert_eq!(
            "restricted".parse::<ShieldMode>().unwrap(),
            ShieldMode::Restricted
        );
        assert_eq!(
            "WHITELIST".parse::<ShieldMode>().unwrap(),
            ShieldMode::Whitelist
        );
        assert_eq!(
            "  whitelist  ".parse::<ShieldMode>().unwrap(),
            ShieldMode::Whitelist
        );

        assert!("invalid_mode".parse::<ShieldMode>().is_err());
        assert!("invalid_action".parse::<ShieldAction>().is_err());
    }

    #[test]
    fn test_flag_emoji_and_restricted() {
        assert_eq!(get_flag_emoji("US"), "🇺🇸");
        assert_eq!(get_flag_emoji("de"), "🇩🇪");
        assert_eq!(get_flag_emoji("IR"), "🇮🇷");
        assert_eq!(get_flag_emoji(""), "🌐");
        assert_eq!(get_flag_emoji("INVALID"), "🌐");

        assert!(is_restricted_country("IR"));
        assert!(is_restricted_country("ru"));
        assert!(is_restricted_country("CN"));
        assert!(!is_restricted_country("US"));
        assert!(!is_restricted_country("DE"));
    }

    #[test]
    fn test_format_ip_summary() {
        let geo = EgressGeo {
            ip: "1.2.3.4".into(),
            country_code: "US".into(),
            city: "San Jose".into(),
            flag: "🇺🇸".into(),
            ..Default::default()
        };
        assert_eq!(format_ip_summary(&geo, true), "🇺🇸 San Jose, US (1.2.3.4)");
        assert_eq!(format_ip_summary(&geo, false), "🇺🇸 San Jose, US");
    }

    #[test]
    fn test_shield_policy_restricted_and_whitelist() {
        let mut cfg = ShieldConfig {
            enabled: true,
            mode: ShieldMode::Restricted,
            ..Default::default()
        };

        let safe_geo = EgressGeo {
            ip: "1.2.3.4".into(),
            country_code: "US".into(),
            country_name: "United States".into(),
            is_restricted: false,
            ..Default::default()
        };
        let (allowed, _) = is_ip_allowed(&safe_geo, &cfg);
        assert!(allowed);

        let restricted_geo = EgressGeo {
            ip: "5.6.7.8".into(),
            country_code: "IR".into(),
            country_name: "Iran".into(),
            is_restricted: true,
            ..Default::default()
        };
        let (blocked, reason) = is_ip_allowed(&restricted_geo, &cfg);
        assert!(!blocked);
        assert!(reason.contains("restricted region"));

        // Switch to whitelist mode
        cfg.mode = ShieldMode::Whitelist;
        cfg.allowed_countries = vec!["DE".into(), "NL".into()];

        let (allowed_wl, _) = is_ip_allowed(
            &EgressGeo {
                country_code: "DE".into(),
                ip: "10.0.0.1".into(),
                ..Default::default()
            },
            &cfg,
        );
        assert!(allowed_wl);

        let (blocked_wl, reason_wl) = is_ip_allowed(
            &EgressGeo {
                country_code: "US".into(),
                country_name: "United States".into(),
                ip: "1.2.3.4".into(),
                ..Default::default()
            },
            &cfg,
        );
        assert!(!blocked_wl);
        assert!(reason_wl.contains("outside allowed whitelist"));
    }

    #[test]
    fn test_shield_config_save_load() {
        let dir = tempdir().unwrap();
        let loaded_def = load_shield_config(dir.path());
        assert!(loaded_def.enabled);
        assert_eq!(loaded_def.mode, ShieldMode::Restricted);

        let updated = set_shield_enabled(dir.path(), false).unwrap();
        assert!(!updated.enabled);

        let reloaded = load_shield_config(dir.path());
        assert!(!reloaded.enabled);
    }

    #[tokio::test]
    async fn test_shield_evaluation_no_poisoning() {
        let dir = tempdir().unwrap();
        let config_dir = dir.path();
        let def = ShieldConfig::default();
        save_shield_config(config_dir, &def).unwrap();

        let geo = EgressGeo {
            ip: "1.1.1.1".into(),
            country_code: "US".into(),
            country_name: "United States".into(),
            is_restricted: false,
            ..Default::default()
        };
        // Run multiple parallel evaluations
        let mut handles = Vec::new();
        for _ in 0..10 {
            let g = geo.clone();
            let path = config_dir.to_path_buf();
            handles.push(tokio::spawn(async move {
                evaluate_and_enforce_shield(&path, Some(g)).await
            }));
        }
        for h in handles {
            let res = h.await.unwrap();
            assert!(!res.violation);
        }
    }
}
