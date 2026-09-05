use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub latest_version: String,
    pub current_version: String,
    pub url: String,
    pub body: Option<String>,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    draft: Option<bool>,
    prerelease: Option<bool>,
}

fn normalize_version(v: &str) -> String {
    let v = v.trim();
    let v = v
        .strip_prefix('v')
        .or_else(|| v.strip_prefix('V'))
        .unwrap_or(v);
    v.to_string()
}

pub fn check_for_update(current_version: &str) -> Option<UpdateInfo> {
    // Use a short timeout to not block startup
    let url = "https://api.github.com/repos/TimAltmann/gitmanager/releases/latest";
    let resp = ureq::get(url)
        .set("User-Agent", "gitmanager")
        .set("Accept", "application/vnd.github.v3+json")
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .ok()?;

    if resp.status() != 200 {
        return None;
    }

    let release: GithubRelease = resp.into_json().ok()?;
    // Ignore drafts and prereleases
    if release.draft.unwrap_or(false) || release.prerelease.unwrap_or(false) {
        return None;
    }

    let latest_raw = release.tag_name;
    let latest_norm = normalize_version(&latest_raw);
    let current_norm = normalize_version(current_version);

    // Parse semver, fallback to string compare if parse fails
    let latest_ver = semver::Version::parse(&latest_norm).ok();
    let current_ver = semver::Version::parse(&current_norm).ok();

    let is_newer = match (latest_ver, current_ver) {
        (Some(l), Some(c)) => l > c,
        _ => latest_norm != current_norm && latest_norm > current_norm,
    };

    if is_newer {
        Some(UpdateInfo {
            latest_version: latest_raw,
            current_version: current_version.to_string(),
            url: release.html_url,
            body: release.body,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_v() {
        assert_eq!(normalize_version("v0.0.4"), "0.0.4");
        assert_eq!(normalize_version("V1.2.3"), "1.2.3");
        assert_eq!(normalize_version("0.1.0"), "0.1.0");
        assert_eq!(normalize_version(" v0.0.4 "), "0.0.4");
    }

    #[test]
    fn version_compare_newer() {
        let current = "0.0.4";
        let latest = "v0.0.5";
        let info = check_for_update(current);
        // This test would require network, so just test normalize and semver logic
        let latest_norm = normalize_version(latest);
        let current_norm = normalize_version(current);
        let l = semver::Version::parse(&latest_norm).unwrap();
        let c = semver::Version::parse(&current_norm).unwrap();
        assert!(l > c);
    }

    #[test]
    fn version_compare_same_no_update() {
        let latest_norm = normalize_version("v0.0.4");
        let current_norm = normalize_version("0.0.4");
        let l = semver::Version::parse(&latest_norm).unwrap();
        let c = semver::Version::parse(&current_norm).unwrap();
        assert!(!(l > c));
    }
}
