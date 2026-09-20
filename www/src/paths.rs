pub fn asset_path(path: &str) -> String {
    path.trim().to_string()
}

pub fn optional_asset_path(path: &str) -> Option<String> {
    let path = asset_path(path);
    (!path.is_empty()).then_some(path)
}

pub fn browser_path_for_route(route: &str, location_path: &str) -> String {
    let _ = location_path;
    slash_terminated_route(route)
}

pub fn normalized_path(path: &str) -> String {
    let trimmed = path.trim();
    let with_slash = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("/{trimmed}")
    };
    let mut normalized = with_slash.trim_end_matches('/').to_string();
    if normalized.is_empty() {
        normalized.push('/');
    }
    normalized
}

fn slash_terminated_route(route: &str) -> String {
    let normalized = normalized_path(route);
    if normalized == "/" {
        normalized
    } else {
        format!("{normalized}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_routes_are_root_relative() {
        assert_eq!(browser_path_for_route("/example", "/"), "/example/");
        assert_eq!(
            browser_path_for_route("/another-title", "/"),
            "/another-title/"
        );
        assert_eq!(browser_path_for_route("/", "/example"), "/");
    }

    #[test]
    fn assets_can_be_root_relative() {
        assert_eq!(
            asset_path("/assets/audio-worklet.js"),
            "/assets/audio-worklet.js"
        );
    }

    #[test]
    fn assets_can_be_absolute_urls() {
        assert_eq!(
            asset_path("https://assets.example.org/archive.sit"),
            "https://assets.example.org/archive.sit"
        );
    }

    #[test]
    fn missing_optional_assets_do_not_resolve_to_the_current_page() {
        assert_eq!(optional_asset_path(""), None);
        assert_eq!(optional_asset_path("   "), None);
        assert_eq!(
            optional_asset_path(" https://assets.example.org/screenshot.png "),
            Some("https://assets.example.org/screenshot.png".to_string())
        );
    }
}
