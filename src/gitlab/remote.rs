//! Git-Remotes parsen und ihren Hosts glab-Hosts zuordnen (E16). Rein, ohne
//! git2 und ohne Prozesse; `git::get_repo_info` füllt `RepoInfo.remotes`.

use crate::config::MrConfig;
use crate::git::RepoInfo;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteScheme {
    Http,
    Https,
    Ssh,
}

/// Geparste Remote-URL, ohne Userinfo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRef {
    pub scheme: RemoteScheme,
    /// Hostname, kleingeschrieben, ohne Port (IPv6 mit Klammern)
    pub host: String,
    pub port: Option<u16>,
    /// Projektpfad `gruppe/untergruppe/projekt`, kleingeschrieben, ohne `.git`
    pub full_path: String,
}

impl RemoteRef {
    /// Host-Schlüssel wie bei glab: HTTP(S) `host` bzw. `host:port`
    /// (Standard-Port entfällt), SSH immer ohne Port – der SSH-Port sagt
    /// nichts über den Web-Host.
    pub fn host_key(&self) -> String {
        match (self.scheme, self.port) {
            (RemoteScheme::Http, Some(p)) if p != 80 => format!("{}:{p}", self.host),
            (RemoteScheme::Https, Some(p)) if p != 443 => format!("{}:{p}", self.host),
            _ => self.host.clone(),
        }
    }
}

/// Ein Remote eines Repos (`RepoInfo.remotes`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    /// URL ohne Zugangsdaten (Anzeige, „Neuer Klon …“)
    pub url_redacted: String,
    /// `None` bei lokalen Pfaden, `file://`, `git://` und unlesbaren URLs
    pub parsed: Option<RemoteRef>,
}

impl RemoteInfo {
    /// Die rohe URL wird sofort bereinigt und nicht gespeichert.
    pub fn new(name: &str, url: &str) -> Self {
        Self {
            name: name.to_string(),
            url_redacted: redact_remote_url(url),
            parsed: parse_remote_url(url),
        }
    }
}

/// Parst `http(s)://[userinfo@]host[:port]/pfad`, `ssh://[user@]host[:port]/pfad`
/// und die scp-Form `[user@]host:pfad`. Host und Pfad kleingeschrieben,
/// `.git` und Slashes am Rand entfernt; der Pfad braucht mindestens
/// `gruppe/projekt`. `file://`, `git://`, lokale Pfade und Müll → `None`.
pub fn parse_remote_url(url: &str) -> Option<RemoteRef> {
    let url = url.trim();
    let (scheme, authority, path) = match url.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = match scheme.to_ascii_lowercase().as_str() {
                "https" => RemoteScheme::Https,
                "http" => RemoteScheme::Http,
                "ssh" | "git+ssh" | "ssh+git" => RemoteScheme::Ssh,
                // file://, git:// und Unbekanntes
                _ => return None,
            };
            let (authority, path) = rest.split_once('/')?;
            (scheme, authority, path)
        }
        None => {
            // scp-Form: Host endet wie bei git am ersten `:`; `/` oder `\`
            // davor bzw. ein Laufwerksbuchstabe heißt lokaler Pfad.
            let (authority, path) = url.split_once(':')?;
            let drive = authority.len() == 1 && authority.as_bytes()[0].is_ascii_alphabetic();
            if drive || authority.contains(['/', '\\']) {
                return None;
            }
            (RemoteScheme::Ssh, authority, path)
        }
    };
    // Userinfo bis zum letzten `@` verwerfen (kann ein Token sein)
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let (host, port) = split_host_port(host_port)?;
    Some(RemoteRef {
        scheme,
        host,
        port,
        full_path: normalize_project_path(path)?,
    })
}

/// `host[:port]` bzw. `[ipv6][:port]`; leerer Port zählt als keiner.
fn split_host_port(s: &str) -> Option<(String, Option<u16>)> {
    let (host, port) = match s.strip_prefix('[') {
        Some(rest) => {
            let (inner, after) = rest.split_once(']')?;
            let valid = !inner.is_empty()
                && inner
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b':' || b == b'.');
            if !valid {
                return None;
            }
            let port = match after {
                "" => None,
                p => Some(p.strip_prefix(':')?),
            };
            (format!("[{inner}]"), port)
        }
        None => {
            let (host, port) = match s.rsplit_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (s, None),
            };
            let valid = !host.is_empty()
                && host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b));
            if !valid {
                return None;
            }
            (host.to_string(), port)
        }
    };
    let port = match port {
        None | Some("") => None,
        // nur Ziffern (`u16::from_str` nähme auch `+80`)
        Some(p) if p.bytes().all(|b| b.is_ascii_digit()) => Some(p.parse().ok()?),
        Some(_) => return None,
    };
    Some((host.to_lowercase(), port))
}

/// Projektpfad nach den GitLab-Regeln (Buchstaben, Ziffern, `_-.`),
/// mindestens `gruppe/projekt`.
fn normalize_project_path(path: &str) -> Option<String> {
    let path = path.to_lowercase();
    let path = path.trim_matches('/');
    let path = path
        .strip_suffix(".git")
        .unwrap_or(path)
        .trim_end_matches('/');
    let mut segments = path.split('/');
    let valid = path.contains('/')
        && segments.all(|seg| {
            !seg.is_empty()
                && seg != "."
                && seg != ".."
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        });
    valid.then(|| path.to_string())
}

/// Entfernt Zugangsdaten aus einer Remote-URL: bei HTTP(S) die ganze
/// Userinfo (der Benutzername kann selbst ein Token sein), bei SSH nur ein
/// Passwort – der SSH-Benutzer (`git@`) wird zum Klonen gebraucht. Ein
/// Passwort kann in `.git/config` unkodiert `/` oder `@` enthalten (für git
/// ungültig, aber nie anzeigen); die Userinfo reicht deshalb bis zum
/// letzten `@` vor dem Host. GitLab-Pfade enthalten kein `@`.
pub fn redact_remote_url(url: &str) -> String {
    let url = url.trim();
    if let Some((scheme, rest)) = url.split_once("://") {
        // Im Zweifel mehr entfernen: letztes `@` der ganzen URL, nicht das
        // letzte vor dem ersten `/`.
        let Some((userinfo, host_tail)) = rest.rsplit_once('@') else {
            return url.to_string();
        };
        let user = userinfo.split(':').next().unwrap_or_default();
        return if scheme.to_ascii_lowercase().contains("ssh") && !user.is_empty() {
            format!("{scheme}://{user}@{host_tail}")
        } else {
            format!("{scheme}://{host_tail}")
        };
    }
    // scp-Form `user:passwort@host:pfad` (für git ungültig, aber nie
    // anzeigen); lokale Pfade wie in `parse_remote_url` unverändert.
    let scp = url.split_once(':').is_some_and(|(authority, _)| {
        let drive = authority.len() == 1 && authority.as_bytes()[0].is_ascii_alphabetic();
        !drive && !authority.contains(['/', '\\'])
    });
    let at = url
        .match_indices('@')
        .map(|(i, _)| i)
        .rev()
        .find(|&i| starts_with_scp_host(&url[i + 1..]));
    match at.filter(|_| scp).map(|i| (&url[..i], &url[i + 1..])) {
        Some((userinfo, rest)) if userinfo.contains(':') => {
            match userinfo.split(':').next().unwrap_or_default() {
                "" => rest.to_string(),
                user => format!("{user}@{rest}"),
            }
        }
        _ => url.to_string(),
    }
}

/// Beginnt `s` mit `host:` (scp-Form, Host auch `[ipv6]`)?
fn starts_with_scp_host(s: &str) -> bool {
    let end = match s.strip_prefix('[') {
        Some(inner) => inner.find(']').map(|i| i + 2),
        None => s.find(':'),
    };
    end.is_some_and(|e| s[e..].starts_with(':') && split_host_port(&s[..e]).is_some())
}

/// Lokale glab-Erkennung für einen Host-Kandidaten (`glab config get`, E16).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlabHost {
    /// glab-Host-Schlüssel (`host` oder `host:port`)
    pub key: String,
    /// glab-Benutzer; leer = nicht angemeldet
    pub user: String,
    /// Abweichender SSH-Host aus der glab-Config
    pub ssh_host: Option<String>,
    /// `api_protocol` aus der glab-Config (`http`/`https`), falls gesetzt
    pub api_protocol: Option<String>,
}

/// Zuordnung eines Remote-Hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostTarget {
    /// glab-Host-Schlüssel, an den Netz-Aufrufe gehen
    GitLab(String),
    /// Weder bei glab angemeldet noch als GitLab-Host eingetragen (z. B. github.com)
    NotGitLab,
}

/// Ergebnis von `resolve_hosts`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostMap {
    /// Remote-Host-Schlüssel (`RemoteRef::host_key`) → Zuordnung
    pub remotes: BTreeMap<String, HostTarget>,
    /// GitLab-Hosts, an die Netz-Aufrufe gehen dürfen: zugeordnete
    /// glab-Hosts ∪ `extra_hosts` (ohne überflüssige), sortiert
    pub gitlab_hosts: Vec<String>,
    /// `extra_hosts`-Einträge, die schon ein angemeldeter glab-Host abdeckt
    /// (über `ssh_host` oder „gehört zu …“) → abdeckender Host. Bekommen
    /// keine Netz-Aufrufe; die Einstellungen bieten das Entfernen an.
    pub redundant_extra: BTreeMap<String, String>,
}

impl HostMap {
    /// glab-Host-Schlüssel für einen Remote, `None` = nicht GitLab.
    pub fn glab_key(&self, remote: &RemoteRef) -> Option<&str> {
        match self.remotes.get(&remote.host_key())? {
            HostTarget::GitLab(key) => Some(key),
            HostTarget::NotGitLab => None,
        }
    }
}

/// Ordnet jedem Remote-Host einen glab-Host zu (E16): exakter Treffer auf
/// einen angemeldeten Host, dann `ssh_host` eines angemeldeten Hosts, dann
/// `host_aliases` („gehört zu …“), zuletzt `extra_hosts`. Eine glab-Anmeldung
/// gewinnt so immer gegen einen von Hand eingetragenen Host. `glab_hosts`
/// darf alle Kandidaten enthalten; angemeldet ist nur, wer einen `user` hat
/// (sonst gingen Netz-Aufrufe z. B. an github.com). Rein, kein Netz.
pub fn resolve_hosts<'a>(
    remotes: impl IntoIterator<Item = &'a RemoteRef>,
    glab_hosts: &[GlabHost],
    cfg: &MrConfig,
) -> HostMap {
    // Angemeldete Hosts (Schlüssel, ssh_host als reiner Hostname), sortiert
    // → bei gleichem ssh_host gewinnt deterministisch der erste Schlüssel.
    let mut signed_in: Vec<(String, Option<String>)> = glab_hosts
        .iter()
        .filter(|h| !h.user.trim().is_empty())
        .map(|h| {
            let ssh = h.ssh_host.as_deref().map(norm_ssh_host);
            (norm_host(&h.key), ssh.filter(|s| !s.is_empty()))
        })
        .filter(|(key, _)| !key.is_empty())
        .collect();
    signed_in.sort();
    let hosts = Hosts {
        signed: signed_in.iter().map(|(key, _)| key.clone()).collect(),
        extra: cfg
            .extra_hosts
            .iter()
            .map(|h| norm_host(h))
            .filter(|h| !h.is_empty())
            .collect(),
        signed_in,
        aliases: cfg
            .host_aliases
            .iter()
            .map(|a| (norm_host(&a.from), norm_host(&a.to)))
            .chain(
                BUILTIN_ALIASES
                    .iter()
                    .map(|(from, to)| (from.to_string(), to.to_string())),
            )
            .collect(),
    };
    let mut map = HostMap::default();
    for remote in remotes {
        if let Entry::Vacant(slot) = map.remotes.entry(remote.host_key()) {
            let target = resolve_one(slot.key(), &remote.host, &hosts);
            slot.insert(target);
        }
    }
    // Eingetragene Hosts, die eine Anmeldung schon abdeckt, sind überflüssig
    // (kein Netz-Aufruf an z. B. die SSH-IP eines angemeldeten Hosts).
    for extra in hosts.extra.iter().filter(|e| !hosts.signed.contains(*e)) {
        if let HostTarget::GitLab(key) = resolve_one(extra, strip_port(extra), &hosts) {
            if key != *extra {
                map.redundant_extra.insert(extra.clone(), key);
            }
        }
    }
    let mut network: BTreeSet<String> = hosts
        .extra
        .iter()
        .filter(|e| !map.redundant_extra.contains_key(*e))
        .cloned()
        .collect();
    network.extend(map.remotes.values().filter_map(|t| match t {
        HostTarget::GitLab(key) => Some(key.clone()),
        HostTarget::NotGitLab => None,
    }));
    map.gitlab_hosts = network.into_iter().collect();
    map
}

/// Normalisierte Eingaben für `resolve_one`.
struct Hosts {
    /// Angemeldete glab-Hosts
    signed: BTreeSet<String>,
    /// Von Hand eingetragene GitLab-Hosts (`extra_hosts`)
    extra: BTreeSet<String>,
    /// Angemeldete Hosts mit ihrem `ssh_host`
    signed_in: Vec<(String, Option<String>)>,
    /// „gehört zu …“ und eingebaute Zuordnungen (von → nach)
    aliases: Vec<(String, String)>,
}

/// GitManager-eigene Zuordnung (nicht in glab): GitLab.com-SSH über Port 443.
const BUILTIN_ALIASES: &[(&str, &str)] = &[("altssh.gitlab.com", "gitlab.com")];

/// Ziel der eingebauten Zuordnung für einen Remote-Hostnamen (z. B.
/// `altssh.gitlab.com` → `gitlab.com`); Kandidat der lokalen Erkennung (E16),
/// denn `resolve_hosts` ordnet nur angemeldeten Zielen zu.
pub fn builtin_alias_target(host: &str) -> Option<&'static str> {
    let host = norm_host(host);
    BUILTIN_ALIASES
        .iter()
        .find(|(from, _)| *from == host)
        .map(|(_, to)| *to)
}

/// Zuordnung eines Remote-Hosts (`key` = `host` bzw. `host:port`, `host` =
/// Hostname ohne Port).
fn resolve_one(key: &str, host: &str, hosts: &Hosts) -> HostTarget {
    // 1. Exakter Treffer auf einen angemeldeten Host (`host` bzw. `host:port`)
    if hosts.signed.contains(key) {
        return HostTarget::GitLab(key.to_string());
    }
    // 2. ssh_host eines angemeldeten Hosts (nur Hostname, wie glab)
    if let Some((glab_key, _)) = hosts
        .signed_in
        .iter()
        .find(|(_, ssh)| ssh.as_deref() == Some(host))
    {
        return HostTarget::GitLab(glab_key.clone());
    }
    // 3. „gehört zu …“: erst `host:port`, dann der Hostname für alle Ports;
    // das Ziel muss selbst ein GitLab-Host sein (sonst kein Netz-Aufruf).
    let alias = hosts
        .aliases
        .iter()
        .find(|(from, _)| from == key)
        .or_else(|| hosts.aliases.iter().find(|(from, _)| from == host));
    if let Some((_, to)) = alias {
        if hosts.signed.contains(to) || hosts.extra.contains(to) {
            return HostTarget::GitLab(to.clone());
        }
    }
    // 4. Von Hand eingetragener GitLab-Host (ohne glab-Anmeldung)
    if hosts.extra.contains(key) {
        return HostTarget::GitLab(key.to_string());
    }
    HostTarget::NotGitLab
}

/// Host-Schlüssel zum Vergleichen: ohne Leerraum am Rand, kleingeschrieben.
pub(super) fn norm_host(host: &str) -> String {
    host.trim().to_lowercase()
}

/// `ssh_host` aus der glab-Config als reiner Hostname: glab speichert ihn
/// auch mit Benutzer, Port oder Schema (`git@192.0.2.10:2222`,
/// `ssh://git@host:22/`).
pub(super) fn norm_ssh_host(raw: &str) -> String {
    let s = norm_host(raw);
    let s = s.strip_prefix("ssh://").unwrap_or(&s);
    let s = s.rsplit_once('@').map_or(s, |(_, host)| host);
    let s = s.split('/').next().unwrap_or_default();
    strip_port(s).to_string()
}

/// Hostname ohne `:port` (`[ipv6]` bleibt ganz).
pub(super) fn strip_port(host: &str) -> &str {
    match host.rsplit_once(':') {
        Some((h, p)) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => h,
        _ => host,
    }
}

/// GitLab-Projekt mit mindestens einem lokalen Klon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProject {
    /// glab-Host-Schlüssel
    pub host_key: String,
    /// Projektpfad, kleingeschrieben
    pub full_path: String,
    /// Klone mit einem Remote auf das Projekt, in Scan-Reihenfolge
    pub repos: Vec<PathBuf>,
    /// Nicht in `excluded_projects` abgewählt (E1/E2)
    pub included: bool,
}

/// Schlüssel für `excluded_projects` (E2): `host/full/path`, kleingeschrieben.
pub fn project_key(host_key: &str, full_path: &str) -> String {
    format!("{host_key}/{full_path}").to_lowercase()
}

/// Fasst die Remotes der gescannten Repos zu GitLab-Projekten zusammen:
/// mehrere Klone desselben Projekts zählen einmal (E2), Remotes ohne
/// GitLab-Host (`HostTarget::NotGitLab`) fallen weg. „Host nicht
/// angemeldet“ in der Repo-Liste (Task 12) betrifft damit nur `extra_hosts`
/// ohne glab-Anmeldung; nicht angemeldete Remote-Hosts (github.com, noch
/// nicht angemeldetes GitLab) zeigt die eigene Liste „Nicht bei glab
/// angemeldete Remote-Hosts“ aus `HostMap.remotes` und `RepoInfo.remotes`.
/// Sortiert nach Host und Pfad.
pub fn local_projects(repos: &[RepoInfo], hosts: &HostMap, cfg: &MrConfig) -> Vec<LocalProject> {
    let excluded: HashSet<String> = cfg
        .excluded_projects
        .iter()
        .map(|k| k.trim().to_lowercase())
        .collect();
    let mut projects: BTreeMap<(String, String), Vec<PathBuf>> = BTreeMap::new();
    for repo in repos {
        for remote in repo.remotes.iter().filter_map(|r| r.parsed.as_ref()) {
            let Some(host_key) = hosts.glab_key(remote) else {
                continue;
            };
            let paths = projects
                .entry((host_key.to_string(), remote.full_path.clone()))
                .or_default();
            // mehrere Remotes eines Klons auf dasselbe Projekt zählen einmal
            if !paths.contains(&repo.path) {
                paths.push(repo.path.clone());
            }
        }
    }
    projects
        .into_iter()
        .map(|((host_key, full_path), repos)| LocalProject {
            included: !excluded.contains(&project_key(&host_key, &full_path)),
            host_key,
            full_path,
            repos,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HostAlias;
    use RemoteScheme::{Http, Https, Ssh};

    fn rr(scheme: RemoteScheme, host: &str, port: Option<u16>, path: &str) -> RemoteRef {
        RemoteRef {
            scheme,
            host: host.to_string(),
            port,
            full_path: path.to_string(),
        }
    }

    fn parsed(url: &str) -> RemoteRef {
        parse_remote_url(url).unwrap_or_else(|| panic!("{url} muss parsebar sein"))
    }

    /// Angemeldeter glab-Host
    fn gh(key: &str, ssh_host: Option<&str>) -> GlabHost {
        GlabHost {
            key: key.to_string(),
            user: "tim".to_string(),
            ssh_host: ssh_host.map(str::to_string),
            api_protocol: None,
        }
    }

    fn target(map: &HostMap, host_key: &str) -> HostTarget {
        map.remotes
            .get(host_key)
            .cloned()
            .unwrap_or_else(|| panic!("{host_key} fehlt in {map:?}"))
    }

    fn gitlab(key: &str) -> HostTarget {
        HostTarget::GitLab(key.to_string())
    }

    // --- parse_remote_url ---

    #[test]
    fn parse_remote_url_table() {
        let cases: &[(&str, RemoteRef)] = &[
            (
                "http://gitlab.local/a/b.git",
                rr(Http, "gitlab.local", None, "a/b"),
            ),
            (
                "ssh://git@192.0.2.10:2222/a/b.git",
                rr(Ssh, "192.0.2.10", Some(2222), "a/b"),
            ),
            ("https://h:8443/a/b.git", rr(Https, "h", Some(8443), "a/b")),
            (
                "git@gitlab.example.com:g/sub/p.git",
                rr(Ssh, "gitlab.example.com", None, "g/sub/p"),
            ),
            // Userinfo mit Token wird verworfen
            (
                "https://oauth2:glpat-SECRET123@gitlab.example.com/kern/backend-api.git",
                rr(Https, "gitlab.example.com", None, "kern/backend-api"),
            ),
            (
                "https://glpat-SECRET123@gitlab.example.com/kern/backend-api.git",
                rr(Https, "gitlab.example.com", None, "kern/backend-api"),
            ),
            // Kleinschreibung, Slash am Ende, ohne `.git`
            (
                "https://Tim@GitLab.Example.com/Kern/Backend-API/",
                rr(Https, "gitlab.example.com", None, "kern/backend-api"),
            ),
            (
                "https://gitlab.example.com/g/p.git/",
                rr(Https, "gitlab.example.com", None, "g/p"),
            ),
            (
                "ssh://git@gitlab.example.com/g/sub/deeper/p.git",
                rr(Ssh, "gitlab.example.com", None, "g/sub/deeper/p"),
            ),
            (
                "git+ssh://git@gitlab.example.com/g/p.git",
                rr(Ssh, "gitlab.example.com", None, "g/p"),
            ),
            // scp-Form ohne Benutzer und mit führendem Slash
            (
                "gitlab.example.com:g/p.git",
                rr(Ssh, "gitlab.example.com", None, "g/p"),
            ),
            (
                "git@gitlab.example.com:/g/p.git",
                rr(Ssh, "gitlab.example.com", None, "g/p"),
            ),
            ("  https://h/a/b.git\n", rr(Https, "h", None, "a/b")),
            ("HTTPS://h/a/b", rr(Https, "h", None, "a/b")),
            // leerer Port wie bei `ssh://host:/pfad`
            ("ssh://git@h:/a/b.git", rr(Ssh, "h", None, "a/b")),
            (
                "https://[::1]:8443/a/b.git",
                rr(Https, "[::1]", Some(8443), "a/b"),
            ),
            (
                "git@my_host.intern:a/b.c-d_e.git",
                rr(Ssh, "my_host.intern", None, "a/b.c-d_e"),
            ),
        ];
        for (url, want) in cases {
            assert_eq!(parse_remote_url(url).as_ref(), Some(want), "{url}");
        }
    }

    #[test]
    fn parse_remote_url_rejects_local_and_garbage() {
        for url in [
            "",
            "   ",
            "Müll",
            "not a url",
            "file:///srv/git/a/b.git",
            "git://gitlab.example.com/a/b.git",
            "ftp://h/a/b.git",
            "/srv/git/a/b.git",
            "../a/b.git",
            "./b.git",
            "C:\\repos\\a\\b.git",
            "C:/repos/a/b.git",
            "\\\\server\\share\\a\\b.git",
            // nur ein Pfadsegment (GitLab braucht Gruppe/Projekt)
            "https://h/onlyone.git",
            "git@h:onlyone.git",
            "https://h:notaport/a/b",
            "https://h:70000/a/b",
            "https:///a/b",
            "https://h",
            "ssh://git@h",
            "git@h:",
            "git@:a/b",
            "https://h/a b/c",
            "https://h/a//b",
            "https://h/a/../b",
            "https://h/a/b?x=1",
            "https://h/a/b#frag",
            "https://h ost/a/b",
        ] {
            assert_eq!(parse_remote_url(url), None, "{url:?}");
        }
    }

    #[test]
    fn host_key_like_glab() {
        assert_eq!(parsed("https://h:8443/a/b.git").host_key(), "h:8443");
        assert_eq!(parsed("https://H/a/b.git").host_key(), "h");
        // Standard-Ports entfallen
        assert_eq!(parsed("https://h:443/a/b.git").host_key(), "h");
        assert_eq!(parsed("http://h:80/a/b.git").host_key(), "h");
        assert_eq!(parsed("http://h:8080/a/b.git").host_key(), "h:8080");
        // SSH immer ohne Port
        assert_eq!(
            parsed("ssh://git@192.0.2.10:2222/a/b.git").host_key(),
            "192.0.2.10"
        );
        assert_eq!(
            parsed("git@gitlab.example.com:g/p.git").host_key(),
            "gitlab.example.com"
        );
        assert_eq!(parsed("https://[::1]:8443/a/b").host_key(), "[::1]:8443");
    }

    #[test]
    fn redact_remote_url_strips_credentials() {
        let cases = [
            (
                "https://oauth2:glpat-SECRET123@gitlab.example.com/kern/backend-api.git",
                "https://gitlab.example.com/kern/backend-api.git",
            ),
            ("https://glpat-SECRET123@h/a/b", "https://h/a/b"),
            (
                "http://user:pw@gitlab.local:8080/a/b.git",
                "http://gitlab.local:8080/a/b.git",
            ),
            // SSH: Benutzer bleibt (zum Klonen nötig), Passwort fällt weg
            (
                "ssh://git:SECRET123@h:2222/a/b.git",
                "ssh://git@h:2222/a/b.git",
            ),
            ("ssh://git@h:2222/a/b.git", "ssh://git@h:2222/a/b.git"),
            (
                "git@gitlab.example.com:g/p.git",
                "git@gitlab.example.com:g/p.git",
            ),
            ("https://h/a/b.git", "https://h/a/b.git"),
            ("/srv/git/a.git", "/srv/git/a.git"),
            ("C:\\repos\\a.git", "C:\\repos\\a.git"),
            ("  https://u:SECRET123@h/a/b  ", "https://h/a/b"),
            // Passwort mit unkodiertem `/` oder `@` (für git ungültig)
            (
                "https://tim:pa/SECRET@gitlab.local/g/p.git",
                "https://gitlab.local/g/p.git",
            ),
            ("https://tim:p@SECRET@h/a/b", "https://h/a/b"),
            ("ssh://git:pa/SECRET@h:2222/a/b", "ssh://git@h:2222/a/b"),
            // scp-Form
            ("user:pw-SECRET@host:g/p.git", "user@host:g/p.git"),
            ("user:p@SECRET@host:g/p", "user@host:g/p"),
            ("user:pa/SECRET@host:g/p", "user@host:g/p"),
            ("user:SECRET@[::1]:g/p", "user@[::1]:g/p"),
            (":SECRET@host:g/p", "host:g/p"),
            // `@` im Pfad gehört nicht zur Userinfo
            ("user:SECRET@host:g/p@x", "user@host:g/p@x"),
            ("git@host:a/b@c", "git@host:a/b@c"),
            // lokale Pfade mit `@` und `:` bleiben
            ("/srv/a:b@h:c", "/srv/a:b@h:c"),
            ("C:\\x:y@h:b", "C:\\x:y@h:b"),
        ];
        for (url, want) in cases {
            assert_eq!(redact_remote_url(url), want, "{url}");
        }
    }

    #[test]
    fn remote_info_never_keeps_token() {
        let info = RemoteInfo::new(
            "origin",
            "https://oauth2:glpat-SECRET123@gitlab.example.com/kern/backend-api.git",
        );
        assert_eq!(info.name, "origin");
        assert_eq!(
            info.url_redacted,
            "https://gitlab.example.com/kern/backend-api.git"
        );
        assert_eq!(
            info.parsed,
            Some(rr(Https, "gitlab.example.com", None, "kern/backend-api"))
        );
        assert!(!format!("{info:?}").contains("SECRET123"));
        assert!(!format!("{info:?}").contains("oauth2"));
        // Lokaler Pfad: kein GitLab-Remote, URL unverändert
        let local = RemoteInfo::new("backup", "/srv/git/a.git");
        assert_eq!(local.parsed, None);
        assert_eq!(local.url_redacted, "/srv/git/a.git");
    }

    // --- resolve_hosts ---

    #[test]
    fn resolve_hosts_exact_match_with_and_without_port() {
        let remotes = [
            parsed("https://gitlab.example.com/kern/backend-api.git"),
            parsed("git@gitlab.example.com:web/kundenportal.git"),
            parsed("https://h:8443/a/b.git"),
            // `h` ohne Port ist ein anderer Host als `h:8443`
            parsed("https://h/c/d.git"),
        ];
        let glab = [gh("GitLab.Example.com", None), gh("h:8443", None)];
        let map = resolve_hosts(&remotes, &glab, &MrConfig::default());
        assert_eq!(
            target(&map, "gitlab.example.com"),
            gitlab("gitlab.example.com")
        );
        assert_eq!(target(&map, "h:8443"), gitlab("h:8443"));
        assert_eq!(target(&map, "h"), HostTarget::NotGitLab);
        assert_eq!(map.gitlab_hosts, vec!["gitlab.example.com", "h:8443"]);
        assert_eq!(map.glab_key(&remotes[1]), Some("gitlab.example.com"));
        assert_eq!(map.glab_key(&remotes[3]), None);
        // Unbekannter Remote (nicht in der Zuordnung) → nicht GitLab
        assert_eq!(map.glab_key(&parsed("https://other/a/b")), None);
    }

    #[test]
    fn resolve_hosts_via_ssh_host() {
        let remotes = [
            parsed("ssh://git@ssh.example.com:2222/kern/backend-api.git"),
            parsed("git@SSH2.example.com:web/kundenportal.git"),
        ];
        let glab = [
            gh("gitlab.example.com", Some("SSH.example.com")),
            // ssh_host mit Port: verglichen wird nur der Hostname
            gh("git.intern.example", Some("ssh2.example.com:2222")),
        ];
        let map = resolve_hosts(&remotes, &glab, &MrConfig::default());
        assert_eq!(
            target(&map, "ssh.example.com"),
            gitlab("gitlab.example.com")
        );
        assert_eq!(
            target(&map, "ssh2.example.com"),
            gitlab("git.intern.example")
        );
        assert_eq!(
            map.gitlab_hosts,
            vec!["git.intern.example", "gitlab.example.com"]
        );
    }

    #[test]
    fn resolve_hosts_ssh_host_with_user_and_port() {
        // glab speichert `ssh_host` auch mit Benutzer und Port, z. B.
        // `git@10.0.0.5:2222`; die Remotes zeigen per SSH auf die IP.
        let remotes = [
            parsed("ssh://git@10.0.0.5:2222/kern/backend-api.git"),
            parsed("git@ssh.example.com:web/kundenportal.git"),
        ];
        let glab = [
            gh("gitlab.example.com", Some("git@10.0.0.5:2222")),
            gh("git.intern.example", Some("ssh://git@SSH.example.com:22/")),
        ];
        let map = resolve_hosts(&remotes, &glab, &MrConfig::default());
        assert_eq!(target(&map, "10.0.0.5"), gitlab("gitlab.example.com"));
        assert_eq!(
            target(&map, "ssh.example.com"),
            gitlab("git.intern.example")
        );
        assert_eq!(
            map.gitlab_hosts,
            vec!["git.intern.example", "gitlab.example.com"]
        );
    }

    #[test]
    fn resolve_hosts_signed_in_ssh_host_beats_unsigned_extra() {
        // Von Hand als GitLab-Host eingetragene IP, die glab über `ssh_host`
        // einem angemeldeten Host zuordnet: die Anmeldung gewinnt, der
        // Eintrag bekommt keine Netz-Aufrufe und gilt als überflüssig.
        let remotes = [parsed("ssh://git@10.0.0.5:2222/kern/backend-api.git")];
        let glab = [gh("gitlab.example.com", Some("git@10.0.0.5:2222"))];
        let cfg = MrConfig {
            extra_hosts: vec!["10.0.0.5".to_string()],
            ..MrConfig::default()
        };
        let map = resolve_hosts(&remotes, &glab, &cfg);
        assert_eq!(target(&map, "10.0.0.5"), gitlab("gitlab.example.com"));
        assert_eq!(map.gitlab_hosts, vec!["gitlab.example.com"]);
        assert_eq!(
            map.redundant_extra.get("10.0.0.5").map(String::as_str),
            Some("gitlab.example.com")
        );
    }

    #[test]
    fn resolve_hosts_unsigned_extra_stays_without_coverage() {
        // Ohne passende Anmeldung bleibt der eingetragene Host ein
        // GitLab-Host (Karte „Nicht angemeldet“, Token hinzufügen).
        let remotes = [parsed("ssh://git@10.0.0.5:2222/kern/backend-api.git")];
        let cfg = MrConfig {
            extra_hosts: vec!["10.0.0.5".to_string()],
            ..MrConfig::default()
        };
        let map = resolve_hosts(&remotes, &[], &cfg);
        assert_eq!(target(&map, "10.0.0.5"), gitlab("10.0.0.5"));
        assert_eq!(map.gitlab_hosts, vec!["10.0.0.5"]);
        assert!(map.redundant_extra.is_empty());
    }

    #[test]
    fn norm_ssh_host_forms() {
        for (raw, want) in [
            ("git@192.0.2.10:2222", "192.0.2.10"),
            ("SSH.Example.com", "ssh.example.com"),
            ("ssh.example.com:22", "ssh.example.com"),
            ("ssh://git@ssh.example.com:2222/", "ssh.example.com"),
            ("  git@host  ", "host"),
            ("[::1]:22", "[::1]"),
            ("", ""),
        ] {
            assert_eq!(norm_ssh_host(raw), want, "{raw}");
        }
    }

    #[test]
    fn resolve_hosts_exact_beats_ssh_host() {
        // `gitlab.example.com` ist selbst angemeldet und zugleich ssh_host
        // eines anderen Hosts → exakter Treffer gewinnt.
        let remotes = [parsed("git@gitlab.example.com:g/p.git")];
        let glab = [
            gh("a.example.com", Some("gitlab.example.com")),
            gh("gitlab.example.com", None),
        ];
        let map = resolve_hosts(&remotes, &glab, &MrConfig::default());
        assert_eq!(
            target(&map, "gitlab.example.com"),
            gitlab("gitlab.example.com")
        );
        assert_eq!(map.gitlab_hosts, vec!["gitlab.example.com"]);
    }

    #[test]
    fn resolve_hosts_via_host_aliases() {
        let remotes = [
            parsed("ssh://git@192.0.2.10:2222/a/b.git"),
            parsed("git@alias-to-nowhere:a/b.git"),
            parsed("https://web:8443/a/b.git"),
        ];
        let glab = [gh("gitlab.local", None)];
        let cfg = MrConfig {
            host_aliases: vec![
                HostAlias {
                    from: "192.0.2.10".to_string(),
                    to: "GitLab.local".to_string(),
                },
                // Ziel weder angemeldet noch in extra_hosts → kein Netz-Aufruf
                HostAlias {
                    from: "alias-to-nowhere".to_string(),
                    to: "not-signed-in.example".to_string(),
                },
                // Alias auf den Hostnamen gilt für alle Ports
                HostAlias {
                    from: "web".to_string(),
                    to: "gitlab.local".to_string(),
                },
            ],
            ..MrConfig::default()
        };
        let map = resolve_hosts(&remotes, &glab, &cfg);
        assert_eq!(target(&map, "192.0.2.10"), gitlab("gitlab.local"));
        assert_eq!(target(&map, "alias-to-nowhere"), HostTarget::NotGitLab);
        assert_eq!(target(&map, "web:8443"), gitlab("gitlab.local"));
        assert_eq!(map.gitlab_hosts, vec!["gitlab.local"]);
    }

    #[test]
    fn resolve_hosts_alias_with_port_beats_host_alias() {
        let remotes = [parsed("https://web:8443/a/b.git")];
        let glab = [gh("one", None), gh("two", None)];
        let cfg = MrConfig {
            host_aliases: vec![
                HostAlias {
                    from: "web".to_string(),
                    to: "one".to_string(),
                },
                HostAlias {
                    from: "web:8443".to_string(),
                    to: "two".to_string(),
                },
            ],
            ..MrConfig::default()
        };
        let map = resolve_hosts(&remotes, &glab, &cfg);
        assert_eq!(target(&map, "web:8443"), gitlab("two"));
    }

    #[test]
    fn resolve_hosts_extra_hosts_count_as_gitlab() {
        let remotes = [
            parsed("http://gitlab.local/a/b.git"),
            parsed("ssh://git@10.0.0.5:2222/a/b.git"),
        ];
        let cfg = MrConfig {
            // nicht normalisiert (Einstellungs-Entwurf)
            extra_hosts: vec![" GitLab.local ".to_string(), "unused.example".to_string()],
            host_aliases: vec![HostAlias {
                from: "10.0.0.5".to_string(),
                to: "gitlab.local".to_string(),
            }],
            ..MrConfig::default()
        };
        let map = resolve_hosts(&remotes, &[], &cfg);
        assert_eq!(target(&map, "gitlab.local"), gitlab("gitlab.local"));
        assert_eq!(target(&map, "10.0.0.5"), gitlab("gitlab.local"));
        // Ausdrücklich eingetragene Hosts gehören immer dazu (E16 b)
        assert_eq!(map.gitlab_hosts, vec!["gitlab.local", "unused.example"]);
    }

    #[test]
    fn resolve_hosts_github_and_bitbucket_are_not_gitlab() {
        let remotes = [
            parsed("git@github.com:rust-lang/rust.git"),
            parsed("https://github.com/rust-lang/cargo.git"),
            parsed("https://bitbucket.example.com/scm/proj/repo.git"),
            parsed("ssh://git@bitbucket.example.com:7999/proj/repo.git"),
            parsed("https://gitlab.example.com/kern/backend-api.git"),
        ];
        let glab = [gh("gitlab.example.com", Some("ssh.gitlab.example.com"))];
        let map = resolve_hosts(&remotes, &glab, &MrConfig::default());
        assert_eq!(target(&map, "github.com"), HostTarget::NotGitLab);
        assert_eq!(target(&map, "bitbucket.example.com"), HostTarget::NotGitLab);
        assert_eq!(
            target(&map, "gitlab.example.com"),
            gitlab("gitlab.example.com")
        );
        assert_eq!(map.remotes.len(), 3, "ein Eintrag je Remote-Host: {map:?}");
        assert_eq!(map.gitlab_hosts, vec!["gitlab.example.com"]);
    }

    #[test]
    fn resolve_hosts_without_glab_hosts_is_empty() {
        let remotes = [
            parsed("https://gitlab.example.com/kern/backend-api.git"),
            parsed("git@github.com:rust-lang/rust.git"),
        ];
        let map = resolve_hosts(&remotes, &[], &MrConfig::default());
        assert!(map.gitlab_hosts.is_empty());
        assert!(map.remotes.values().all(|t| *t == HostTarget::NotGitLab));
        assert_eq!(map.glab_key(&remotes[0]), None);
        let empty = resolve_hosts(std::iter::empty(), &[], &MrConfig::default());
        assert_eq!(empty, HostMap::default());
    }

    #[test]
    fn resolve_hosts_ignores_candidates_without_user() {
        // `discover_glab_hosts` liefert jeden Kandidaten; ohne `user` ist
        // er nicht angemeldet und bekommt keinen Netz-Aufruf (E16).
        let remotes = [
            parsed("https://github.com/rust-lang/cargo.git"),
            parsed("git@ssh.example.com:g/p.git"),
            parsed("ssh://git@10.0.0.5:2222/a/b.git"),
        ];
        let glab = [
            GlabHost {
                key: "github.com".to_string(),
                ..GlabHost::default()
            },
            GlabHost {
                key: "gitlab.example.com".to_string(),
                user: "  ".to_string(),
                ssh_host: Some("ssh.example.com".to_string()),
                api_protocol: None,
            },
        ];
        let cfg = MrConfig {
            host_aliases: vec![HostAlias {
                from: "10.0.0.5".to_string(),
                to: "gitlab.example.com".to_string(),
            }],
            ..MrConfig::default()
        };
        let map = resolve_hosts(&remotes, &glab, &cfg);
        assert!(map.remotes.values().all(|t| *t == HostTarget::NotGitLab));
        assert!(map.gitlab_hosts.is_empty());
    }

    #[test]
    fn resolve_hosts_altssh_gitlab_com() {
        // GitLab.com-SSH über Port 443 (altssh) gehört zu gitlab.com
        let remotes = [parsed("ssh://git@altssh.gitlab.com:443/g/p.git")];
        let map = resolve_hosts(&remotes, &[gh("gitlab.com", None)], &MrConfig::default());
        assert_eq!(target(&map, "altssh.gitlab.com"), gitlab("gitlab.com"));
        let map = resolve_hosts(&remotes, &[], &MrConfig::default());
        assert_eq!(target(&map, "altssh.gitlab.com"), HostTarget::NotGitLab);
    }

    // --- local_projects ---

    fn repo(path: &str, remotes: &[(&str, &str)]) -> RepoInfo {
        let mut info = RepoInfo::new(PathBuf::from(path), "main".to_string(), false, false);
        info.remotes = remotes
            .iter()
            .map(|(name, url)| RemoteInfo::new(name, url))
            .collect();
        info
    }

    fn host_map_for(repos: &[RepoInfo], glab: &[GlabHost], cfg: &MrConfig) -> HostMap {
        resolve_hosts(
            repos
                .iter()
                .flat_map(|r| r.remotes.iter().filter_map(|x| x.parsed.as_ref())),
            glab,
            cfg,
        )
    }

    #[test]
    fn local_projects_merges_clones() {
        let repos = [
            repo(
                "/dev/backend-api",
                &[("origin", "git@gitlab.example.com:kern/backend-api.git")],
            ),
            repo(
                "/dev/backend-api-2",
                &[(
                    "origin",
                    "https://oauth2:glpat-SECRET123@GitLab.example.com/Kern/Backend-API.git",
                )],
            ),
            repo(
                "/dev/fork",
                &[
                    ("origin", "git@gitlab.example.com:tim/backend-api.git"),
                    ("upstream", "git@gitlab.example.com:kern/backend-api.git"),
                ],
            ),
            repo(
                "/dev/rust",
                &[("origin", "https://github.com/rust-lang/rust.git")],
            ),
            repo("/dev/local-only", &[("backup", "/srv/git/a/b.git")]),
            repo("/dev/no-remote", &[]),
        ];
        let glab = [gh("gitlab.example.com", None)];
        let cfg = MrConfig::default();
        let map = host_map_for(&repos, &glab, &cfg);
        let projects = local_projects(&repos, &map, &cfg);
        assert_eq!(
            projects,
            vec![
                LocalProject {
                    host_key: "gitlab.example.com".to_string(),
                    full_path: "kern/backend-api".to_string(),
                    repos: vec![
                        PathBuf::from("/dev/backend-api"),
                        PathBuf::from("/dev/backend-api-2"),
                        PathBuf::from("/dev/fork"),
                    ],
                    included: true,
                },
                LocalProject {
                    host_key: "gitlab.example.com".to_string(),
                    full_path: "tim/backend-api".to_string(),
                    repos: vec![PathBuf::from("/dev/fork")],
                    included: true,
                },
            ]
        );
    }

    #[test]
    fn local_projects_two_remotes_same_project_count_once() {
        // https- und ssh-Remote im selben Klon zeigen auf dasselbe Projekt
        let repos = [repo(
            "/dev/a",
            &[
                ("origin", "https://gitlab.example.com/g/p.git"),
                ("ssh", "ssh://git@ssh.gitlab.example.com/g/p.git"),
            ],
        )];
        let glab = [gh("gitlab.example.com", Some("ssh.gitlab.example.com"))];
        let cfg = MrConfig::default();
        let projects = local_projects(&repos, &host_map_for(&repos, &glab, &cfg), &cfg);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].repos, vec![PathBuf::from("/dev/a")]);
    }

    #[test]
    fn local_projects_excluded_case_insensitive() {
        let repos = [
            repo(
                "/dev/a",
                &[("origin", "https://gitlab.example.com/kern/backend-api.git")],
            ),
            repo(
                "/dev/b",
                &[("origin", "https://h:8443/web/kundenportal.git")],
            ),
            repo(
                "/dev/c",
                &[("origin", "https://gitlab.example.com/tools/gitmanager.git")],
            ),
        ];
        let glab = [gh("gitlab.example.com", None), gh("h:8443", None)];
        // Bewusst nicht normalisiert (Großschreibung, Leerzeichen)
        let cfg = MrConfig {
            excluded_projects: vec![
                "GitLab.Example.com/Kern/Backend-API".to_string(),
                " h:8443/WEB/kundenportal ".to_string(),
            ],
            ..MrConfig::default()
        };
        let projects = local_projects(&repos, &host_map_for(&repos, &glab, &cfg), &cfg);
        let included: Vec<(String, bool)> = projects
            .iter()
            .map(|p| (project_key(&p.host_key, &p.full_path), p.included))
            .collect();
        assert_eq!(
            included,
            vec![
                ("gitlab.example.com/kern/backend-api".to_string(), false),
                ("gitlab.example.com/tools/gitmanager".to_string(), true),
                ("h:8443/web/kundenportal".to_string(), false),
            ]
        );
    }

    #[test]
    fn local_projects_without_gitlab_hosts_is_empty() {
        let repos = [repo(
            "/dev/a",
            &[("origin", "https://gitlab.example.com/g/p.git")],
        )];
        let cfg = MrConfig::default();
        let map = host_map_for(&repos, &[], &cfg);
        assert!(local_projects(&repos, &map, &cfg).is_empty());
    }

    #[test]
    fn project_key_is_lowercase() {
        assert_eq!(project_key("H:8443", "Kern/API"), "h:8443/kern/api");
    }
}
