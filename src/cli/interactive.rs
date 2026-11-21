use crate::balancer::switcher::switch_to_account;
use crate::storage::models::AccountProfile;

pub async fn run_interactive_switcher(accounts: &[AccountProfile]) -> Result<(), String> {
    if accounts.is_empty() {
        return Err("No accounts registered to switch.".into());
    }

    let mut options = Vec::new();
    for acc in accounts {
        let active = if acc.active == Some(true) {
            " (● Current Active)"
        } else {
            ""
        };
        let tier = acc.tier.as_deref().unwrap_or("Standard");
        options.push(format!("{} [{Tier}]{active}", acc.email, Tier = tier));
    }

    let ans = inquire::Select::new("Select account to activate in Antigravity:", options)
        .prompt()
        .map_err(|e| format!("Selection cancelled: {e}"))?;

    let email = ans.split_whitespace().next().unwrap_or_default();
    let (success, msg) = switch_to_account(email, true, "both").await;
    if success {
        println!("✓ {msg}");
        Ok(())
    } else {
        Err(msg)
    }
}
