//! Web module.

pub mod assets;
pub mod server;

pub use assets::*;
pub use server::*;

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
}
