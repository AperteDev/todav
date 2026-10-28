//! The latest stable release on GitHub, for About to offer a newer version.

use crate::json::Json;

/// Stable only: GitHub leaves out drafts and pre-releases.
const LATEST: &str = "https://api.github.com/repos/AperteDev/todav/releases/latest";

/// The version and the page of the latest stable release, if newer than `current`; None too
/// when it cannot be checked.
pub fn newer(current: &str) -> Option<(String, String)> {
    let agent = ureq::Agent::config_builder()
        .user_agent("Todav")
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent();
    let body = agent
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    let json = Json::parse(&body)?;
    let version = json.get("tag_name")?.str()?.trim_start_matches('v');
    let url = json.get("html_url")?.str()?;
    is_newer(version, current).then(|| (version.to_string(), url.to_string()))
}

/// `version` is above `current`, both plain dotted numbers (anything else is not newer).
fn is_newer(version: &str, current: &str) -> bool {
    let parse = |v: &str| {
        v.split('.')
            .map(|n| n.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()
    };
    matches!((parse(version), parse(current)), (Some(v), Some(c)) if v > c)
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_numbers() {
        assert!(is_newer("2.0.10", "2.0.9"));
        assert!(is_newer("2.1", "2.0.2"));
        assert!(!is_newer("2.0.2", "2.0.2"));
        assert!(!is_newer("1.9.9", "2.0.0"));
        assert!(!is_newer("2.1.0-rc1", "2.0.0"));
    }
}
