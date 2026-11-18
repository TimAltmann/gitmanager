//! Neue Kommentare/Commits je MR und Windows-Benachrichtigungen der
//! MR-Ansicht: Stand laden und speichern, „gesehen“ beim Öffnen (Browser,
//! Claude/Agent, IDE, Auschecken), Ereignisse nach jedem Abruf.

use super::merge_requests::MrResult;
use super::{MainView, MyApp};
use crate::claude_notify::{show_simple_notification, SimpleToast, ToastClick};
use crate::gitlab::activity::{
    note_snippet, others_notes, MrActivityStore, MrEvent, MrEventKind, NoteInfo,
};
use crate::gitlab::service::fetch_recent_notes;
use crate::gitlab::time::now_unix;
use crate::i18n::{tr, tr_fmt};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

/// Kennung der Kopfzeile der MR-Benachrichtigungen
const HEADER_ID: &str = "merge-requests";
/// Zeichen des Kommentars in der Benachrichtigung
const SNIPPET_CHARS: usize = 160;
/// Höchstens so viele Benachrichtigungen je Abruf; der Rest als Sammelmeldung
const MAX_TOASTS: usize = 4;

/// Klick in einer MR-Benachrichtigung.
struct ToastHit {
    host_key: String,
    id: String,
    url: String,
    click: ToastClick,
}

pub(super) struct MrActivityState {
    pub store: MrActivityStore,
    /// `None` in Tests: nichts speichern
    path: Option<PathBuf>,
    click_tx: Sender<ToastHit>,
    click_rx: Receiver<ToastHit>,
    /// Weckt die App beim Klick auf eine Benachrichtigung
    ctx: Option<egui::Context>,
}

impl Default for MrActivityState {
    fn default() -> Self {
        let (click_tx, click_rx) = mpsc::channel();
        Self {
            store: MrActivityStore::default(),
            path: None,
            click_tx,
            click_rx,
            ctx: None,
        }
    }
}

impl MrActivityState {
    /// Stand aus dem Datenordner.
    pub fn load_default() -> Self {
        let path = MrActivityStore::default_path();
        Self {
            store: path
                .as_deref()
                .map(MrActivityStore::load)
                .unwrap_or_default(),
            path,
            ..Self::default()
        }
    }

    fn save(&self) {
        if let Some(path) = &self.path {
            if let Err(e) = self.store.save(path) {
                eprintln!("MR-Aktivität nicht gespeichert: {e}");
            }
        }
    }
}

/// Neue Kommentare eines MR mit den zuletzt gelesenen Kommentaren.
pub(super) struct NotesJob {
    event: MrEvent,
    notes: Vec<NoteInfo>,
}

impl MyApp {
    /// Nach einem Abruf: Stand übernehmen, Ereignisse melden, für neue
    /// Kommentare die letzten Kommentare nachladen.
    pub(super) fn on_mr_activity(&mut self, me_per_host: &BTreeMap<String, String>) {
        let Some(snap) = self.mr_snapshot.as_deref_mut() else {
            return;
        };
        let events = self.mr_activity.store.observe(snap, now_unix());
        self.mr_activity.store.annotate(snap);
        self.mr_activity.save();
        let mut by_host: HashMap<String, Vec<MrEvent>> = HashMap::new();
        let mut direct = Vec::new();
        for event in events {
            match event.kind {
                MrEventKind::NotesAdded { .. } => by_host
                    .entry(event.host_key.clone())
                    .or_default()
                    .push(event),
                _ => direct.push(event),
            }
        }
        self.notify_mr_events(direct.into_iter().map(|e| (e, None)).collect());
        for (host, events) in by_host {
            let runner = Arc::clone(&self.glab);
            let tx = self.mr_tx.clone();
            let me = me_per_host.get(&host).cloned();
            std::thread::spawn(move || {
                let mrs: Vec<(String, u64)> = events
                    .iter()
                    .map(|e| (e.project_path.clone(), e.iid))
                    .collect();
                let notes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    fetch_recent_notes(&*runner, &host, &mrs)
                }))
                .unwrap_or_default();
                let jobs = events
                    .into_iter()
                    .zip(notes.into_iter().chain(std::iter::repeat_with(Vec::new)))
                    .map(|(event, notes)| NotesJob { event, notes })
                    .collect();
                let _ = tx.send(MrResult::Notes { me, jobs });
            });
        }
    }

    /// Letzte Kommentare da: nur eigene → als gesehen, sonst melden.
    pub(super) fn on_mr_notes(&mut self, me: Option<String>, jobs: Vec<NotesJob>) {
        let mut notify = Vec::new();
        let mut changed = false;
        for NotesJob { event, notes } in jobs {
            let MrEventKind::NotesAdded { before, now } = event.kind else {
                continue;
            };
            let added = now.saturating_sub(before);
            let others = others_notes(&notes, added, me.as_deref());
            // Ohne lesbare Kommentare (Fehler, Rechte) trotzdem melden
            if others.is_empty() && !notes.is_empty() {
                changed |= self
                    .mr_activity
                    .store
                    .mark_notes_seen(&event.host_key, &event.id, now);
                continue;
            }
            let line = match others.as_slice() {
                [] => None,
                [one] => Some(format!(
                    "{}: {}",
                    one.author_name,
                    note_snippet(&one.body, SNIPPET_CHARS)
                )),
                [.., last] => Some(tr_fmt(
                    self.config.language,
                    "mr_notify_comments_more",
                    &[
                        &last.author_name,
                        &(others.len() - 1).to_string(),
                        &note_snippet(&last.body, SNIPPET_CHARS),
                    ],
                )),
            };
            notify.push((event, line));
        }
        if changed {
            self.mr_activity.save();
            if let Some(snap) = self.mr_snapshot.as_deref_mut() {
                self.mr_activity.store.annotate(snap);
            }
        }
        self.notify_mr_events(notify);
    }

    /// Benachrichtigungen zeigen (wenn eingeschaltet); `line` ersetzt die
    /// Standardzeile (Kommentar mit Autor).
    fn notify_mr_events(&mut self, events: Vec<(MrEvent, Option<String>)>) {
        if !self.config.merge_requests.notify || events.is_empty() {
            return;
        }
        let lang = self.config.language;
        let total = events.len();
        for (i, (event, line)) in events.into_iter().enumerate() {
            if i + 1 == MAX_TOASTS && total > MAX_TOASTS {
                let toast = SimpleToast {
                    header_id: HEADER_ID,
                    header: tr(lang, "view_mrs"),
                    title: tr_fmt(lang, "mr_notify_more", &[&(total - i).to_string()]),
                    line: String::new(),
                    attribution: String::new(),
                    button_open: tr(lang, "mr_notify_btn_show"),
                    button_show: tr(lang, "mr_notify_btn_show"),
                };
                self.show_mr_toast(toast, String::new(), String::new(), String::new());
                break;
            }
            let (title_key, default_line) = match &event.kind {
                MrEventKind::ReviewRequested => (
                    "mr_notify_review",
                    tr_fmt(lang, "mr_notify_review_line", &[&event.author]),
                ),
                MrEventKind::Mentioned => (
                    "mr_notify_mention",
                    tr_fmt(lang, "mr_notify_mention_line", &[&event.author]),
                ),
                MrEventKind::NotesAdded { before, now } => (
                    "mr_notify_comments",
                    tr_fmt(
                        lang,
                        "mr_notify_comments_line",
                        &[&now.saturating_sub(*before).to_string()],
                    ),
                ),
            };
            let toast = SimpleToast {
                header_id: HEADER_ID,
                header: tr(lang, "view_mrs"),
                title: format!("{} · {}", tr(lang, title_key), event.title),
                line: line.unwrap_or(default_line),
                attribution: format!("{} !{}", event.project_path, event.iid),
                button_open: tr(lang, "mr_notify_btn_open"),
                button_show: tr(lang, "mr_notify_btn_show"),
            };
            self.show_mr_toast(toast, event.host_key, event.id, event.web_url);
        }
    }

    fn show_mr_toast(&self, toast: SimpleToast, host_key: String, id: String, url: String) {
        let tx = self.mr_activity.click_tx.clone();
        let ctx = self.mr_activity.ctx.clone();
        show_simple_notification(toast, move |click| {
            let _ = tx.send(ToastHit {
                host_key: host_key.clone(),
                id: id.clone(),
                url: url.clone(),
                click,
            });
            if let Some(ctx) = &ctx {
                ctx.request_repaint();
            }
        });
    }

    /// Klicks auf MR-Benachrichtigungen (aus `poll_mr`).
    pub(super) fn poll_mr_toasts(&mut self, ctx: &egui::Context) {
        if self.mr_activity.ctx.is_none() {
            self.mr_activity.ctx = Some(ctx.clone());
        }
        while let Ok(hit) = self.mr_activity.click_rx.try_recv() {
            match hit.click {
                ToastClick::Terminal if !hit.url.is_empty() => {
                    ctx.open_url(egui::OpenUrl::new_tab(&hit.url));
                    self.mark_mr_seen(&hit.host_key, &hit.id);
                }
                _ => {
                    self.main_view = MainView::MergeRequests;
                    self.show_main_window(ctx);
                }
            }
        }
    }

    /// MR geöffnet (Browser, Agent, IDE, Auschecken): Neues gilt als gesehen.
    pub(super) fn mark_mr_seen(&mut self, host_key: &str, id: &str) {
        let Some(snap) = self.mr_snapshot.as_deref_mut() else {
            return;
        };
        let Some(mr) = MrActivityStore::find(snap, host_key, id).cloned() else {
            return;
        };
        if self.mr_activity.store.mark_seen(&mr) {
            self.mr_activity.store.annotate(snap);
            self.mr_activity.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::test_app;
    use super::*;
    use crate::gitlab::activity::Activity;
    use crate::gitlab::model::{MergeRequest, MrSnapshot, Reason};

    fn mr(id: &str, notes: u32) -> MergeRequest {
        MergeRequest {
            host_key: "h".to_string(),
            id: id.to_string(),
            iid: 1,
            title: "T".to_string(),
            user_notes_count: Some(notes),
            commit_count: Some(1),
            diff_head_sha: Some("x".to_string()),
            reasons: vec![Reason::Reviewer],
            ..MergeRequest::default()
        }
    }

    fn set_snapshot(app: &mut MyApp, others: Vec<MergeRequest>) {
        app.mr_snapshot = Some(Box::new(MrSnapshot {
            others,
            ..MrSnapshot::default()
        }));
    }

    #[test]
    fn opening_marks_seen_and_clears_badge() {
        let mut app = test_app();
        set_snapshot(&mut app, vec![mr("a", 1)]);
        app.on_mr_activity(&BTreeMap::new());
        set_snapshot(&mut app, vec![mr("a", 4)]);
        app.on_mr_activity(&BTreeMap::new());
        let snap = app.mr_snapshot.as_deref().unwrap();
        assert_eq!(snap.others[0].activity.new_notes, 3);

        app.mark_mr_seen("h", "a");
        let snap = app.mr_snapshot.as_deref().unwrap();
        assert_eq!(snap.others[0].activity, Activity::default());
    }

    #[test]
    fn own_comments_count_as_seen() {
        let mut app = test_app();
        set_snapshot(&mut app, vec![mr("a", 1)]);
        app.on_mr_activity(&BTreeMap::new());
        set_snapshot(&mut app, vec![mr("a", 2)]);
        // Abruf der Kommentare nicht abwarten: Ergebnis direkt einspielen
        let store = &mut app.mr_activity.store;
        store.observe(app.mr_snapshot.as_deref().unwrap(), now_unix());
        let event = MrEvent {
            kind: MrEventKind::NotesAdded { before: 1, now: 2 },
            host_key: "h".to_string(),
            id: "a".to_string(),
            iid: 1,
            project_path: "g/p".to_string(),
            title: "T".to_string(),
            web_url: String::new(),
            author: String::new(),
        };
        let mine = NoteInfo {
            author_username: "me".to_string(),
            body: "erledigt".to_string(),
            ..NoteInfo::default()
        };
        app.on_mr_notes(
            Some("me".to_string()),
            vec![NotesJob {
                event,
                notes: vec![mine],
            }],
        );
        let snap = app.mr_snapshot.as_deref().unwrap();
        assert_eq!(snap.others[0].activity.new_notes, 0);
    }
}
