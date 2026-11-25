use std::collections::HashMap;
use tokio::process::Command;
use tokio_stream::StreamExt;

/// Sends a desktop notification with interactive action buttons via `org.freedesktop.Notifications`.
///
/// Each tuple in `actions` is `(action_id, label)`. Returns `Some(notification_id)` when D-Bus
/// accepts the notification, or `None` if D-Bus delivery fails.
pub async fn send_desktop_notification_with_actions(
    summary: &str,
    body: &str,
    urgency: &str,
    actions: &[(&str, &str)],
) -> Option<u32> {
    let connection = zbus::Connection::session().await.ok()?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
    .ok()?;

    let mut hints = HashMap::new();
    let urgency_val: u8 = match urgency {
        "low" => 0,
        "critical" => 2,
        _ => 1,
    };
    hints.insert("urgency", zbus::zvariant::Value::from(urgency_val));

    let flat_actions: Vec<&str> = actions
        .iter()
        .flat_map(|(id, label)| [*id, *label])
        .collect();
    let res: Result<u32, _> = proxy
        .call(
            "Notify",
            &(
                "Antigravity Token Tracker",
                0u32,
                "dialog-information",
                summary,
                body,
                flat_actions,
                hints,
                5000i32,
            ),
        )
        .await;
    res.ok()
}

/// Sends a desktop notification via D-Bus, falling back to `notify-send` if D-Bus is unavailable.
pub async fn send_desktop_notification(summary: &str, body: &str, urgency: &str) -> bool {
    if send_desktop_notification_with_actions(summary, body, urgency, &[])
        .await
        .is_some()
    {
        return true;
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
        .await
        .is_ok_and(|o| o.status.success())
}

/// Spawns a background task listening for `org.freedesktop.Notifications` `ActionInvoked` D-Bus signals.
///
/// Invokes `on_action` with the action key whenever a user clicks an interactive notification button.
/// Exits quietly if the session bus or notifications service is unavailable.
#[must_use]
pub fn spawn_notification_action_listener(
    on_action: impl Fn(String) + Send + Sync + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(connection) = zbus::Connection::session().await else {
            return;
        };
        let Ok(proxy) = zbus::Proxy::new(
            &connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )
        .await
        else {
            return;
        };
        let Ok(mut stream) = proxy.receive_signal("ActionInvoked").await else {
            return;
        };

        while let Some(msg) = stream.next().await {
            if let Ok((_id, action_key)) = msg.body().deserialize::<(u32, String)>() {
                on_action(action_key);
            }
        }
    })
}
