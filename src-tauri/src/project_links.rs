//! Opens the project's public pages in the system browser.
//!
//! The renderer only names which page it wants; the URLs live here, so the
//! webview cannot ask the shell to open an arbitrary address.

use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const REPOSITORY_URL: &str = "https://github.com/VirgileVP/Decktector";
const ISSUES_URL: &str = "https://github.com/VirgileVP/Decktector/issues";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectLink {
    Repository,
    Issues,
}

impl ProjectLink {
    fn url(self) -> &'static str {
        match self {
            Self::Repository => REPOSITORY_URL,
            Self::Issues => ISSUES_URL,
        }
    }
}

/// Opens `link` in the user's default browser.
#[tauri::command]
pub fn open_project_link(app: AppHandle, link: ProjectLink) -> Result<(), String> {
    app.opener()
        .open_url(link.url(), None::<&str>)
        .map_err(|error| format!("Failed to open {}: {error}", link.url()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_the_names_the_renderer_sends() {
        let repository: ProjectLink = serde_json::from_str("\"repository\"").unwrap();
        let issues: ProjectLink = serde_json::from_str("\"issues\"").unwrap();
        assert_eq!(repository, ProjectLink::Repository);
        assert_eq!(issues, ProjectLink::Issues);
    }

    #[test]
    fn rejects_an_unknown_page() {
        assert!(serde_json::from_str::<ProjectLink>("\"https://example.com\"").is_err());
    }

    #[test]
    fn every_link_stays_on_the_project_repository() {
        for link in [ProjectLink::Repository, ProjectLink::Issues] {
            assert!(link.url().starts_with(REPOSITORY_URL));
        }
    }
}
