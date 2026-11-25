//! Local Axum HTTP dashboard, SSE stream, and embedded SPA assets.

/// Embedded frontend static assets and SPA fallback handler.
pub mod assets;
/// Axum HTTP router, API handlers, SSE broadcaster, and server entrypoint.
pub mod server;

pub use assets::{FrontendAssets, static_handler};
pub use server::{
    DoctorQueryParams, HistoryQueryParams, IpQueryParams, QuotaQueryParams, SwitchParams,
    SyncSurfacesParams, WebError, broadcast_event, create_router, start_web_server,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_assets_exist() {
        assert!(FrontendAssets::get("index.html").is_some());
        let index = FrontendAssets::get("index.html").unwrap();
        let html_str = std::str::from_utf8(index.data.as_ref()).unwrap();
        assert!(html_str.contains("<!doctype html>") || html_str.contains("<html"));
    }

    #[tokio::test]
    async fn test_create_router() {
        let router = create_router();
        // Router created cleanly without panicking
        let _ = router;
    }

    #[tokio::test]
    async fn test_cors_allows_localhost_and_rejects_non_localhost() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral localhost port");
        let addr = listener.local_addr().expect("local_addr");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, create_router()).await;
        });

        let client = reqwest::Client::new();
        let base_url = format!("http://{addr}/api/ip");

        // Non-localhost origin must NOT receive Access-Control-Allow-Origin
        let evil_resp = client
            .get(&base_url)
            .header("Origin", "https://evil.example.com")
            .send()
            .await
            .expect("evil origin request");
        assert!(
            !evil_resp
                .headers()
                .contains_key("access-control-allow-origin"),
            "non-localhost origin should not receive Access-Control-Allow-Origin"
        );

        let spoofed_resp = client
            .get(&base_url)
            .header("Origin", "http://localhost.evil.com:8765")
            .send()
            .await
            .expect("spoofed localhost origin request");
        assert!(
            !spoofed_resp
                .headers()
                .contains_key("access-control-allow-origin"),
            "spoofed localhost subdomain should not receive Access-Control-Allow-Origin"
        );

        // Localhost origin with arbitrary port MUST receive Access-Control-Allow-Origin
        let local_resp = client
            .get(&base_url)
            .header("Origin", "http://localhost:5173")
            .send()
            .await
            .expect("localhost origin request");
        assert_eq!(
            local_resp
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://localhost:5173")
        );

        let loopback_resp = client
            .get(&base_url)
            .header("Origin", "http://127.0.0.1:3000")
            .send()
            .await
            .expect("127.0.0.1 origin request");
        assert_eq!(
            loopback_resp
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some("http://127.0.0.1:3000")
        );

        server.abort();
    }

    #[test]
    fn test_web_error_type_exists() {
        let io_err = std::io::Error::new(std::io::ErrorKind::AddrInUse, "in use");
        let web_err = WebError::from(io_err);
        assert!(web_err.to_string().contains("in use"));
    }
}
