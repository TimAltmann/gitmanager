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

pub(crate) fn normalize_version(v: &str) -> String {
    let v = v.trim();
    let v = v
        .strip_prefix('v')
        .or_else(|| v.strip_prefix('V'))
        .unwrap_or(v);
    v.to_string()
}

/// Validiert die lokale Binary-Version rein lokal (kein Netzwerk).
/// Muss VOR jedem Netzwerk-Request laufen, damit eine kaputte
/// CARGO_PKG_VERSION sofort (statt nach 5s-Timeout) als Err kommt.
fn validate_current_version(current_version: &str) -> Result<semver::Version, String> {
    let current_norm = normalize_version(current_version);
    semver::Version::parse(&current_norm).map_err(|e| format!("Binary semver ungültig: {e}"))
}

fn github_request_error(e: ureq::Error) -> String {
    // ureq 2.x liefert non-2xx als Err(Status) – die "Status != 200"-Prüfung
    // unten sieht 4xx/5xx daher nie. Hier mit eigenem, handlungsfähigem Text.
    match e {
        ureq::Error::Status(403, _) => {
            "GitHub API Rate-Limit erreicht (403) – später erneut versuchen".to_string()
        }
        ureq::Error::Status(code, _) => {
            format!("GitHub-Request fehlgeschlagen: HTTP {code}")
        }
        other => format!("GitHub-Request fehlgeschlagen: {other}"),
    }
}

pub fn check_for_update_result(current_version: &str) -> Result<Option<UpdateInfo>, String> {
    // Use a short timeout to not block startup
    let url = "https://api.github.com/repos/TimAltmann/gitmanager/releases/latest";
    // Lokale Version zuerst validieren: spart bis zu 5s Timeout bei kaputter Version.
    let current_ver = validate_current_version(current_version)?;
    let resp = ureq::get(url)
        .set("User-Agent", "gitmanager")
        .set("Accept", "application/vnd.github.v3+json")
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .map_err(github_request_error)?;

    // Nur für 2xx-non-200 / ungefolgte 3xx erreichbar (4xx/5xx kommen als
    // Err(Status) aus call() und landen in github_request_error).
    if resp.status() != 200 {
        return Err(format!("GitHub-Status {} (erwartet 200)", resp.status()));
    }

    let release: GithubRelease = resp
        .into_json()
        .map_err(|e| format!("Release-JSON ungültig: {e}"))?;
    // Ignore drafts and prereleases
    if release.draft.unwrap_or(false) || release.prerelease.unwrap_or(false) {
        return Ok(None);
    }

    let latest_raw = release.tag_name;
    let latest_norm = normalize_version(&latest_raw);

    // Parse semver; bei Parse-Fehler kein Update (nicht lexikalisch raten)
    let latest_ver =
        semver::Version::parse(&latest_norm).map_err(|e| format!("Tag semver ungültig: {e}"))?;

    if latest_ver > current_ver {
        Ok(Some(UpdateInfo {
            latest_version: latest_raw,
            current_version: current_version.to_string(),
            url: release.html_url,
            body: release.body,
        }))
    } else {
        Ok(None)
    }
}

#[allow(dead_code)]
pub fn check_for_update(current_version: &str) -> Option<UpdateInfo> {
    check_for_update_result(current_version).ok().flatten()
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
        // Kein Netzwerk im Unit-Test: nur lokale normalize + semver-Logik prüfen
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

    #[test]
    fn result_returns_error_on_invalid_current_semver() {
        // M2: Fehler müssen als Err mit Kontext kommen, nicht still None.
        // Ungültige Version muss SOFORT (ohne Netzwerk) als "Binary semver"-Err kommen.
        // validate_current_version ist rein lokal und damit netzunabhängig testbar:
        let err = validate_current_version("not-a-version").unwrap_err();
        assert!(
            err.contains("Binary semver"),
            "expected Binary-semver error, got: {err}"
        );
        assert!(validate_current_version("0.1.1").is_ok());
        assert!(validate_current_version("v0.0.4").is_ok());
        // Und der Wrapper muss denselben Fehler liefern, ohne je auf Netzwerk zu warten:
        let err2 = check_for_update_result("not-a-version").unwrap_err();
        assert!(
            err2.contains("Binary semver"),
            "expected early Binary-semver error, got: {err2}"
        );
    }
}
