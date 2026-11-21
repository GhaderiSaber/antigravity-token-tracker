# CLI Terminal Presentation & Interaction (`src/cli`)

## Overview
Provides terminal display rendering, progress bars, account status tables, and interactive terminal prompts for the `agy-token` command-line interface.

## Interface
- `pub fn format_progress_bar(pct: f64) -> String`: Formats a 10-block unicode colored progress bar for quota percentages `[src/cli/display.rs:format_progress_bar]`
- `pub fn render_account_table(email: &str, account: &AccountQuotaResult, is_active: bool) -> String`: Renders formatted terminal tables for an account's quota buckets and reset countdowns `[src/cli/display.rs:render_account_table]`
- `pub fn render_status_dashboard(all_quotas: &HashMap<String, AccountQuotaResult>, geo_info: &EgressGeo, shield_cfg: &ShieldConfig, bal_cfg: &BalancerConfig) -> String`: Generates the complete colored status overview dashboard `[src/cli/display.rs:render_status_dashboard]`
- `pub async fn run_interactive_switcher(accounts: &[AccountProfile]) -> Result<(), String>`: Prompts the user with an interactive terminal list to switch the active account session `[src/cli/interactive.rs:run_interactive_switcher]`
- `pub mod display`: Terminal table and formatting utilities `[src/cli/mod.rs:display]`
- `pub mod interactive`: Terminal prompt and interactive selection handlers `[src/cli/mod.rs:interactive]`

## Invariants
- Clamps filled block count to a maximum of 10 via `filled.min(10)` so total progress bar width is always 10 blocks `[src/cli/display.rs:format_progress_bar]`
- Defaults account tier to `"Standard"` if `account.tier.is_empty()` `[src/cli/display.rs:render_account_table]`
- Truncates group display name to 25 characters followed by `"..."` if `g.display_name.len() > 28` `[src/cli/display.rs:render_account_table]`
- Falls back to 0.0 remaining percent, `"-"` countdown, and `"HEALTHY"` status badge when `g.weekly` is None `[src/cli/display.rs:render_account_table]`
- Returns `Err("No accounts registered to switch.")` if `accounts.is_empty()` `[src/cli/interactive.rs:run_interactive_switcher]`
- Returns `Ok(())` on `switch_to_account` success, otherwise propagates `Err(msg)` `[src/cli/interactive.rs:run_interactive_switcher]`

## Side Effects
- Blocks and reads user input interactively from terminal via `inquire::Select::prompt()` `[src/cli/interactive.rs:run_interactive_switcher]`
- Switches active account session and potentially restarts Antigravity processes by awaiting `switch_to_account(email, true, "both")` `[src/cli/interactive.rs:run_interactive_switcher]`
- Prints messages to stdout via `println!` `[src/cli/interactive.rs:run_interactive_switcher]`

## Verification
- Run: `cargo test`
