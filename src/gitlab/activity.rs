//! Neue Kommentare und Commits je MR seit dem letzten Öffnen (Browser,
//! Claude, IDE, Auschecken) und Ereignisse für Benachrichtigungen (neue
//! Review-Anfrage, Erwähnung, neue Kommentare anderer).
//!
//! Der Stand liegt je MR in [`MrActivityStore`] und wird als JSON im
//! Datenordner gespeichert. Rein, ohne Netz und UI.

use super::model::{MergeRequest, MrSnapshot, Reason};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Einträge, die so lange (Sekunden) in keinem Abruf mehr auftauchten,
/// fallen weg (gemergt, geschlossen, nicht mehr beteiligt).
const FORGET_AFTER_SECS: i64 = 30 * 24 * 3600;

/// Stand eines MR.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MrMark {
    /// Kommentare beim letzten Öffnen
    pub seen_notes: u32,
    /// Commits beim letzten Öffnen
    pub seen_commits: u32,
    /// Head-Commit beim letzten Öffnen (erkennt auch Force-Pushes)
    pub seen_sha: Option<String>,
    /// Kommentare beim letzten Abruf (Benachrichtigung nur für neue)
    pub known_notes: u32,
    /// Beim letzten Abruf als Reviewer eingetragen
    pub reviewer: bool,
    /// Beim letzten Abruf erwähnt
    pub mentioned: bool,
    /// Unix-Sekunden des letzten Abrufs, in dem der MR vorkam
    pub last_fetch: i64,
}

/// Neues seit dem letzten Öffnen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Activity {
    pub new_notes: u32,
    pub new_commits: u32,
    /// Head-Commit geändert, ohne dass die Anzahl stieg (Force-Push, Rebase)
    pub rewritten: bool,
}

impl Activity {
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.new_notes == 0 && self.new_commits == 0 && !self.rewritten
    }
}

/// Anlass einer Benachrichtigung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MrEventKind {
    /// Ich wurde als Reviewer eingetragen
    ReviewRequested,
    /// Ich wurde erwähnt (`in_comment`: in einem Kommentar, sonst in der
    /// Beschreibung)
    Mentioned { in_comment: bool },
    /// Kommentarzahl gestiegen; ob die Kommentare von anderen stammen,
    /// klärt erst [`others_notes`] mit den letzten Kommentaren
    NotesAdded { before: u32, now: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MrEvent {
    pub kind: MrEventKind,
    pub host_key: String,
    pub id: String,
    pub iid: u64,
    pub project_path: String,
    pub title: String,
    pub web_url: String,
    pub author: String,
}

impl MrEvent {
    fn of(mr: &MergeRequest, kind: MrEventKind) -> Self {
        Self {
            kind,
            host_key: mr.host_key.clone(),
            id: mr.id.clone(),
            iid: mr.iid,
            project_path: mr.project_path.clone(),
            title: mr.title.clone(),
            web_url: mr.web_url.clone(),
            author: mr.author.name.clone(),
        }
    }
}

/// Schlüssel je MR (IDs sind nur je Host eindeutig).
pub fn mr_key(host_key: &str, id: &str) -> String {
    format!("{host_key}|{id}")
}

/// Gespeicherter Stand aller MRs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MrActivityStore {
    /// Erster Abruf erledigt: davor gibt es keine Benachrichtigungen (sonst
    /// meldete der erste Start jeden offenen MR als neu)
    pub initialized: bool,
    pub marks: HashMap<String, MrMark>,
}

impl MrActivityStore {
    /// `mr_activity.json` im lokalen Datenordner.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("com", "gitmanager", "gitmanager")
            .map(|d| d.data_local_dir().join("mr_activity.json"))
    }

    /// Fehlende oder kaputte Datei → leerer Stand.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string(self).map_err(std::io::Error::other)?;
        // Erst vollständig schreiben, dann ersetzen: kein halbes JSON beim Absturz
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }

    /// Neues seit dem letzten Öffnen; unbekannte MRs gelten als gesehen.
    pub fn activity(&self, mr: &MergeRequest) -> Activity {
        let Some(mark) = self.marks.get(&mr_key(&mr.host_key, &mr.id)) else {
            return Activity::default();
        };
        let new_notes = mr
            .user_notes_count
            .map_or(0, |n| n.saturating_sub(mark.seen_notes));
        let new_commits = mr
            .commit_count
            .map_or(0, |n| n.saturating_sub(mark.seen_commits));
        let rewritten = new_commits == 0
            && matches!((&mark.seen_sha, &mr.diff_head_sha), (Some(a), Some(b)) if a != b);
        Activity {
            new_notes,
            new_commits,
            rewritten,
        }
    }

    /// Trägt in jedem MR des Stands `activity` ein.
    pub fn annotate(&self, snapshot: &mut MrSnapshot) {
        let lists = snapshot
            .mine
            .iter_mut()
            .chain(snapshot.others.iter_mut())
            .chain(
                snapshot
                    .uninvolved
                    .iter_mut()
                    .flat_map(|l| l.mrs.iter_mut()),
            );
        for mr in lists {
            mr.activity = self.activity(mr);
        }
    }

    /// MR aus dem Stand (Meine, Von anderen, Nicht beteiligt).
    pub fn find<'a>(
        snapshot: &'a MrSnapshot,
        host_key: &str,
        id: &str,
    ) -> Option<&'a MergeRequest> {
        snapshot
            .mine
            .iter()
            .chain(&snapshot.others)
            .chain(snapshot.uninvolved.iter().flat_map(|l| &l.mrs))
            .find(|m| m.host_key == host_key && m.id == id)
    }

    /// MR geöffnet: aktueller Stand gilt als gesehen. `true` bei Änderung.
    pub fn mark_seen(&mut self, mr: &MergeRequest) -> bool {
        let mark = self.marks.entry(mr_key(&mr.host_key, &mr.id)).or_default();
        let before = mark.clone();
        if let Some(n) = mr.user_notes_count {
            mark.seen_notes = n;
            mark.known_notes = mark.known_notes.max(n);
        }
        if let Some(n) = mr.commit_count {
            mark.seen_commits = n;
        }
        if mr.diff_head_sha.is_some() {
            mark.seen_sha = mr.diff_head_sha.clone();
        }
        *mark != before
    }

    /// Kommentare bis `count` als gesehen (alle neuen sind meine eigenen).
    pub fn mark_notes_seen(&mut self, host_key: &str, id: &str, count: u32) -> bool {
        match self.marks.get_mut(&mr_key(host_key, id)) {
            Some(mark) if mark.seen_notes < count => {
                mark.seen_notes = count;
                true
            }
            _ => false,
        }
    }

    /// Nimmt einen Abruf auf: neue MRs gelten als gesehen, der Stand für
    /// Benachrichtigungen rückt nach. Liefert die fälligen Ereignisse (vor
    /// dem ersten Abruf keine).
    pub fn observe(&mut self, snapshot: &MrSnapshot, now: i64) -> Vec<MrEvent> {
        let notify = self.initialized;
        let mut events = Vec::new();
        let mut present = HashSet::new();
        for (mr, mine) in snapshot
            .mine
            .iter()
            .map(|m| (m, true))
            .chain(snapshot.others.iter().map(|m| (m, false)))
        {
            let key = mr_key(&mr.host_key, &mr.id);
            if !present.insert(key.clone()) {
                continue;
            }
            let reviewer = mr.is_reviewer();
            let mentioned = mr
                .reasons
                .iter()
                .any(|r| matches!(r, Reason::MentionComment | Reason::MentionDescription));
            let mention = MrEventKind::Mentioned {
                in_comment: mr.reasons.contains(&Reason::MentionComment),
            };
            let notes = mr.user_notes_count.unwrap_or(0);
            let mark = match self.marks.get_mut(&key) {
                Some(mark) => mark,
                None => {
                    // Erstes Auftauchen: Stand als gesehen, damit nicht jeder
                    // MR mit „neu“ startet; Review-Anfrage/Erwähnung melden
                    if notify && !mine {
                        if reviewer {
                            events.push(MrEvent::of(mr, MrEventKind::ReviewRequested));
                        } else if mentioned {
                            events.push(MrEvent::of(mr, mention.clone()));
                        }
                    }
                    let mut mark = MrMark {
                        reviewer,
                        mentioned,
                        last_fetch: now,
                        ..MrMark::default()
                    };
                    mark.seen_notes = notes;
                    mark.known_notes = notes;
                    mark.seen_commits = mr.commit_count.unwrap_or(0);
                    mark.seen_sha = mr.diff_head_sha.clone();
                    self.marks.insert(key, mark);
                    continue;
                }
            };
            if notify && !mine {
                if reviewer && !mark.reviewer {
                    events.push(MrEvent::of(mr, MrEventKind::ReviewRequested));
                } else if mentioned && !mark.mentioned {
                    events.push(MrEvent::of(mr, mention.clone()));
                }
            }
            if notify && mr.user_notes_count.is_some() && notes > mark.known_notes {
                events.push(MrEvent::of(
                    mr,
                    MrEventKind::NotesAdded {
                        before: mark.known_notes,
                        now: notes,
                    },
                ));
            }
            if mr.user_notes_count.is_some() {
                mark.known_notes = notes;
                // Gelöschte Kommentare: „neu“ darf nicht hängen bleiben
                mark.seen_notes = mark.seen_notes.min(notes);
            }
            if let Some(c) = mr.commit_count {
                mark.seen_commits = mark.seen_commits.min(c);
            }
            mark.reviewer = reviewer;
            // Bleibt gesetzt: fehlen die Erwähnungen in einem Abruf (Teilfehler),
            // käme sonst beim nächsten dieselbe Meldung noch einmal
            mark.mentioned |= mentioned;
            mark.last_fetch = now;
        }
        self.marks
            .retain(|_, m| now.saturating_sub(m.last_fetch) < FORGET_AFTER_SECS);
        self.initialized = true;
        events
    }
}

/// Ein Kommentar aus der Folgeabfrage (`graphql::recent_notes`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteInfo {
    pub author_username: String,
    pub author_name: String,
    pub body: String,
    pub system: bool,
}

/// Die `added` neuesten Nutzer-Kommentare (`notes` älteste zuerst), die
/// nicht von `me` stammen.
pub fn others_notes<'a>(notes: &'a [NoteInfo], added: u32, me: Option<&str>) -> Vec<&'a NoteInfo> {
    let user_notes: Vec<&NoteInfo> = notes.iter().filter(|n| !n.system).collect();
    let skip = user_notes.len().saturating_sub(added as usize);
    user_notes
        .into_iter()
        .skip(skip)
        .filter(|n| me.is_none_or(|me| !n.author_username.eq_ignore_ascii_case(me)))
        .collect()
}

/// Kommentar einzeilig und gekürzt für die Benachrichtigung.
pub fn note_snippet(body: &str, max_chars: usize) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > max_chars {
        let cut: String = flat.chars().take(max_chars).collect();
        format!("{}…", cut.trim_end())
    } else {
        flat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mr(id: &str, notes: u32, commits: u32, sha: &str) -> MergeRequest {
        MergeRequest {
            host_key: "gitlab.example.com".to_string(),
            id: id.to_string(),
            iid: 7,
            title: format!("MR {id}"),
            user_notes_count: Some(notes),
            commit_count: Some(commits),
            diff_head_sha: Some(sha.to_string()),
            ..MergeRequest::default()
        }
    }

    fn snap(mine: Vec<MergeRequest>, others: Vec<MergeRequest>) -> MrSnapshot {
        MrSnapshot {
            mine,
            others,
            ..MrSnapshot::default()
        }
    }

    fn reviewer(mut m: MergeRequest) -> MergeRequest {
        m.reasons = vec![Reason::Reviewer];
        m
    }

    #[test]
    fn first_fetch_is_silent_and_seen() {
        let mut store = MrActivityStore::default();
        let a = reviewer(mr("a", 3, 2, "x"));
        let events = store.observe(&snap(vec![], vec![a.clone()]), 100);
        assert!(events.is_empty());
        assert!(store.initialized);
        assert!(store.activity(&a).is_empty());
    }

    #[test]
    fn new_review_request_and_mention_notify() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![], vec![]), 100);
        let a = reviewer(mr("a", 0, 1, "x"));
        let mut b = mr("b", 0, 1, "y");
        b.reasons = vec![Reason::MentionComment];
        let events = store.observe(&snap(vec![], vec![a, b]), 200);
        let kinds: Vec<_> = events.iter().map(|e| e.kind.clone()).collect();
        assert_eq!(
            kinds,
            [
                MrEventKind::ReviewRequested,
                MrEventKind::Mentioned { in_comment: true }
            ]
        );
    }

    #[test]
    fn reviewer_added_later_notifies_once() {
        let mut store = MrActivityStore::default();
        let mut a = mr("a", 0, 1, "x");
        a.reasons = vec![Reason::MentionDescription];
        store.observe(&snap(vec![], vec![a.clone()]), 100);
        let a2 = reviewer(a.clone());
        let events = store.observe(&snap(vec![], vec![a2.clone()]), 200);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, MrEventKind::ReviewRequested);
        assert!(store.observe(&snap(vec![], vec![a2]), 300).is_empty());
    }

    #[test]
    fn mention_in_comment_comes_with_notes_event() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![], vec![mr("a", 1, 1, "x")]), 100);
        let mut a = mr("a", 2, 1, "x");
        a.reasons = vec![Reason::MentionComment];
        let kinds: Vec<_> = store
            .observe(&snap(vec![], vec![a]), 200)
            .into_iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                MrEventKind::Mentioned { in_comment: true },
                MrEventKind::NotesAdded { before: 1, now: 2 }
            ]
        );
    }

    #[test]
    fn mention_missing_once_does_not_renotify() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![], vec![]), 100);
        let mut a = mr("a", 0, 1, "x");
        a.reasons = vec![Reason::MentionComment];
        assert_eq!(store.observe(&snap(vec![], vec![a.clone()]), 200).len(), 1);
        let mut gone = a.clone();
        gone.reasons.clear();
        assert!(store.observe(&snap(vec![], vec![gone]), 300).is_empty());
        assert!(store.observe(&snap(vec![], vec![a]), 400).is_empty());
    }

    #[test]
    fn new_notes_and_commits_show_until_seen() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![mr("a", 2, 1, "x")], vec![]), 100);
        let later = mr("a", 5, 3, "z");
        let events = store.observe(&snap(vec![later.clone()], vec![]), 200);
        assert_eq!(
            events[0].kind,
            MrEventKind::NotesAdded { before: 2, now: 5 }
        );
        let act = store.activity(&later);
        assert_eq!((act.new_notes, act.new_commits), (3, 2));
        assert!(!act.rewritten);
        let mut s = snap(vec![later.clone()], vec![]);
        store.annotate(&mut s);
        assert_eq!(s.mine[0].activity.new_notes, 3);
        assert!(store.mark_seen(&later));
        assert!(store.activity(&later).is_empty());
        // Gleicher Stand erneut: keine weitere Benachrichtigung
        assert!(store.observe(&snap(vec![later], vec![]), 300).is_empty());
    }

    #[test]
    fn force_push_counts_as_rewritten() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![mr("a", 0, 2, "x")], vec![]), 100);
        let act = store.activity(&mr("a", 0, 2, "y"));
        assert!(act.rewritten);
        assert_eq!(act.new_commits, 0);
    }

    #[test]
    fn deleted_notes_do_not_stick() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![mr("a", 4, 1, "x")], vec![]), 100);
        store.observe(&snap(vec![mr("a", 2, 1, "x")], vec![]), 200);
        let events = store.observe(&snap(vec![mr("a", 3, 1, "x")], vec![]), 300);
        assert_eq!(
            events[0].kind,
            MrEventKind::NotesAdded { before: 2, now: 3 }
        );
        assert_eq!(store.activity(&mr("a", 3, 1, "x")).new_notes, 1);
    }

    #[test]
    fn old_entries_are_forgotten() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![mr("a", 0, 1, "x")], vec![]), 100);
        store.observe(&snap(vec![], vec![]), 100 + FORGET_AFTER_SECS);
        assert!(store.marks.is_empty());
    }

    #[test]
    fn mark_notes_seen_only_raises() {
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![mr("a", 1, 1, "x")], vec![]), 100);
        store.observe(&snap(vec![mr("a", 3, 1, "x")], vec![]), 200);
        assert!(store.mark_notes_seen("gitlab.example.com", "a", 3));
        assert!(!store.mark_notes_seen("gitlab.example.com", "a", 2));
        assert_eq!(store.activity(&mr("a", 3, 1, "x")).new_notes, 0);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/mr_activity.json");
        let mut store = MrActivityStore::default();
        store.observe(&snap(vec![mr("a", 1, 1, "x")], vec![]), 100);
        store.save(&path).unwrap();
        assert_eq!(MrActivityStore::load(&path), store);
        std::fs::write(&path, "kaputt").unwrap();
        assert_eq!(MrActivityStore::load(&path), MrActivityStore::default());
    }

    fn note(user: &str, body: &str, system: bool) -> NoteInfo {
        NoteInfo {
            author_username: user.to_string(),
            author_name: user.to_uppercase(),
            body: body.to_string(),
            system,
        }
    }

    #[test]
    fn others_notes_skips_own_and_system() {
        let notes = [
            note("alt", "alt", false),
            note("bot", "added 1 commit", true),
            note("me", "meins", false),
            note("anna", "bitte prüfen", false),
        ];
        let got = others_notes(&notes, 2, Some("ME"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "bitte prüfen");
        assert!(others_notes(&notes[..3], 1, Some("me")).is_empty());
    }

    #[test]
    fn snippet_flattens_and_cuts() {
        assert_eq!(note_snippet("a\n\n b", 10), "a b");
        assert_eq!(note_snippet("abcdefghij klm", 5), "abcde…");
    }
}
