//! Help → project links (Vetorial's GitHub repository and the original VectorCraft project it is
//! based on). The commands return the URL; the frontend opens it.

use serde_json::{Value, json};

use super::*;

/// Vetorial's source repository.
pub const REPO_URL: &str = "https://github.com/alefe7z/vetorial-pro";
/// The open-source project Vetorial is based on (MIT OR Apache-2.0), credited in Help and About.
pub const UPSTREAM_URL: &str = "https://github.com/storytold/vectorcraft";

/// The original project's page (credit).
pub fn app_page_url() -> String {
    UPSTREAM_URL.to_string()
}

/// This app's source repository.
pub fn github_url() -> String {
    REPO_URL.to_string()
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "help.appPage", "Original project (VectorCraft)", ["Help"], None, "{} → {url} the open-source project Vetorial is based on", always, |_, _| url(app_page_url())),
        cmd!(query "help.github", "Vetorial on GitHub", ["Help"], None, "{} → {url} source code, issues and releases", always, |_, _| url(github_url())),
        cmd!(query "help.links", "Links", [], None, "{} → {appPage, github}", always, |_, _| {
            Ok(json!({ "appPage": app_page_url(), "github": github_url() }))
        }),
    ]
}

fn url(u: String) -> Result<Value> {
    Ok(json!({ "url": u }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        let mut s = Session::new();
        assert_eq!(s.execute("help.appPage", &json!({})).unwrap()["url"], UPSTREAM_URL);
        assert_eq!(s.execute("help.github", &json!({})).unwrap()["url"], REPO_URL);
        assert_eq!(s.execute("help.links", &json!({})).unwrap()["github"], REPO_URL);
        assert!(s.execute("help.discord", &json!({})).is_err(), "no third-party community links");
    }
}
