use std::collections::HashMap;
use std::process::Command;

pub async fn send_desktop_notification(summary: &str, body: &str, urgency: &str) -> bool {
    if let Ok(connection) = zbus::Connection::session().await
        && let Ok(proxy) = zbus::Proxy::new(
            &connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await
    {
        let mut hints = HashMap::new();
        let urgency_val: u8 = match urgency {
            "low" => 0,
            "critical" => 2,
            _ => 1,
        };
        hints.insert("urgency", zbus::zvariant::Value::from(urgency_val));

        let actions: Vec<String> = Vec::new();
        let res: Result<u32, _> = proxy
            .call(
                "Notify",
                &(
                    "Antigravity Token Tracker",
                    0u32,
                    "dialog-information",
                    summary,
                    body,
                    actions,
                    hints,
                    5000i32,
                ),
            )
            .await;
        if res.is_ok() {
            return true;
        }
    }

    // Fallback to notify-send
    Command::new("notify-send")
        .args([
            "-u",
            urgency,
            "-a",
            "Antigravity Token Tracker",
            summary,
            body,
        ])
        .output()
        .is_ok_and(|o| o.status.success())
}
