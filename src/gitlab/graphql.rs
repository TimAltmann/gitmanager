//! GitLab-GraphQL über `glab api graphql`: Abfragen als Modell (Fragment
//! `Mr`, Dashboard, Erwähnungen, nicht beteiligte MRs, Zugriff), Aufruf mit
//! Schema-Fallback und Parser in Rohmodelle mit `Option` überall. Die
//! Domänenlogik (Klassifizierung, Sortierung) folgt in `model.rs`.

use super::glab::{
    classify, graphql_messages, GlabErrorKind, GlabOutput, GlabRequest, GlabRunner, StdinBody,
};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashSet};

/// Seitengröße der MR-Listen im Dashboard und der Erwähnungen.
pub const PAGE_SIZE: usize = 50;
/// Seitengröße je Projekt bei „Alle offenen“.
pub const UNINVOLVED_PAGE_SIZE: usize = 20;
/// Höchstens so viele Projekte je `UNINVOLVED`-Abfrage (Komplexität).
pub const UNINVOLVED_CHUNK: usize = 25;
/// Höchstens so viele Projekte je `UNINVOLVED_PAGE`-Abfrage. GitLab rechnet
/// jeden Alias mit seinem ganzen Teilbaum: ein Projekt mit `first: 20` und
/// Fragment `Mr` kostet etwa 70 (abgeleitet aus DASHBOARD, gemessen 167 für
/// zwei Listen mit `first: 50`), drei also etwa 210 von 250.
pub const UNINVOLVED_PAGE_CHUNK: usize = 3;
/// `projects(fullPaths:)` nimmt höchstens 50 Pfade.
pub const ACCESS_CHUNK: usize = 50;
/// Erwähnungen: höchstens so viele Seiten (erledigte To-dos häufen sich).
pub const MENTIONS_MAX_PAGES: usize = 3;
/// Suchtreffer je Projekt bei Erwähnungen in der Beschreibung.
pub const DESCRIPTION_MENTIONS_PAGE_SIZE: usize = 20;
/// Versuche je Abfrage beim Schema-Fallback; GitLab meldet höchstens 5
/// Fehler je Antwort.
pub const MAX_ATTEMPTS: usize = 4;
/// Wiederholungen, wenn ein ausgelasteter Server die Abfrage mit „Timeout on
/// …“ ohne Daten ablehnt (z. B. „Timeout on validation of query“).
pub const TIMEOUT_RETRIES: usize = 2;
/// Pause vor der n-ten Wiederholung nach einem Server-Timeout.
fn timeout_backoff(retry: usize) -> std::time::Duration {
    if cfg!(test) {
        return std::time::Duration::ZERO;
    }
    std::time::Duration::from_secs(2 * retry as u64)
}
/// GitLab-Grenze für den Abfragetext; nur in Tests geprüft.
#[cfg_attr(not(test), allow(dead_code))]
pub const MAX_QUERY_CHARS: usize = 10_000;

/// Typnamen je nach GitLab-Version; Schema-Fehler nennen den des Servers.
const CURRENT_USER: &[&str] = &["CurrentUser", "UserCore"];
const AUTHOR: &[&str] = &["MergeRequestAuthor", "UserCore"];
const REVIEWER: &[&str] = &["MergeRequestReviewer", "UserCore"];
const MR_FRAGMENT: &str = "Mr";

// --- Abfragemodell -----------------------------------------------------

/// Auswahl auf einem Objekttyp. Mehrere Typnamen, wo GitLab den Typ
/// zwischen Versionen umbenannt hat (`currentUser`: `CurrentUser`, früher
/// `UserCore`); Schema-Fehler nennen den Typ des Servers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub on: Vec<&'static str>,
    pub items: Vec<Item>,
}

impl Selection {
    fn new(on: &[&'static str], items: Vec<Item>) -> Self {
        Self {
            on: on.to_vec(),
            items,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Field(Field),
    /// `...Name` auf ein Fragment der Abfrage
    Spread(&'static str),
    /// `... on Typ { … }`
    Inline(Selection),
}

impl From<Field> for Item {
    fn from(field: Field) -> Self {
        Self::Field(field)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub alias: Option<String>,
    pub name: &'static str,
    pub args: Vec<Arg>,
    /// `None` bei Skalaren
    pub sel: Option<Selection>,
}

impl Field {
    /// Objektfeld mit Auswahl auf den Typen `on`.
    fn object(name: &'static str, on: &[&'static str], items: Vec<Item>) -> Self {
        Self {
            alias: None,
            name,
            args: Vec::new(),
            sel: Some(Selection::new(on, items)),
        }
    }

    fn alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Pflichtargument
    fn arg(mut self, name: &'static str, value: impl std::fmt::Display) -> Self {
        self.args.push(Arg {
            name,
            value: value.to_string(),
            optional: false,
        });
        self
    }

    /// Argument, das beim Schema-Fallback entfallen darf
    fn opt_arg(mut self, name: &'static str, value: impl std::fmt::Display) -> Self {
        self.args.push(Arg {
            name,
            value: value.to_string(),
            optional: true,
        });
        self
    }
}

/// Skalare Felder, durch Leerzeichen getrennt.
fn scalars(names: &'static str) -> Vec<Item> {
    names
        .split_whitespace()
        .map(|name| {
            Item::Field(Field {
                alias: None,
                name,
                args: Vec::new(),
                sel: None,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    pub name: &'static str,
    /// GraphQL-Literal oder Variable (`$me`); Pfade und Namen nur als Variable
    pub value: String,
    /// Darf beim Schema-Fallback entfallen; der Client filtert nach.
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub name: &'static str,
    pub sel: Selection,
}

/// Eine GraphQL-Operation samt Variablen.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    /// Operationsname
    pub name: &'static str,
    /// Variablen `(name, GraphQL-Typ)`; gesendet werden nur benutzte
    pub var_defs: Vec<(String, &'static str)>,
    pub variables: Map<String, Value>,
    pub root: Selection,
    pub fragments: Vec<Fragment>,
}

impl Query {
    fn new(name: &'static str, root: Vec<Item>) -> Self {
        Self {
            name,
            var_defs: Vec::new(),
            variables: Map::new(),
            root: Selection::new(&["Query"], root),
            fragments: Vec::new(),
        }
    }

    /// Variable `$name: ty`; `None` wird nicht gesendet (= `null`).
    fn var(mut self, name: impl Into<String>, ty: &'static str, value: Option<Value>) -> Self {
        let name = name.into();
        if let Some(value) = value {
            self.variables.insert(name.clone(), value);
        }
        self.var_defs.push((name, ty));
        self
    }

    fn fragment(mut self, fragment: Fragment) -> Self {
        self.fragments.push(fragment);
        self
    }

    /// Kompakter Abfragetext; nur benutzte Variablen und Fragmente.
    // Nur in Tests genutzt (Abfragetext prüfen).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn text(&self) -> String {
        self.render().0
    }

    /// Body für `--input -`: `{"query": …, "variables": …}`.
    pub fn body(&self) -> String {
        let (text, used) = self.render();
        let variables: Map<String, Value> = self
            .variables
            .iter()
            .filter(|(name, _)| used.contains(name.as_str()))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        serde_json::json!({ "query": text, "variables": variables }).to_string()
    }

    /// Text und benutzte Variablen. Ein unbenutztes Fragment oder eine
    /// unbenutzte Variable lehnt GitLab ab (nach entfernten Argumenten).
    fn render(&self) -> (String, BTreeSet<String>) {
        let mut spreads = BTreeSet::new();
        for_each_item(&self.root, &mut |item| {
            if let Item::Spread(name) = item {
                spreads.insert(*name);
            }
        });
        let fragments: Vec<&Fragment> = self
            .fragments
            .iter()
            .filter(|f| spreads.contains(f.name))
            .collect();
        let mut used = BTreeSet::new();
        let mut collect = |item: &Item| {
            if let Item::Field(field) = item {
                for arg in &field.args {
                    used.extend(variables_in(&arg.value));
                }
            }
        };
        for_each_item(&self.root, &mut collect);
        for fragment in &fragments {
            for_each_item(&fragment.sel, &mut collect);
        }
        let defs: Vec<String> = self
            .var_defs
            .iter()
            .filter(|(name, _)| used.contains(name))
            .map(|(name, ty)| format!("${name}: {ty}"))
            .collect();
        let mut out = format!("query {}", self.name);
        if !defs.is_empty() {
            out.push('(');
            out.push_str(&defs.join(", "));
            out.push(')');
        }
        out.push(' ');
        render_selection(&self.root, &mut out);
        for fragment in fragments {
            let on = fragment.sel.on.first().copied().unwrap_or_default();
            out.push_str(&format!(" fragment {} on {on} ", fragment.name));
            render_selection(&fragment.sel, &mut out);
        }
        (out, used)
    }

    /// Entfernt, was ein Schema-Fehler bemängelt; `true` bei Änderung.
    /// Danach fallen leere Auswahlen samt Elternknoten weg.
    pub fn reduce(&mut self, err: &SchemaError) -> bool {
        let changed = match err.code {
            SchemaErrorCode::UndefinedField => self.walk_mut(&mut |sel| {
                if !sel.on.iter().any(|t| *t == err.type_name) {
                    return false;
                }
                let before = sel.items.len();
                sel.items
                    .retain(|item| !matches!(item, Item::Field(f) if f.name == err.name));
                sel.items.len() != before
            }),
            SchemaErrorCode::ArgumentNotAccepted
            | SchemaErrorCode::ArgumentLiteralsIncompatible => self.walk_mut(&mut |sel| {
                let mut changed = false;
                for item in &mut sel.items {
                    if let Item::Field(Field {
                        alias, name, args, ..
                    }) = item
                    {
                        let before = args.len();
                        args.retain(|arg| {
                            !(arg.optional && err.hits_argument(name, alias.as_deref(), arg))
                        });
                        changed |= args.len() != before;
                    }
                }
                changed
            }),
        };
        if changed {
            self.prune();
        }
        changed
    }

    /// `visit` auf jede Auswahl der Operation und der Fragmente.
    fn walk_mut(&mut self, visit: &mut dyn FnMut(&mut Selection) -> bool) -> bool {
        let mut changed = walk_mut(&mut self.root, visit);
        for fragment in &mut self.fragments {
            changed |= walk_mut(&mut fragment.sel, visit);
        }
        changed
    }

    /// Entfernt leere Auswahlen samt Elternknoten, leere Fragmente samt
    /// Spreads; wiederholt, bis sich nichts mehr ändert.
    fn prune(&mut self) {
        loop {
            let empty: Vec<&'static str> = self
                .fragments
                .iter()
                .filter(|f| f.sel.items.is_empty())
                .map(|f| f.name)
                .collect();
            let changed = self.walk_mut(&mut |sel| {
                let before = sel.items.len();
                sel.items.retain(|item| match item {
                    Item::Field(Field {
                        sel: Some(child), ..
                    })
                    | Item::Inline(child) => !child.items.is_empty(),
                    Item::Spread(name) => !empty.contains(name),
                    Item::Field(_) => true,
                });
                sel.items.len() != before
            });
            if !changed {
                break;
            }
        }
        self.fragments.retain(|f| !f.sel.items.is_empty());
    }
}

/// Alle Einträge einer Auswahl, auch verschachtelte (ohne Fragmente).
fn for_each_item<'a>(sel: &'a Selection, visit: &mut dyn FnMut(&'a Item)) {
    for item in &sel.items {
        visit(item);
        if let Item::Field(Field {
            sel: Some(child), ..
        })
        | Item::Inline(child) = item
        {
            for_each_item(child, visit);
        }
    }
}

/// `visit` auf `sel` und alle Unterauswahlen (Eltern vor Kindern); `true`,
/// wenn irgendwo etwas geändert wurde.
fn walk_mut(sel: &mut Selection, visit: &mut dyn FnMut(&mut Selection) -> bool) -> bool {
    let mut changed = visit(sel);
    for item in &mut sel.items {
        if let Item::Field(Field {
            sel: Some(child), ..
        })
        | Item::Inline(child) = item
        {
            changed |= walk_mut(child, visit);
        }
    }
    changed
}

/// Variablennamen (`$name`) in einem Argumentwert.
fn variables_in(value: &str) -> impl Iterator<Item = String> + '_ {
    value.split('$').skip(1).map(|rest| {
        rest.chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect()
    })
}

fn render_selection(sel: &Selection, out: &mut String) {
    out.push('{');
    for item in &sel.items {
        out.push(' ');
        match item {
            Item::Field(field) => {
                if let Some(alias) = &field.alias {
                    out.push_str(alias);
                    out.push_str(": ");
                }
                out.push_str(field.name);
                if !field.args.is_empty() {
                    let args: Vec<String> = field
                        .args
                        .iter()
                        .map(|arg| format!("{}: {}", arg.name, arg.value))
                        .collect();
                    out.push('(');
                    out.push_str(&args.join(", "));
                    out.push(')');
                }
                if let Some(child) = &field.sel {
                    out.push(' ');
                    render_selection(child, out);
                }
            }
            Item::Spread(name) => {
                out.push_str("...");
                out.push_str(name);
            }
            Item::Inline(child) => {
                out.push_str("... on ");
                out.push_str(child.on.first().copied().unwrap_or_default());
                out.push(' ');
                render_selection(child, out);
            }
        }
    }
    out.push_str(" }");
}

/// Fragment `Mr`: alles, was Ansicht, Sortierung, Pipeline-Link (E7) und
/// Checkout brauchen.
fn mr_fragment() -> Fragment {
    let mut items = scalars(
        "id iid title draft webUrl webPath updatedAt sourceBranch targetBranch \
         sourceBranchExists diffHeadSha",
    );
    items.push(Field::object("project", &["Project"], scalars("fullPath")).into());
    items.push(Field::object("sourceProject", &["Project"], scalars("fullPath")).into());
    items.push(Field::object("author", AUTHOR, scalars("username name")).into());
    let approver = Field::object("nodes", &["UserCore"], scalars("username name bot"));
    items.push(Field::object("approvedBy", &["UserCoreConnection"], vec![approver.into()]).into());
    items.extend(scalars("approvalsRequired approvalsLeft"));
    // Kein `detailedStatus`: manche Server liefern es leer, obwohl es
    // Pflichtfeld ist, und dann fällt die ganze `headPipeline` weg.
    items.push(Field::object("headPipeline", &["Pipeline"], scalars("status path")).into());
    items.extend(scalars(
        "resolvableDiscussionsCount resolvedDiscussionsCount detailedMergeStatus",
    ));
    let mut reviewer = scalars("username name");
    let interaction = Field::object(
        "mergeRequestInteraction",
        &["UserMergeRequestInteraction"],
        scalars("reviewState"),
    );
    reviewer.push(interaction.into());
    let nodes = Field::object("nodes", REVIEWER, reviewer);
    let reviewers = Field::object(
        "reviewers",
        &["MergeRequestReviewerConnection"],
        vec![nodes.into()],
    );
    items.push(reviewers.into());
    Fragment {
        name: MR_FRAGMENT,
        sel: Selection::new(&["MergeRequest"], items),
    }
}

fn page_info() -> Item {
    Field::object("pageInfo", &["PageInfo"], scalars("hasNextPage endCursor")).into()
}

/// `nodes { ...Mr }`
fn mr_nodes() -> Item {
    Field::object("nodes", &["MergeRequest"], vec![Item::Spread(MR_FRAGMENT)]).into()
}

/// Offene MRs anderer in einem Projekt; `after` = Name der Cursor-Variable.
fn project_mrs(after: Option<&str>) -> Field {
    let mut items = scalars("count");
    items.extend([page_info(), mr_nodes()]);
    let mut field = Field::object("mergeRequests", &["MergeRequestConnection"], items)
        .arg("state", "opened")
        .arg("first", UNINVOLVED_PAGE_SIZE);
    if let Some(var) = after {
        field = field.arg("after", format!("${var}"));
    }
    field
        .opt_arg("sort", "UPDATED_DESC")
        .opt_arg("not", "{authorUsername: $me}")
}

/// Pfade ohne Dubletten; GitLab vergleicht ohne Groß-/Kleinschreibung.
fn unique_paths(paths: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .iter()
        .filter(|p| seen.insert(p.to_lowercase()))
        .cloned()
        .collect()
}

/// `DASHBOARD`: Server-Version, eigener Benutzer, eigene offene MRs und
/// offene Review-Anfragen (je `first: 50`).
pub fn dashboard() -> Query {
    let list = |alias: &str, name: &'static str| -> Item {
        Field::object(
            name,
            &["MergeRequestConnection"],
            vec![page_info(), mr_nodes()],
        )
        .alias(alias)
        .arg("state", "opened")
        .arg("first", PAGE_SIZE)
        .opt_arg("sort", "UPDATED_DESC")
        .into()
    };
    let mut user = scalars("username name");
    user.push(list("authored", "authoredMergeRequests"));
    user.push(list("reviewing", "reviewRequestedMergeRequests"));
    Query::new(
        "GmDashboard",
        vec![
            Field::object("metadata", &["Metadata"], scalars("version enterprise")).into(),
            Field::object("currentUser", CURRENT_USER, user).into(),
        ],
    )
    .fragment(mr_fragment())
}

/// `MENTIONS`: To-dos „erwähnt“/„direkt angesprochen“ (offen und erledigt,
/// denn Kommentieren erledigt sie); `after` = Cursor der nächsten Seite.
/// `note == null` heißt Erwähnung in der Beschreibung.
pub fn mentions(after: Option<&str>) -> Query {
    let mut target = scalars("state");
    target.push(Item::Spread(MR_FRAGMENT));
    let mut todo = scalars("id action");
    todo.push(Field::object("note", &["Note"], scalars("id")).into());
    let on_mr = Item::Inline(Selection::new(&["MergeRequest"], target));
    todo.push(Field::object("targetEntity", &["Todoable"], vec![on_mr]).into());
    let nodes = Field::object("nodes", &["Todo"], todo);
    let todos = Field::object(
        "todos",
        &["TodoConnection"],
        vec![page_info(), nodes.into()],
    )
    .opt_arg("type", "[MERGEREQUEST]")
    .opt_arg("action", "[mentioned, directly_addressed]")
    // Pflicht: ohne `state` nur offene To-dos, erledigte Erwähnungen fehlten
    // dann still (der Client kann das nicht nachfiltern)
    .arg("state", "[pending, done]")
    .opt_arg("sort", "CREATED_DESC")
    .arg("first", PAGE_SIZE)
    .arg("after", "$after");
    let mut user = scalars("username");
    user.push(todos.into());
    Query::new(
        "GmMentions",
        vec![Field::object("currentUser", CURRENT_USER, user).into()],
    )
    .var("after", "String", after.map(Value::from))
    .fragment(mr_fragment())
}

/// `DESCRIPTION_MENTIONS`: offene MRs der Projekte, in deren Beschreibung
/// `@me` steht, je Abfrage höchstens `UNINVOLVED_CHUNK` Pfade. Ergänzt die
/// To-dos, die feingranulare Tokens ohne „Todo: Read“ leer liefern. Die
/// Suche ist unscharf (auch `@me2`, `x@me.de`); `parse_description_mentions`
/// prüft die Beschreibung selbst. Ohne Pfade keine Abfrage.
pub fn description_mentions(paths: &[String], me: &str) -> Vec<Query> {
    unique_paths(paths)
        .chunks(UNINVOLVED_CHUNK)
        .map(|chunk| {
            let mut mr = scalars("description");
            mr.push(Item::Spread(MR_FRAGMENT));
            let mr_nodes = Field::object("nodes", &["MergeRequest"], mr);
            let mrs = Field::object(
                "mergeRequests",
                &["MergeRequestConnection"],
                vec![mr_nodes.into()],
            )
            .arg("state", "opened")
            // Ohne `search` (alter Server) die neuesten offenen MRs; der
            // Client prüft ohnehin jede Beschreibung
            .opt_arg("search", "$search")
            .opt_arg("in", "[DESCRIPTION]")
            .opt_arg("sort", "UPDATED_DESC")
            .arg("first", DESCRIPTION_MENTIONS_PAGE_SIZE);
            let mut project = scalars("fullPath");
            project.push(mrs.into());
            let nodes = Field::object("nodes", &["Project"], project);
            let projects = Field::object("projects", &["ProjectConnection"], vec![nodes.into()])
                .arg("fullPaths", "$paths")
                .arg("first", chunk.len());
            Query::new("GmDescriptionMentions", vec![projects.into()])
                .var("paths", "[String!]", Some(Value::from(chunk.to_vec())))
                .var("search", "String", Some(Value::from(format!("@{me}"))))
                .fragment(mr_fragment())
        })
        .collect()
}

/// `UNINVOLVED`: offene MRs anderer in den Projekten, je Abfrage höchstens
/// `UNINVOLVED_CHUNK` Pfade. Ohne Pfade keine Abfrage.
pub fn uninvolved(paths: &[String], me: &str) -> Vec<Query> {
    unique_paths(paths)
        .chunks(UNINVOLVED_CHUNK)
        .map(|chunk| {
            let mut project = scalars("fullPath");
            project.push(project_mrs(None).into());
            let nodes = Field::object("nodes", &["Project"], project);
            let projects = Field::object("projects", &["ProjectConnection"], vec![nodes.into()])
                .arg("fullPaths", "$paths")
                .arg("first", chunk.len());
            Query::new("GmUninvolved", vec![projects.into()])
                .var("paths", "[String!]", Some(Value::from(chunk.to_vec())))
                .var("me", "String", Some(Value::from(me)))
                .fragment(mr_fragment())
        })
        .collect()
}

/// `UNINVOLVED_PAGE`: nächste Seite je Projekt `(pfad, cursor)`, bis zu
/// `UNINVOLVED_PAGE_CHUNK` Projekte per Alias (`p0`, `p1`, …) je Abfrage;
/// mehr Projekte ergeben mehrere Abfragen (Komplexität).
pub fn uninvolved_page(pages: &[(String, String)], me: &str) -> Vec<Query> {
    pages
        .chunks(UNINVOLVED_PAGE_CHUNK)
        .map(|chunk| {
            let mut query = Query::new("GmUninvolvedPage", Vec::new());
            for (i, (path, cursor)) in chunk.iter().enumerate() {
                let (p, c) = (format!("p{i}"), format!("c{i}"));
                let mut items = scalars("fullPath");
                items.push(project_mrs(Some(&c)).into());
                let project = Field::object("project", &["Project"], items)
                    .alias(p.clone())
                    .arg("fullPath", format!("${p}"));
                query.root.items.push(project.into());
                query = query.var(p, "ID!", Some(Value::from(path.as_str()))).var(
                    c,
                    "String",
                    Some(Value::from(cursor.as_str())),
                );
            }
            query
                .var("me", "String", Some(Value::from(me)))
                .fragment(mr_fragment())
        })
        .collect()
}

/// `ACCESS`: Benutzer, Server-Version und Zugriff je Projekt, höchstens
/// `ACCESS_CHUNK` Pfade je Abfrage. Ohne Pfade eine Abfrage ohne
/// `projects` (leeres `fullPaths` hieße „alle Projekte“).
pub fn access(paths: &[String]) -> Vec<Query> {
    let build = |chunk: Option<&[String]>| {
        let mut root: Vec<Item> = vec![
            Field::object("currentUser", CURRENT_USER, scalars("username name bot")).into(),
            Field::object("metadata", &["Metadata"], scalars("version enterprise")).into(),
        ];
        let Some(chunk) = chunk else {
            return Query::new("GmAccess", root);
        };
        let mut project = scalars("fullPath webUrl archived mergeRequestsEnabled");
        let level = Field::object(
            "maxAccessLevel",
            &["AccessLevel"],
            scalars("integerValue stringValue"),
        );
        let permissions = Field::object(
            "userPermissions",
            &["ProjectPermissions"],
            scalars("readMergeRequest"),
        );
        project.extend([level.into(), permissions.into()]);
        let nodes = Field::object("nodes", &["Project"], project);
        let projects = Field::object("projects", &["ProjectConnection"], vec![nodes.into()])
            .arg("fullPaths", "$paths")
            .arg("first", chunk.len());
        root.push(projects.into());
        Query::new("GmAccess", root).var("paths", "[String!]", Some(Value::from(chunk.to_vec())))
    };
    let paths = unique_paths(paths);
    if paths.is_empty() {
        return vec![build(None)];
    }
    paths.chunks(ACCESS_CHUNK).map(|c| build(Some(c))).collect()
}

// --- Schema-Fallback ---------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SchemaErrorCode {
    /// Feld gibt es auf dem Typ nicht
    UndefinedField,
    /// Argument wird nicht angenommen
    ArgumentNotAccepted,
    /// Argumentwert passt nicht (z. B. unbekannter Enum-Wert)
    ArgumentLiteralsIncompatible,
}

/// Schema-Fehler aus `errors[].extensions`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaError {
    pub code: SchemaErrorCode,
    /// `typeName`; bei Argumentfehlern die Art (`Field`, `InputObject`)
    pub type_name: String,
    /// Feldname bzw. Argumentname
    pub name: String,
    /// Bei Argumentfehlern das Feld bzw. Input-Objekt, falls bekannt; bei
    /// einem Feld mit Alias nennt GitLab den Alias
    pub field: Option<String>,
}

impl SchemaError {
    /// Betrifft der Argumentfehler das Argument `arg` von Feld `field`
    /// (Alias `alias`)?
    fn hits_argument(&self, field: &str, alias: Option<&str>, arg: &Arg) -> bool {
        match self.type_name.as_str() {
            // nicht angenommener Schlüssel in einem Input-Objekt, z. B.
            // `authorUsername` in `not: {authorUsername: $me}`
            "InputObject" => {
                arg.value.starts_with('{') && arg.value.contains(&format!("{}:", self.name))
            }
            _ => {
                arg.name == self.name
                    && self
                        .field
                        .as_deref()
                        .is_none_or(|f| f == field || alias == Some(f))
            }
        }
    }
}

/// Schema-Fehler einer Antwort; andere Fehler werden übergangen.
pub fn schema_errors(json: &Value) -> Vec<SchemaError> {
    let Some(errors) = json.get("errors").and_then(Value::as_array) else {
        return Vec::new();
    };
    errors.iter().filter_map(schema_error).collect()
}

fn schema_error(err: &Value) -> Option<SchemaError> {
    let ext = err.get("extensions")?;
    let code = match ext.get("code")?.as_str()? {
        "undefinedField" => SchemaErrorCode::UndefinedField,
        "argumentNotAccepted" => SchemaErrorCode::ArgumentNotAccepted,
        "argumentLiteralsIncompatible" => SchemaErrorCode::ArgumentLiteralsIncompatible,
        _ => return None,
    };
    let (type_name, name, field) = match code {
        SchemaErrorCode::UndefinedField => (text(ext, "typeName")?, text(ext, "fieldName")?, None),
        SchemaErrorCode::ArgumentNotAccepted => (
            text(ext, "typeName")?,
            text(ext, "argumentName")?,
            text(ext, "name"),
        ),
        // ohne Feld in `extensions`: aus „Argument 'x' on Field 'y' …“
        SchemaErrorCode::ArgumentLiteralsIncompatible => (
            text(ext, "typeName").unwrap_or_default(),
            text(ext, "argumentName")?,
            err.get("message")
                .and_then(Value::as_str)
                .and_then(field_in_message),
        ),
    };
    Some(SchemaError {
        code,
        type_name,
        name,
        field,
    })
}

/// Feldname aus „… on Field 'todos' …“.
fn field_in_message(message: &str) -> Option<String> {
    let rest = message.split_once("on Field '")?.1;
    Some(rest.split_once('\'')?.0.to_string())
}

/// Gemerkte Reduktionen eines Hosts und die Server-Version dazu. Der
/// Service hält eine Instanz je Host über die Sitzung.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostSchema {
    /// `metadata.version`, zu der `removed` gehört
    pub version: Option<String>,
    pub removed: BTreeSet<SchemaError>,
}

impl HostSchema {
    /// Neue Server-Version: Gemerktes kann überholt sein und wird neu gelernt.
    fn observe_version(&mut self, version: &str) {
        if self.version.as_deref() == Some(version) {
            return;
        }
        if self.version.is_some() {
            self.removed.clear();
        }
        self.version = Some(version.to_string());
    }
}

/// Antwort mit `data`; `errors` = Meldungen neben `data` (Teilerfolg).
#[derive(Debug, Clone, PartialEq)]
pub struct GqlData {
    pub data: Value,
    pub errors: Vec<String>,
}

/// `glab api graphql -i -X POST -H "Content-Type: application/json"
/// --input -` mit `{query, variables}` auf stdin; Host nach E16. Den
/// Content-Type setzt glab < 1.123 bei `--input` nicht selbst (E18).
pub fn graphql_request(host_key: &str, query: &Query) -> GlabRequest {
    GlabRequest::api(
        host_key,
        [
            "graphql",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "--input",
            "-",
        ],
    )
    .with_stdin(StdinBody::Json(query.body()))
}

/// Führt eine Abfrage aus. Schema-Fehler entfernen die bemängelten Felder
/// bzw. Argumente, solange neue kommen, höchstens `MAX_ATTEMPTS` Versuche;
/// das Gelernte landet in `schema`. Ohne `data` → Fehlerart aus `classify`.
pub fn run_query(
    runner: &dyn GlabRunner,
    host_key: &str,
    schema: &mut HostSchema,
    query: Query,
) -> Result<GqlData, GlabErrorKind> {
    run_query_with(runner, host_key, schema, query, &|req| req)
}

/// Wie `run_query`; `prepare` ergänzt jeden Versuch (eigener Prozess) um
/// Kind-Env, z. B. `GLAB_API_PROTOCOL=http` (E17) oder `GITLAB_TOKEN` beim
/// Prüfen eines neuen Tokens (E3).
pub fn run_query_with(
    runner: &dyn GlabRunner,
    host_key: &str,
    schema: &mut HostSchema,
    mut query: Query,
    prepare: &dyn Fn(GlabRequest) -> GlabRequest,
) -> Result<GqlData, GlabErrorKind> {
    for err in &schema.removed {
        query.reduce(err);
    }
    let mut learned = Vec::new();
    let mut attempt = 1;
    let mut timeouts = 0;
    let (out, json) = loop {
        let out = match runner.run(prepare(graphql_request(host_key, &query))) {
            Ok(out) => out,
            Err(e) => {
                // Gelerntes gilt auch, wenn ein späterer Versuch scheitert
                schema.removed.extend(learned);
                return Err(e.kind());
            }
        };
        let json = out.json();
        if timeouts < TIMEOUT_RETRIES && json.as_ref().is_some_and(is_server_timeout) {
            timeouts += 1;
            std::thread::sleep(timeout_backoff(timeouts));
            continue;
        }
        if attempt < MAX_ATTEMPTS {
            let mut changed = false;
            for err in json.as_ref().map(schema_errors).unwrap_or_default() {
                if query.reduce(&err) {
                    changed = true;
                    learned.push(err);
                }
            }
            if changed {
                attempt += 1;
                continue;
            }
        }
        break (out, json);
    };
    let result = gql_data(&out, json);
    if let Ok(got) = &result {
        if let Some(version) = got
            .data
            .pointer("/metadata/version")
            .and_then(Value::as_str)
        {
            schema.observe_version(version);
        }
    }
    schema.removed.extend(learned);
    result
}

/// Antwort ohne Daten, deren Fehler alle Zeitüberschreitungen des Servers
/// sind („Timeout on validation of query“, „Timeout on …“): bei Last
/// vorübergehend, eine Wiederholung lohnt.
fn is_server_timeout(json: &Value) -> bool {
    let has_data = json.get("data").is_some_and(|d| !d.is_null());
    let messages = graphql_messages(json);
    !has_data && !messages.is_empty() && messages.iter().all(|m| m.starts_with("Timeout on "))
}

/// `data` aus der Antwort; ohne `data` oder ab HTTP 400 die Fehlerart.
fn gql_data(out: &GlabOutput, json: Option<Value>) -> Result<GqlData, GlabErrorKind> {
    let failed = out.http.as_ref().is_some_and(|h| h.code >= 400);
    let Some(mut json) = json.filter(|_| !failed) else {
        return Err(classify(out).unwrap_or(GlabErrorKind::Other));
    };
    match json.get_mut("data").map(Value::take) {
        Some(data) if data.is_object() => Ok(GqlData {
            errors: graphql_messages(&json),
            data,
        }),
        _ => Err(classify(out).unwrap_or(GlabErrorKind::Other)),
    }
}

// --- Rohmodelle und Parser ---------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserNode {
    pub username: Option<String>,
    pub name: Option<String>,
    pub bot: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReviewerNode {
    pub username: Option<String>,
    pub name: Option<String>,
    /// `MergeRequestReviewState` (`UNREVIEWED`, `REVIEW_STARTED`, …)
    pub review_state: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipelineNode {
    /// `PipelineStatusEnum` (`SUCCESS`, `RUNNING`, …)
    pub status: Option<String>,
    pub path: Option<String>,
}

/// MR aus dem Fragment `Mr`; jedes Feld kann fehlen (Schema-Fallback,
/// Rechte, alte GitLab-Versionen).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MrNode {
    /// Globale ID (`gid://gitlab/MergeRequest/…`), Schlüssel zum Zusammenführen
    pub id: Option<String>,
    pub iid: Option<u64>,
    pub title: Option<String>,
    pub draft: Option<bool>,
    pub web_url: Option<String>,
    pub web_path: Option<String>,
    pub updated_at: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
    pub source_branch_exists: Option<bool>,
    pub diff_head_sha: Option<String>,
    pub project_path: Option<String>,
    pub source_project_path: Option<String>,
    pub author: Option<UserNode>,
    pub approved_by: Option<Vec<UserNode>>,
    /// Nur EE
    pub approvals_required: Option<u32>,
    /// Nur EE
    pub approvals_left: Option<u32>,
    /// `None` = keine Pipeline (oder Feld nicht abfragbar)
    pub head_pipeline: Option<PipelineNode>,
    pub resolvable_discussions: Option<u32>,
    pub resolved_discussions: Option<u32>,
    pub detailed_merge_status: Option<String>,
    pub reviewers: Option<Vec<ReviewerNode>>,
    /// Nur bei Erwähnungen (`opened`, `closed`, `merged`, …)
    pub state: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dashboard {
    pub version: Option<String>,
    pub enterprise: Option<bool>,
    /// `None` bei `currentUser: null` (nicht angemeldet)
    pub me: Option<UserNode>,
    pub authored: Vec<MrNode>,
    pub authored_page: Option<PageInfo>,
    pub reviewing: Vec<MrNode>,
    pub reviewing_page: Option<PageInfo>,
}

/// Wo erwähnt wurde: `note == null` → Beschreibung, sonst Kommentar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MentionKind {
    Description,
    Comment,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MentionNode {
    pub todo_id: Option<String>,
    /// `mentioned` oder `directly_addressed`
    pub action: Option<String>,
    /// `None`, wenn `note` nicht abgefragt werden konnte
    pub kind: Option<MentionKind>,
    pub mr: MrNode,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mentions {
    pub me: Option<UserNode>,
    /// Nur offene MRs; je Erwähnung ein Eintrag
    pub items: Vec<MentionNode>,
    pub page: Option<PageInfo>,
    /// Leere Seite, obwohl weitere folgen: GitLab blendet To-dos aus, die
    /// das Token nicht lesen darf (feingranular ohne „Todo: Read“)
    pub hidden: bool,
}

impl Mentions {
    /// Cursor der nächsten Seite, solange weniger als `MENTIONS_MAX_PAGES`
    /// geladen sind.
    pub fn next_cursor(&self, pages_loaded: usize) -> Option<&str> {
        let page = self.page.as_ref().filter(|p| p.has_next_page)?;
        (pages_loaded < MENTIONS_MAX_PAGES)
            .then_some(page.end_cursor.as_deref())
            .flatten()
    }
}

/// Offene MRs anderer in einem Projekt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectMrs {
    pub full_path: Option<String>,
    /// `count` laut GitLab
    pub count: Option<u32>,
    pub page: Option<PageInfo>,
    pub mrs: Vec<MrNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectNode {
    pub full_path: Option<String>,
    pub web_url: Option<String>,
    pub archived: Option<bool>,
    pub merge_requests_enabled: Option<bool>,
    /// `maxAccessLevel.integerValue` (0 = keine Mitgliedschaft)
    pub access_level: Option<u32>,
    /// `maxAccessLevel.stringValue` (`DEVELOPER`, `NO_ACCESS`, …)
    pub access_name: Option<String>,
    pub read_merge_request: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    pub version: Option<String>,
    pub enterprise: Option<bool>,
    pub me: Option<UserNode>,
    pub projects: Vec<ProjectNode>,
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_string)
}

fn flag(v: &Value, key: &str) -> Option<bool> {
    v.get(key)?.as_bool()
}

fn number(v: &Value, key: &str) -> Option<u32> {
    u32::try_from(v.get(key)?.as_u64()?).ok()
}

/// Objekte unter `key.nodes`; `null`-Einträge (keine Rechte) fallen weg.
fn nodes<'a>(v: &'a Value, key: &str) -> Option<Vec<&'a Value>> {
    let nodes = v.get(key)?.get("nodes")?.as_array()?;
    Some(nodes.iter().filter(|n| n.is_object()).collect())
}

/// `key.pageInfo`
fn page(v: &Value, key: &str) -> Option<PageInfo> {
    let info = v.get(key)?.get("pageInfo")?;
    info.is_object().then(|| PageInfo {
        has_next_page: flag(info, "hasNextPage").unwrap_or(false),
        end_cursor: text(info, "endCursor"),
    })
}

/// `metadata { version enterprise }`
fn metadata(data: &Value) -> (Option<String>, Option<bool>) {
    let meta = data.get("metadata").unwrap_or(&Value::Null);
    (text(meta, "version"), flag(meta, "enterprise"))
}

fn parse_user(v: &Value) -> Option<UserNode> {
    v.is_object().then(|| UserNode {
        username: text(v, "username"),
        name: text(v, "name"),
        bot: flag(v, "bot"),
    })
}

/// Ohne `id` und `iid` kein MR (z. B. To-do zu einem Issue).
fn parse_mr(v: &Value) -> Option<MrNode> {
    let id = text(v, "id");
    // `iid` ist in GraphQL ein String
    let iid = v.get("iid").and_then(|iid| match iid {
        Value::String(s) => s.parse().ok(),
        other => other.as_u64(),
    });
    if id.is_none() && iid.is_none() {
        return None;
    }
    let path = |key: &str| v.get(key).and_then(|p| text(p, "fullPath"));
    let pipeline = v.get("headPipeline").filter(|p| p.is_object());
    Some(MrNode {
        id,
        iid,
        title: text(v, "title"),
        draft: flag(v, "draft"),
        web_url: text(v, "webUrl"),
        web_path: text(v, "webPath"),
        updated_at: text(v, "updatedAt"),
        source_branch: text(v, "sourceBranch"),
        target_branch: text(v, "targetBranch"),
        source_branch_exists: flag(v, "sourceBranchExists"),
        diff_head_sha: text(v, "diffHeadSha"),
        project_path: path("project"),
        source_project_path: path("sourceProject"),
        author: v.get("author").and_then(parse_user),
        approved_by: nodes(v, "approvedBy").map(|n| n.into_iter().filter_map(parse_user).collect()),
        approvals_required: number(v, "approvalsRequired"),
        approvals_left: number(v, "approvalsLeft"),
        head_pipeline: pipeline.map(|p| PipelineNode {
            status: text(p, "status"),
            path: text(p, "path"),
        }),
        resolvable_discussions: number(v, "resolvableDiscussionsCount"),
        resolved_discussions: number(v, "resolvedDiscussionsCount"),
        detailed_merge_status: text(v, "detailedMergeStatus"),
        reviewers: nodes(v, "reviewers").map(|n| {
            n.into_iter()
                .map(|r| ReviewerNode {
                    username: text(r, "username"),
                    name: text(r, "name"),
                    review_state: r
                        .get("mergeRequestInteraction")
                        .and_then(|i| text(i, "reviewState")),
                })
                .collect()
        }),
        state: text(v, "state"),
    })
}

fn mr_list(v: &Value, key: &str) -> Vec<MrNode> {
    nodes(v, key)
        .unwrap_or_default()
        .into_iter()
        .filter_map(parse_mr)
        .collect()
}

pub fn parse_dashboard(data: &Value) -> Dashboard {
    let (version, enterprise) = metadata(data);
    let user = data.get("currentUser").unwrap_or(&Value::Null);
    Dashboard {
        version,
        enterprise,
        me: parse_user(user),
        authored: mr_list(user, "authored"),
        authored_page: page(user, "authored"),
        reviewing: mr_list(user, "reviewing"),
        reviewing_page: page(user, "reviewing"),
    }
}

/// Nur offene MRs; nach entfernten Argumenten (Schema-Fallback) zusätzlich
/// nur MRs und nur „erwähnt“/„direkt angesprochen“.
pub fn parse_mentions(data: &Value) -> Mentions {
    const ACTIONS: [&str; 2] = ["mentioned", "directly_addressed"];
    let user = data.get("currentUser").unwrap_or(&Value::Null);
    let items = nodes(user, "todos")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|todo| {
            let action = text(todo, "action");
            if action.as_deref().is_some_and(|a| !ACTIONS.contains(&a)) {
                return None;
            }
            let mr = parse_mr(todo.get("targetEntity")?)?;
            if mr.state.as_deref().is_some_and(|s| s != "opened") {
                return None;
            }
            let kind = todo.get("note").map(|note| {
                if note.is_null() {
                    MentionKind::Description
                } else {
                    MentionKind::Comment
                }
            });
            Some(MentionNode {
                todo_id: text(todo, "id"),
                action,
                kind,
                mr,
            })
        })
        .collect();
    let page = page(user, "todos");
    let hidden = nodes(user, "todos").is_some_and(|n| n.is_empty())
        && page.as_ref().is_some_and(|p| p.has_next_page);
    Mentions {
        me: parse_user(user),
        items,
        page,
        hidden,
    }
}

/// Antwort auf `description_mentions`: nur MRs, deren Beschreibung `me`
/// wirklich erwähnt; ohne Beschreibung (Feld entfallen) kein Treffer.
pub fn parse_description_mentions(data: &Value, me: &str) -> Vec<MentionNode> {
    let mut found = Vec::new();
    for project in nodes(data, "projects").unwrap_or_default() {
        let full_path = text(project, "fullPath");
        for node in nodes(project, "mergeRequests").unwrap_or_default() {
            let described = node.get("description").and_then(Value::as_str);
            if !described.is_some_and(|d| mentions_user(d, me)) {
                continue;
            }
            let Some(mut mr) = parse_mr(node) else {
                continue;
            };
            if mr.project_path.is_none() {
                mr.project_path.clone_from(&full_path);
            }
            found.push(MentionNode {
                kind: Some(MentionKind::Description),
                mr,
                ..MentionNode::default()
            });
        }
    }
    found
}

/// `@user` als Erwähnung im Text, ohne Groß-/Kleinschreibung: davor kein
/// Namens- oder Mail-Zeichen, danach kein weiteres Namenszeichen (ein Punkt
/// am Satzende gehört nicht zum Namen).
pub fn mentions_user(text: &str, user: &str) -> bool {
    if user.is_empty() {
        return false;
    }
    let name_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let needle = format!("@{}", user.to_ascii_lowercase());
    // Nur ASCII wird umgewandelt: die Byte-Positionen bleiben gleich
    let text = text.to_ascii_lowercase();
    text.match_indices(&needle).any(|(start, m)| {
        let free_before = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !name_char(c) && c != '.' && c != '@');
        let mut after = text[start + m.len()..].chars();
        let free_after = match after.next() {
            None => true,
            Some('.') => after.next().is_none_or(|c| !name_char(c) && c != '.'),
            Some(c) => !name_char(c),
        };
        free_before && free_after
    })
}

/// Ohne eigene MRs (falls `not` entfallen ist).
pub fn parse_uninvolved(data: &Value, me: &str) -> Vec<ProjectMrs> {
    nodes(data, "projects")
        .unwrap_or_default()
        .into_iter()
        .map(|project| parse_project_mrs(project, me))
        .collect()
}

/// Antwort auf `uninvolved_page`: Projekte in Alias-Reihenfolge; ein
/// verschwundenes Projekt (`null`) fällt weg.
pub fn parse_uninvolved_page(data: &Value, me: &str) -> Vec<ProjectMrs> {
    (0..)
        .map_while(|i| data.get(format!("p{i}")))
        .filter(|project| project.is_object())
        .map(|project| parse_project_mrs(project, me))
        .collect()
}

fn parse_project_mrs(project: &Value, me: &str) -> ProjectMrs {
    let full_path = text(project, "fullPath");
    let mine = |mr: &MrNode| {
        mr.author
            .as_ref()
            .and_then(|a| a.username.as_deref())
            .is_some_and(|u| u.eq_ignore_ascii_case(me))
    };
    let mrs = mr_list(project, "mergeRequests")
        .into_iter()
        .filter(|mr| !mine(mr))
        .map(|mut mr| {
            if mr.project_path.is_none() {
                mr.project_path = full_path.clone();
            }
            mr
        })
        .collect();
    let list = project.get("mergeRequests").unwrap_or(&Value::Null);
    ProjectMrs {
        full_path,
        count: number(list, "count"),
        page: page(project, "mergeRequests"),
        mrs,
    }
}

pub fn parse_access(data: &Value) -> Access {
    let (version, enterprise) = metadata(data);
    let projects = nodes(data, "projects")
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let level = p.get("maxAccessLevel").unwrap_or(&Value::Null);
            ProjectNode {
                full_path: text(p, "fullPath"),
                web_url: text(p, "webUrl"),
                archived: flag(p, "archived"),
                merge_requests_enabled: flag(p, "mergeRequestsEnabled"),
                access_level: number(level, "integerValue"),
                access_name: text(level, "stringValue"),
                read_merge_request: p
                    .get("userPermissions")
                    .and_then(|u| flag(u, "readMergeRequest")),
            }
        })
        .collect();
    Access {
        version,
        enterprise,
        me: data.get("currentUser").and_then(parse_user),
        projects,
    }
}

/// Angefragte Pfade ohne Knoten in der Antwort: GitLab liefert für fehlende
/// und nicht sichtbare Projekte nichts (Vergleich ohne Groß-/Kleinschreibung).
// Nur in Tests genutzt.
#[cfg_attr(not(test), allow(dead_code))]
pub fn missing_paths(requested: &[String], found: &[ProjectNode]) -> Vec<String> {
    let found: HashSet<String> = found
        .iter()
        .filter_map(|p| p.full_path.as_deref())
        .map(str::to_lowercase)
        .collect();
    requested
        .iter()
        .filter(|p| !found.contains(&p.to_lowercase()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab::glab::fake::FakeCall;
    use crate::gitlab::glab::{
        EnvValue, FakeGlab, FakeReply, GatedRunner, GlabError, GlabOutput, HttpHead, StdinBody,
    };
    use std::time::Duration;

    const HOST: &str = "gitlab.example.com";
    const ME: &str = "taltmann";

    const DASHBOARD_EE: &str = include_str!("fixtures/dashboard_ee.json");
    const DASHBOARD_CE: &str = include_str!("fixtures/dashboard_ce.json");
    const DASHBOARD_CE_UNDEFINED: &str = include_str!("fixtures/dashboard_ce_undefined.json");
    const CURRENT_USER_NULL: &str = include_str!("fixtures/current_user_null.json");
    const MENTIONS: &str = include_str!("fixtures/mentions.json");
    const UNINVOLVED: &str = include_str!("fixtures/uninvolved.json");
    const ACCESS: &str = include_str!("fixtures/access.json");
    const UNDEFINED_ROUND1: &str = include_str!("fixtures/undefined_round1.json");
    const UNDEFINED_ROUND2: &str = include_str!("fixtures/undefined_round2.json");
    const ARGUMENT_NOT_ACCEPTED: &str = include_str!("fixtures/argument_not_accepted.json");
    const SAME_FIELD_TWO_TYPES: &str = include_str!("fixtures/same_field_two_types.json");

    fn json(fixture: &str) -> Value {
        serde_json::from_str(fixture).expect("Fixture ist gültiges JSON")
    }

    fn data(fixture: &str) -> Value {
        json(fixture)["data"].clone()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// Antwort von `glab api graphql -i`: HTTP 200, bei `errors` Exit 1.
    fn reply(body: &str) -> FakeReply {
        let failed = body.contains("\"errors\"");
        FakeReply::output(GlabOutput {
            exit: i32::from(failed),
            http: Some(HttpHead {
                code: 200,
                reason: "OK".to_string(),
            }),
            body: body.to_string(),
            stderr_tail: if failed {
                "glab: GraphQL-Fehler".to_string()
            } else {
                String::new()
            },
        })
    }

    fn gated() -> GatedRunner<FakeGlab> {
        GatedRunner::new(FakeGlab::new())
    }

    /// Abfragetext und Variablen eines protokollierten Aufrufs.
    fn sent(call: &FakeCall) -> (String, Value) {
        let Some(StdinBody::Json(body)) = &call.request.stdin else {
            panic!("kein JSON auf stdin");
        };
        let body: Value = serde_json::from_str(body).expect("Body ist JSON");
        let query = body["query"].as_str().expect("query").to_string();
        (query, body["variables"].clone())
    }

    fn compact(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn removed(type_name: &str, name: &str) -> SchemaError {
        SchemaError {
            code: SchemaErrorCode::UndefinedField,
            type_name: type_name.to_string(),
            name: name.to_string(),
            field: None,
        }
    }

    // --- Step 2: Parser ------------------------------------------------

    #[test]
    fn parse_dashboard_ee() {
        let dash = parse_dashboard(&data(DASHBOARD_EE));
        assert_eq!(dash.version.as_deref(), Some("18.5.1-ee"));
        assert_eq!(dash.enterprise, Some(true));
        let me = dash.me.expect("angemeldet");
        assert_eq!(me.username.as_deref(), Some(ME));
        assert_eq!(me.name.as_deref(), Some("Tim Altmann"));
        assert_eq!(dash.authored.len(), 3);
        assert_eq!(dash.reviewing.len(), 2);
        assert_eq!(
            dash.authored_page,
            Some(PageInfo {
                has_next_page: false,
                end_cursor: Some("eyJpZCI6IjUwMzA1In0".to_string()),
            })
        );
        assert_eq!(
            dash.reviewing_page.as_ref().map(|p| p.has_next_page),
            Some(true)
        );

        let mr = &dash.authored[0];
        let want = MrNode {
            id: Some("gid://gitlab/MergeRequest/50412".to_string()),
            iid: Some(412),
            title: Some("Rechnungsexport als CSV ergänzen".to_string()),
            draft: Some(false),
            web_url: Some(
                "https://gitlab.example.com/kern/backend-api/-/merge_requests/412".to_string(),
            ),
            web_path: Some("/kern/backend-api/-/merge_requests/412".to_string()),
            updated_at: Some("2026-10-17T09:35:12Z".to_string()),
            source_branch: Some("feature/csv-export".to_string()),
            target_branch: Some("main".to_string()),
            source_branch_exists: Some(true),
            diff_head_sha: Some("3f2a9c1d7e5b4a8f9c0d1e2f3a4b5c6d7e8f9a0b".to_string()),
            project_path: Some("kern/backend-api".to_string()),
            source_project_path: Some("kern/backend-api".to_string()),
            author: Some(UserNode {
                username: Some(ME.to_string()),
                name: Some("Tim Altmann".to_string()),
                bot: None,
            }),
            approved_by: Some(vec![
                UserNode {
                    username: Some("mbecker".to_string()),
                    name: Some("Miriam Becker".to_string()),
                    bot: Some(false),
                },
                UserNode {
                    username: Some("syilmaz".to_string()),
                    name: Some("Sara Yilmaz".to_string()),
                    bot: Some(false),
                },
            ]),
            approvals_required: Some(2),
            approvals_left: Some(0),
            head_pipeline: Some(PipelineNode {
                status: Some("SUCCESS".to_string()),
                path: Some("/kern/backend-api/-/pipelines/9001".to_string()),
            }),
            resolvable_discussions: Some(3),
            resolved_discussions: Some(3),
            detailed_merge_status: Some("MERGEABLE".to_string()),
            reviewers: Some(vec![
                ReviewerNode {
                    username: Some("mbecker".to_string()),
                    name: Some("Miriam Becker".to_string()),
                    review_state: Some("APPROVED".to_string()),
                },
                ReviewerNode {
                    username: Some("syilmaz".to_string()),
                    name: Some("Sara Yilmaz".to_string()),
                    review_state: Some("APPROVED".to_string()),
                },
            ]),
            state: None,
        };
        assert_eq!(mr, &want);

        // Review-Anfrage mit eigenem Status; Fork-MR erkennbar am Quellprojekt
        let review = &dash.reviewing[0];
        assert_eq!(review.iid, Some(421));
        let mine = review.reviewers.as_ref().expect("reviewers")[0].clone();
        assert_eq!(mine.username.as_deref(), Some(ME));
        assert_eq!(mine.review_state.as_deref(), Some("UNREVIEWED"));
        let fork = &dash.reviewing[1];
        assert_eq!(fork.project_path.as_deref(), Some("web/kundenportal"));
        assert_eq!(
            fork.source_project_path.as_deref(),
            Some("jkrueger/kundenportal")
        );
    }

    #[test]
    fn parse_bot_approval_and_missing_pipeline() {
        let dash = parse_dashboard(&data(DASHBOARD_EE));
        let mr = dash
            .authored
            .iter()
            .find(|m| m.iid == Some(305))
            .expect("!305");
        let approvers = mr.approved_by.as_ref().expect("approvedBy");
        assert_eq!(approvers.len(), 1);
        assert_eq!(approvers[0].username.as_deref(), Some("renovate-bot"));
        assert_eq!(approvers[0].bot, Some(true));
        assert_eq!(mr.approvals_required, Some(0));
        // `headPipeline: null` = keine Pipeline
        assert_eq!(mr.head_pipeline, None);
        assert_eq!(mr.reviewers, Some(Vec::new()));
    }

    #[test]
    fn parse_dashboard_ce_without_approval_fields() {
        let dash = parse_dashboard(&data(DASHBOARD_CE));
        assert_eq!(dash.enterprise, Some(false));
        assert!(dash.reviewing.is_empty());
        assert_eq!(
            dash.reviewing_page,
            Some(PageInfo {
                has_next_page: false,
                end_cursor: None,
            })
        );
        let mr = &dash.authored[0];
        assert_eq!(mr.iid, Some(19));
        assert_eq!(mr.draft, Some(true));
        assert_eq!(mr.approvals_required, None);
        assert_eq!(mr.approvals_left, None);
        assert_eq!(mr.approved_by, Some(Vec::new()));
        assert_eq!(mr.detailed_merge_status.as_deref(), Some("DRAFT_STATUS"));
    }

    #[test]
    fn parse_current_user_null() {
        let dash = parse_dashboard(&data(CURRENT_USER_NULL));
        assert_eq!(dash.me, None);
        assert!(dash.authored.is_empty() && dash.reviewing.is_empty());
        assert_eq!(dash.version.as_deref(), Some("18.5.1-ee"));
        // leeres bzw. fremdes `data` bricht nicht
        assert_eq!(parse_dashboard(&Value::Null), Dashboard::default());
    }

    #[test]
    fn parse_mentions_kinds_and_only_open() {
        let mentions = parse_mentions(&data(MENTIONS));
        assert_eq!(mentions.me.and_then(|m| m.username).as_deref(), Some(ME));
        assert_eq!(
            mentions.page,
            Some(PageInfo {
                has_next_page: true,
                end_cursor: Some("MTAwMjQ".to_string()),
            })
        );
        let got: Vec<(Option<&str>, Option<u64>, Option<MentionKind>)> = mentions
            .items
            .iter()
            .map(|m| (m.todo_id.as_deref(), m.mr.iid, m.kind))
            .collect();
        // geschlossen (!398) und gemergt (!401) fallen weg; !77 zweimal
        assert_eq!(
            got,
            vec![
                (
                    Some("gid://gitlab/Todo/7001"),
                    Some(77),
                    Some(MentionKind::Description)
                ),
                (
                    Some("gid://gitlab/Todo/7002"),
                    Some(310),
                    Some(MentionKind::Comment)
                ),
                (
                    Some("gid://gitlab/Todo/7005"),
                    Some(77),
                    Some(MentionKind::Comment)
                ),
            ]
        );
        assert_eq!(
            mentions.items[1].action.as_deref(),
            Some("directly_addressed")
        );
        assert_eq!(mentions.items[0].mr.state.as_deref(), Some("opened"));
        assert_eq!(
            mentions.items[0].mr.project_path.as_deref(),
            Some("plattform/auth-service")
        );
    }

    #[test]
    fn mentions_at_most_three_pages() {
        let mentions = parse_mentions(&data(MENTIONS));
        assert_eq!(mentions.next_cursor(1), Some("MTAwMjQ"));
        assert_eq!(
            mentions.next_cursor(MENTIONS_MAX_PAGES - 1),
            Some("MTAwMjQ")
        );
        assert_eq!(mentions.next_cursor(MENTIONS_MAX_PAGES), None);
        let last = Mentions {
            page: Some(PageInfo {
                has_next_page: false,
                end_cursor: Some("x".to_string()),
            }),
            ..Mentions::default()
        };
        assert_eq!(last.next_cursor(1), None);
    }

    #[test]
    fn parse_mentions_detects_hidden_todos() {
        let page = |nodes: Value, next: bool| {
            serde_json::json!({ "currentUser": { "username": ME, "todos": {
                "pageInfo": { "hasNextPage": next, "endCursor": "MTA" },
                "nodes": nodes
            } } })
        };
        // Token ohne „Todo: Read“: leere Seite, aber weitere gemeldet
        assert!(parse_mentions(&page(serde_json::json!([]), true)).hidden);
        assert!(parse_mentions(&page(serde_json::json!([null, null]), true)).hidden);
        // wirklich keine To-dos
        assert!(!parse_mentions(&page(serde_json::json!([]), false)).hidden);
        // To-dos da, aber keine zu einem MR
        let issue = serde_json::json!([{ "id": "t1", "action": "mentioned", "note": null, "targetEntity": {} }]);
        let mentions = parse_mentions(&page(issue, true));
        assert!(mentions.items.is_empty());
        assert!(!mentions.hidden);
    }

    /// MR einer `description_mentions`-Antwort (nur das Nötigste).
    fn described_mr(iid: u64, description: Option<&str>) -> Value {
        let mut mr = serde_json::json!({
            "id": format!("gid://gitlab/MergeRequest/{iid}"),
            "iid": iid.to_string(),
            "title": format!("MR {iid}"),
            "author": { "username": "alehmann", "name": "Anna Lehmann" }
        });
        if let Some(description) = description {
            mr["description"] = Value::from(description);
        }
        mr
    }

    #[test]
    fn parse_description_mentions_checks_description() {
        let data = serde_json::json!({ "projects": { "nodes": [
            { "fullPath": "web/kundenportal", "mergeRequests": { "nodes": [
                described_mr(1, Some("Bitte schauen, @taltmann.")),
                described_mr(2, Some("cc @taltmann2 und mail@taltmann.de")),
                described_mr(3, None)
            ] } },
            null,
            { "fullPath": "kern/backend-api", "mergeRequests": { "nodes": [
                described_mr(4, Some("Review: @TAltmann"))
            ] } }
        ] } });
        let found = parse_description_mentions(&data, ME);
        let got: Vec<(Option<u64>, Option<&str>, Option<MentionKind>)> = found
            .iter()
            .map(|m| (m.mr.iid, m.mr.project_path.as_deref(), m.kind))
            .collect();
        assert_eq!(
            got,
            vec![
                (
                    Some(1),
                    Some("web/kundenportal"),
                    Some(MentionKind::Description)
                ),
                (
                    Some(4),
                    Some("kern/backend-api"),
                    Some(MentionKind::Description)
                ),
            ]
        );
        assert!(found.iter().all(|m| m.todo_id.is_none()));
    }

    #[test]
    fn mentions_user_only_whole_names() {
        for (text, hit) in [
            ("@taltmann", true),
            ("cc @taltmann, @jkrueger", true),
            ("(@taltmann)", true),
            ("Danke @TAltmann.", true),
            ("@taltmann: bitte", true),
            ("Zeile\n@taltmann\n", true),
            ("@taltmann2 und @taltmann", true),
            ("@taltmann2", false),
            ("@taltmann-bot", false),
            ("@taltmann_x", false),
            ("@taltmann.dev", false),
            ("mail@taltmann.de", false),
            ("@@taltmann", false),
            ("ä@taltmann", false),
            ("taltmann", false),
            ("", false),
        ] {
            assert_eq!(mentions_user(text, ME), hit, "{text}");
        }
        assert!(mentions_user("Hallo @t.altmann!", "t.altmann"));
        assert!(!mentions_user("@ irgendwas", ""));
    }

    #[test]
    fn parse_mentions_filters_after_removed_arguments() {
        // Ohne `type`/`action`-Argument: andere Ziele und Aktionen nachfiltern;
        // ohne `note`-Feld bleibt die Art unbekannt.
        let data = serde_json::json!({
            "currentUser": { "todos": { "nodes": [
                { "id": "t1", "action": "assigned", "targetEntity": { "state": "opened", "id": "mr1", "iid": "1" } },
                { "id": "t2", "action": "mentioned", "targetEntity": { "state": "opened" } },
                { "id": "t3", "action": "mentioned", "targetEntity": { "state": "opened", "id": "mr3", "iid": "3" } },
                null
            ] } }
        });
        let mentions = parse_mentions(&data);
        assert_eq!(mentions.items.len(), 1);
        assert_eq!(mentions.items[0].todo_id.as_deref(), Some("t3"));
        assert_eq!(mentions.items[0].kind, None);
        assert_eq!(mentions.page, None);
    }

    #[test]
    fn parse_uninvolved_without_own_mrs() {
        let projects = parse_uninvolved(&data(UNINVOLVED), ME);
        assert_eq!(projects.len(), 2);
        let portal = &projects[0];
        assert_eq!(portal.full_path.as_deref(), Some("web/kundenportal"));
        assert_eq!(portal.count, Some(5));
        assert_eq!(
            portal.page.as_ref().and_then(|p| p.end_cursor.as_deref()),
            Some("eyJ1cGRhdGVkX2F0IjoiMjAyNi0xMC0xNyJ9")
        );
        // eigener MR (Autor in anderer Schreibweise) fällt weg
        let iids: Vec<Option<u64>> = portal.mrs.iter().map(|m| m.iid).collect();
        assert_eq!(iids, vec![Some(95)]);
        let api = &projects[1];
        assert_eq!(api.page.as_ref().map(|p| p.has_next_page), Some(false));
        let mr = &api.mrs[0];
        // Projekt fehlt im Knoten → aus dem Elternknoten
        assert_eq!(mr.project_path.as_deref(), Some("kern/backend-api"));
        let pipeline = mr.head_pipeline.as_ref().expect("Pipeline");
        assert_eq!(pipeline.status.as_deref(), Some("RUNNING"));
        assert_eq!(mr.source_branch_exists, None);
    }

    #[test]
    fn parse_access_public_archived_and_missing() {
        let access = parse_access(&data(ACCESS));
        assert_eq!(access.version.as_deref(), Some("18.5.1-ee"));
        let me = access.me.as_ref().expect("Benutzer");
        assert_eq!(me.bot, Some(false));
        assert_eq!(access.projects.len(), 3);
        let member = &access.projects[0];
        assert_eq!(member.access_level, Some(30));
        assert_eq!(member.access_name.as_deref(), Some("DEVELOPER"));
        assert_eq!(member.read_merge_request, Some(true));
        assert_eq!(
            member.web_url.as_deref(),
            Some("https://gitlab.example.com/Kern/Backend-API")
        );
        // öffentlich ohne Mitgliedschaft: Stufe 0, MRs trotzdem lesbar
        let public = &access.projects[1];
        assert_eq!(public.access_level, Some(0));
        assert_eq!(public.access_name.as_deref(), Some("NO_ACCESS"));
        assert_eq!(public.read_merge_request, Some(true));
        let archived = &access.projects[2];
        assert_eq!(archived.archived, Some(true));
        assert_eq!(archived.merge_requests_enabled, Some(false));

        let requested = strings(&[
            "kern/backend-api",
            "oss/public-lib",
            "alt/legacy-tool",
            "geheim/verschwunden",
        ]);
        assert_eq!(
            missing_paths(&requested, &access.projects),
            strings(&["geheim/verschwunden"])
        );
    }

    #[test]
    fn schema_errors_reads_codes() {
        let errs = schema_errors(&json(UNDEFINED_ROUND1));
        assert_eq!(
            errs,
            vec![
                removed("Metadata", "enterprise"),
                removed("MergeRequest", "webPath"),
                removed("MergeRequest", "approvalsRequired"),
                removed("MergeRequest", "approvalsLeft"),
                removed("MergeRequest", "detailedMergeStatus"),
            ]
        );
        assert_eq!(
            schema_errors(&json(ARGUMENT_NOT_ACCEPTED)),
            vec![SchemaError {
                code: SchemaErrorCode::ArgumentNotAccepted,
                type_name: "Field".to_string(),
                name: "not".to_string(),
                field: Some("mergeRequests".to_string()),
            }]
        );
        let literal = serde_json::json!({ "errors": [
            {
                "message": "Argument 'action' on Field 'todos' has an invalid value ([mentioned, directly_addressed]). Expected type '[TodoActionEnum!]'.",
                "extensions": { "code": "argumentLiteralsIncompatible", "typeName": "Field", "argumentName": "action" }
            },
            { "message": "Variable $me is declared by GmUninvolved but not used", "extensions": { "code": "variableNotUsed", "variableName": "me" } },
            { "message": "Internal server error" },
            "kaputt"
        ] });
        assert_eq!(
            schema_errors(&literal),
            vec![SchemaError {
                code: SchemaErrorCode::ArgumentLiteralsIncompatible,
                type_name: "Field".to_string(),
                name: "action".to_string(),
                field: Some("todos".to_string()),
            }]
        );
        assert!(schema_errors(&data(DASHBOARD_EE)).is_empty());
    }

    // --- Step 2: Schema-Fallback über FakeGlab --------------------------

    #[test]
    fn fallback_six_undefined_fields_in_two_rounds() {
        let runner = gated();
        runner.inner().push(reply(UNDEFINED_ROUND1));
        runner.inner().push(reply(UNDEFINED_ROUND2));
        runner.inner().push(reply(DASHBOARD_CE));
        let mut schema = HostSchema::default();
        let got = run_query(&runner, HOST, &mut schema, dashboard()).expect("Erfolg");
        let dash = parse_dashboard(&got.data);
        assert_eq!(dash.authored[0].iid, Some(19));
        assert!(got.errors.is_empty());

        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 3);
        let (first, _) = sent(&calls[0]);
        let (last, _) = sent(&calls[2]);
        for field in [
            "enterprise",
            "webPath",
            "approvalsRequired",
            "approvalsLeft",
            "detailedMergeStatus",
            "reviewState",
        ] {
            assert!(first.contains(field), "{field} fehlt anfangs");
            assert!(!last.contains(field), "{field} nicht entfernt");
        }
        // leere Auswahl entfernt den Elternknoten
        assert!(!last.contains("mergeRequestInteraction"), "{last}");
        assert!(last.contains("metadata { version }"), "{last}");
        assert!(
            last.contains("reviewers { nodes { username name } }"),
            "{last}"
        );
        assert!(last.contains("webUrl"), "{last}");

        assert_eq!(schema.removed.len(), 6);
        assert!(schema
            .removed
            .contains(&removed("UserMergeRequestInteraction", "reviewState")));
        assert_eq!(schema.version.as_deref(), Some("18.5.1"));

        // gemerkt je Host: der nächste Aufruf geht gleich reduziert raus
        runner.inner().push(reply(DASHBOARD_CE));
        run_query(&runner, HOST, &mut schema, dashboard()).expect("Erfolg");
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(sent(&calls[0]).0, last);
    }

    #[test]
    fn fallback_ce_drops_approval_fields() {
        let runner = gated();
        runner.inner().push(reply(DASHBOARD_CE_UNDEFINED));
        runner.inner().push(reply(DASHBOARD_CE));
        let mut schema = HostSchema::default();
        let got = run_query(&runner, HOST, &mut schema, dashboard()).expect("Erfolg");
        assert_eq!(parse_dashboard(&got.data).enterprise, Some(false));
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        let (text, _) = sent(&calls[1]);
        assert!(!text.contains("approvals"), "{text}");
        assert!(text.contains("approvedBy"), "{text}");
        // gelernte Felder gelten auch für andere Abfragen desselben Hosts
        runner.inner().push(reply(MENTIONS));
        run_query(&runner, HOST, &mut schema, mentions(None)).expect("Erfolg");
        let (text, _) = sent(&runner.inner().take_calls()[0]);
        assert!(!text.contains("approvalsLeft"), "{text}");
    }

    #[test]
    fn fallback_argument_not_accepted_for_not() {
        let runner = gated();
        runner.inner().push(reply(ARGUMENT_NOT_ACCEPTED));
        runner.inner().push(reply(UNINVOLVED));
        let mut schema = HostSchema::default();
        let paths = strings(&["web/kundenportal", "kern/backend-api"]);
        let mut queries = uninvolved(&paths, ME);
        assert_eq!(queries.len(), 1);
        let got = run_query(&runner, HOST, &mut schema, queries.remove(0)).expect("Erfolg");
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        let (first, vars) = sent(&calls[0]);
        assert!(first.contains("not: {authorUsername: $me}"), "{first}");
        assert!(first.contains("$me: String"), "{first}");
        assert_eq!(vars["me"], ME);
        // ohne `not` auch ohne `$me` (sonst „Variable declared but not used“)
        let (second, vars) = sent(&calls[1]);
        assert!(!second.contains("not:"), "{second}");
        assert!(!second.contains("$me"), "{second}");
        assert!(vars.get("me").is_none(), "{vars}");
        assert!(second.contains("sort: UPDATED_DESC"), "{second}");
        // der Client filtert eigene MRs nach
        let projects = parse_uninvolved(&got.data, ME);
        assert!(projects
            .iter()
            .flat_map(|p| &p.mrs)
            .all(|m| m.iid != Some(99)));
    }

    #[test]
    fn fallback_same_field_name_on_two_types() {
        let runner = gated();
        runner.inner().push(reply(SAME_FIELD_TWO_TYPES));
        runner.inner().push(reply(MENTIONS));
        let mut schema = HostSchema::default();
        run_query(&runner, HOST, &mut schema, mentions(None)).expect("Erfolg");
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        let (first, _) = sent(&calls[0]);
        let (second, _) = sent(&calls[1]);
        assert!(first.contains("note { id }"), "{first}");
        // `Note.id` weg (und damit `note`), `Todo.id` und `MergeRequest.id` bleiben
        assert!(!second.contains("note"), "{second}");
        assert!(
            second.contains("nodes { id action targetEntity"),
            "{second}"
        );
        assert!(
            second.contains("fragment Mr on MergeRequest { id iid"),
            "{second}"
        );
    }

    #[test]
    fn fallback_at_most_four_attempts() {
        let runner = gated();
        for field in ["title", "draft", "webUrl", "iid", "updatedAt"] {
            let body = serde_json::json!({ "errors": [{
                "message": format!("Field '{field}' doesn't exist on type 'MergeRequest'"),
                "extensions": { "code": "undefinedField", "typeName": "MergeRequest", "fieldName": field }
            }] });
            runner.inner().push(reply(&body.to_string()));
        }
        let mut schema = HostSchema::default();
        let got = run_query(&runner, HOST, &mut schema, dashboard());
        assert!(
            matches!(got, Err(GlabErrorKind::GraphQl(ref m)) if m[0].contains("'iid'")),
            "{got:?}"
        );
        assert_eq!(runner.inner().take_calls().len(), MAX_ATTEMPTS);
        assert_eq!(runner.inner().pending(), 1);
    }

    #[test]
    fn fallback_stops_without_new_reduction() {
        let runner = gated();
        let body = serde_json::json!({ "errors": [{
            "message": "Field 'bogus' doesn't exist on type 'MergeRequest'",
            "extensions": { "code": "undefinedField", "typeName": "MergeRequest", "fieldName": "bogus" }
        }] });
        runner.inner().push(reply(&body.to_string()));
        let mut schema = HostSchema::default();
        let got = run_query(&runner, HOST, &mut schema, dashboard());
        assert!(matches!(got, Err(GlabErrorKind::GraphQl(_))), "{got:?}");
        assert_eq!(runner.inner().take_calls().len(), 1);
        assert!(schema.removed.is_empty());
    }

    #[test]
    fn new_server_version_resets_reductions() {
        let runner = gated();
        let mut schema = HostSchema {
            version: Some("17.11.0-ee".to_string()),
            removed: [removed("MergeRequest", "webPath")].into_iter().collect(),
        };
        runner.inner().push(reply(DASHBOARD_EE));
        run_query(&runner, HOST, &mut schema, dashboard()).expect("Erfolg");
        let (text, _) = sent(&runner.inner().take_calls()[0]);
        assert!(!text.contains("webPath"), "{text}");
        assert_eq!(schema.version.as_deref(), Some("18.5.1-ee"));
        assert!(schema.removed.is_empty());
        // gleiche Version: nichts verwerfen
        schema.removed.insert(removed("MergeRequest", "webPath"));
        runner.inner().push(reply(DASHBOARD_EE));
        run_query(&runner, HOST, &mut schema, dashboard()).expect("Erfolg");
        assert_eq!(schema.removed.len(), 1);
    }

    #[test]
    fn run_query_classifies_failures_and_partial_success() {
        let runner = gated();
        let mut schema = HostSchema::default();
        runner.inner().push(FakeReply::http(
            401,
            r#"{"errors":[{"message":"Invalid token"}]}"#,
        ));
        assert_eq!(
            run_query(&runner, HOST, &mut schema, dashboard()),
            Err(GlabErrorKind::InvalidToken)
        );
        // Teilerfolg: `data` neben `errors` (glab endet mit 1)
        let partial = r#"{"data":{"currentUser":{"username":"taltmann","todos":null}},"errors":[{"message":"Todos sind gerade nicht verfügbar","path":["currentUser","todos"]}]}"#;
        runner.inner().push(reply(partial));
        let got = run_query(&runner, HOST, &mut schema, mentions(None)).expect("Teilerfolg");
        assert_eq!(got.errors, strings(&["Todos sind gerade nicht verfügbar"]));
        assert_eq!(
            parse_mentions(&got.data)
                .me
                .and_then(|m| m.username)
                .as_deref(),
            Some(ME)
        );
        // glab < 1.123 ohne Content-Type (E18)
        runner.inner().push(reply(
            r#"{"errors":[{"message":"Unexpected end of document","locations":[]}]}"#,
        ));
        assert_eq!(
            run_query(&runner, HOST, &mut schema, dashboard()),
            Err(GlabErrorKind::MissingContentType)
        );
    }

    #[test]
    fn run_query_retries_server_timeouts() {
        // Ausgelasteter Server: „Timeout on validation of query“ ohne Daten →
        // dieselbe Abfrage wird wiederholt (gitlab.firma.example)
        let timeout = r#"{"errors":[{"message":"Timeout on validation of query"}]}"#;
        let runner = gated();
        let mut schema = HostSchema::default();
        runner.inner().push(reply(timeout));
        runner.inner().push(reply(CURRENT_USER_NULL));
        let got = run_query(&runner, HOST, &mut schema, dashboard()).expect("zweiter Versuch");
        assert!(got.errors.is_empty());
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].request.args, calls[1].request.args);

        // Bleibt der Server zu langsam: nach TIMEOUT_RETRIES Wiederholungen
        // die Meldung des Servers
        for _ in 0..=TIMEOUT_RETRIES {
            runner.inner().push(reply(timeout));
        }
        assert_eq!(
            run_query(&runner, HOST, &mut schema, dashboard()),
            Err(GlabErrorKind::GraphQl(strings(&[
                "Timeout on validation of query"
            ])))
        );
        assert_eq!(runner.inner().take_calls().len(), TIMEOUT_RETRIES + 1);
        assert_eq!(runner.inner().pending(), 0);

        // Teilerfolg mit Timeout eines Feldes: Daten da, keine Wiederholung
        let partial = r#"{"data":{"currentUser":{"username":"taltmann","todos":null}},"errors":[{"message":"Timeout on CurrentUser.todos","path":["currentUser","todos"]}]}"#;
        runner.inner().push(reply(partial));
        let got = run_query(&runner, HOST, &mut schema, mentions(None)).expect("Teilerfolg");
        assert_eq!(got.errors, strings(&["Timeout on CurrentUser.todos"]));
        assert_eq!(runner.inner().take_calls().len(), 1);
    }

    #[test]
    fn graphql_calls_send_json_header_and_include() {
        let runner = gated();
        let paths = strings(&["kern/backend-api"]);
        let pages = vec![("kern/backend-api".to_string(), "abc".to_string())];
        let mut queries = vec![dashboard(), mentions(Some("MTA"))];
        queries.extend(uninvolved(&paths, ME));
        queries.extend(uninvolved_page(&pages, ME));
        queries.extend(access(&paths));
        assert_eq!(queries.len(), 5);
        let mut schema = HostSchema::default();
        for query in queries {
            runner.inner().push(reply(CURRENT_USER_NULL));
            run_query(&runner, HOST, &mut schema, query).expect("Erfolg");
        }
        // Host mit Port über `GITLAB_HOST` (E16)
        runner.inner().push(reply(CURRENT_USER_NULL));
        run_query(&runner, "git.intern:8443", &mut schema, dashboard()).expect("Erfolg");

        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 6);
        for call in &calls {
            let args: Vec<&str> = call.request.args.iter().map(String::as_str).collect();
            assert_eq!(args[0], "api");
            assert!(args.contains(&"graphql"), "{args:?}");
            assert!(args.contains(&"-i"), "{args:?}");
            assert!(
                args.windows(2)
                    .any(|w| w == ["-H", "Content-Type: application/json"]),
                "{args:?}"
            );
            assert!(args.windows(2).any(|w| w == ["-X", "POST"]), "{args:?}");
            assert!(args.windows(2).any(|w| w == ["--input", "-"]), "{args:?}");
            let (text, _) = sent(call);
            assert!(text.starts_with("query Gm"), "{text}");
        }
        let args = &calls[0].request.args;
        assert!(
            args.windows(2).any(|w| w == ["--hostname", HOST]),
            "{args:?}"
        );
        let last = &calls[5];
        assert_eq!(last.request.host_key.as_deref(), Some("git.intern:8443"));
        assert_eq!(
            last.env.get("GITLAB_HOST").map(String::as_str),
            Some("git.intern:8443")
        );
        assert!(!last.request.args.iter().any(|a| a == "--hostname"));
        // Cursor als Variable, nicht im Text
        let (text, vars) = sent(&calls[1]);
        assert!(text.contains("after: $after"), "{text}");
        assert_eq!(vars["after"], "MTA");
    }

    // --- Step 3: Größe und Blöcke ---------------------------------------

    #[test]
    fn query_text_stays_under_limit() {
        let long: Vec<String> = (0..ACCESS_CHUNK)
            .map(|i| format!("gruppe-mit-langem-namen/untergruppe/projekt-{i:03}"))
            .collect();
        let pages: Vec<(String, String)> = long
            .iter()
            .take(UNINVOLVED_PAGE_CHUNK)
            .map(|p| (p.clone(), "eyJpZCI6IjEyMzQ1Njc4OSJ9".to_string()))
            .collect();
        let mut queries = vec![dashboard(), mentions(Some("MTAwMjQ"))];
        queries.extend(uninvolved(&long[..UNINVOLVED_CHUNK], ME));
        queries.extend(description_mentions(&long[..UNINVOLVED_CHUNK], ME));
        queries.extend(uninvolved_page(&pages, ME));
        queries.extend(access(&long));
        assert_eq!(queries.len(), 6);
        for query in &queries {
            let text = compact(&query.text());
            assert!(
                text.chars().count() < MAX_QUERY_CHARS,
                "{}: {} Zeichen",
                query.name,
                text.len()
            );
            // Pfade nur als Variable, nie im Text
            assert!(!text.contains("projekt-0"), "{text}");
        }
    }

    #[test]
    fn full_paths_in_chunks() {
        let mut paths: Vec<String> = (0..60).map(|i| format!("g/p{i}")).collect();
        paths.push("G/P3".to_string()); // doppelt, andere Schreibweise
        let chunks = |queries: &[Query]| -> Vec<Vec<String>> {
            queries
                .iter()
                .map(|q| {
                    q.variables["paths"]
                        .as_array()
                        .expect("paths")
                        .iter()
                        .map(|p| p.as_str().expect("Pfad").to_string())
                        .collect()
                })
                .collect()
        };
        let unique: Vec<String> = paths[..60].to_vec();

        let queries = uninvolved(&paths, ME);
        let parts = chunks(&queries);
        assert_eq!(
            parts.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![25, 25, 10]
        );
        assert_eq!(parts.concat(), unique);
        for (query, part) in queries.iter().zip(&parts) {
            let text = query.text();
            let first = format!("projects(fullPaths: $paths, first: {})", part.len());
            assert!(text.contains(&first), "{text}");
        }

        let queries = access(&paths);
        let parts = chunks(&queries);
        assert_eq!(parts.iter().map(Vec::len).collect::<Vec<_>>(), vec![50, 10]);
        assert_eq!(parts.concat(), unique);

        // ohne Pfade: kein Aufruf bzw. Zugriff ohne `projects`
        assert!(uninvolved(&[], ME).is_empty());
        assert!(uninvolved_page(&[], ME).is_empty());
        let empty = access(&[]);
        assert_eq!(empty.len(), 1);
        let text = empty[0].text();
        assert!(!text.contains("projects"), "{text}");
        assert!(!text.contains("$paths"), "{text}");
        assert!(text.contains("currentUser { username name bot }"), "{text}");

        // Folgeseiten: jeder Alias zählt voll zur Komplexität, daher kleine Blöcke
        let pages: Vec<(String, String)> = (0..30)
            .map(|i| (format!("g/p{i}"), format!("c{i}")))
            .collect();
        let queries = uninvolved_page(&pages, ME);
        assert_eq!(queries.len(), 10);
        for query in &queries {
            assert_eq!(query.root.items.len(), UNINVOLVED_PAGE_CHUNK);
            assert_eq!(query.text().matches(": project(").count(), 3);
        }
        assert_eq!(queries[9].variables["p2"], "g/p29");
    }

    #[test]
    fn dashboard_text_contains_mr_fragment() {
        let text = dashboard().text();
        assert!(text.starts_with("query GmDashboard { metadata { version enterprise } currentUser { username name authored: authoredMergeRequests(state: opened, first: 50, sort: UPDATED_DESC) { pageInfo { hasNextPage endCursor } nodes { ...Mr } } reviewing: reviewRequestedMergeRequests("), "{text}");
        let fragment = "fragment Mr on MergeRequest { id iid title draft webUrl webPath updatedAt sourceBranch targetBranch sourceBranchExists diffHeadSha project { fullPath } sourceProject { fullPath } author { username name } approvedBy { nodes { username name bot } } approvalsRequired approvalsLeft headPipeline { status path } resolvableDiscussionsCount resolvedDiscussionsCount detailedMergeStatus reviewers { nodes { username name mergeRequestInteraction { reviewState } } } }";
        assert!(text.ends_with(fragment), "{text}");
        assert_eq!(text.matches("...Mr").count(), 2);
        assert_eq!(text.matches("fragment Mr").count(), 1);
    }

    #[test]
    fn mentions_text_and_variables() {
        let query = mentions(None);
        let text = query.text();
        assert!(text.starts_with("query GmMentions($after: String) { currentUser { username todos(type: [MERGEREQUEST], action: [mentioned, directly_addressed], state: [pending, done], sort: CREATED_DESC, first: 50, after: $after) { pageInfo { hasNextPage endCursor } nodes { id action note { id } targetEntity { ... on MergeRequest { state ...Mr } } } } } }"), "{text}");
        let body: Value = serde_json::from_str(&query.body()).expect("JSON");
        assert_eq!(body["query"], text);
        assert_eq!(body["variables"], serde_json::json!({}));
    }

    #[test]
    fn description_mentions_text_and_variables() {
        let paths: Vec<String> = (0..30).map(|i| format!("g/p{i}")).collect();
        let queries = description_mentions(&paths, ME);
        assert_eq!(queries.len(), 2);
        let text = queries[0].text();
        assert!(text.starts_with("query GmDescriptionMentions($paths: [String!], $search: String) { projects(fullPaths: $paths, first: 25) { nodes { fullPath mergeRequests(state: opened, search: $search, in: [DESCRIPTION], sort: UPDATED_DESC, first: 20) { nodes { description ...Mr } } } } }"), "{text}");
        assert!(text.contains("fragment Mr on MergeRequest"), "{text}");
        let body: Value = serde_json::from_str(&queries[1].body()).expect("JSON");
        assert_eq!(body["variables"]["search"], "@taltmann");
        assert_eq!(body["variables"]["paths"].as_array().map(Vec::len), Some(5));
        assert!(description_mentions(&[], ME).is_empty());

        // Server ohne `search`: Variable entfällt, der Client prüft selbst
        let mut query = queries[0].clone();
        let search = SchemaError {
            code: SchemaErrorCode::ArgumentNotAccepted,
            type_name: "Field".to_string(),
            name: "search".to_string(),
            field: Some("mergeRequests".to_string()),
        };
        assert!(query.reduce(&search));
        let text = query.text();
        assert!(
            text.starts_with("query GmDescriptionMentions($paths: [String!]) {"),
            "{text}"
        );
        let body: Value = serde_json::from_str(&query.body()).expect("JSON");
        assert_eq!(body["variables"].get("search"), None);
    }

    #[test]
    fn uninvolved_page_aliases_projects() {
        let pages = vec![
            ("web/kundenportal".to_string(), "c-portal".to_string()),
            ("kern/backend-api".to_string(), "c-api".to_string()),
        ];
        let queries = uninvolved_page(&pages, ME);
        assert_eq!(queries.len(), 1);
        let text = queries[0].text();
        assert!(
            text.starts_with(
                "query GmUninvolvedPage($p0: ID!, $c0: String, $p1: ID!, $c1: String, $me: String)"
            ),
            "{text}"
        );
        assert!(text.contains("p0: project(fullPath: $p0) { fullPath mergeRequests(state: opened, first: 20, after: $c0, sort: UPDATED_DESC, not: {authorUsername: $me}) { count pageInfo { hasNextPage endCursor } nodes { ...Mr } } }"), "{text}");
        assert!(text.contains("p1: project(fullPath: $p1)"), "{text}");
        let body: Value = serde_json::from_str(&queries[0].body()).expect("JSON");
        assert_eq!(
            body["variables"],
            serde_json::json!({
                "p0": "web/kundenportal", "c0": "c-portal",
                "p1": "kern/backend-api", "c1": "c-api", "me": ME
            })
        );

        let data = serde_json::json!({
            "p0": { "fullPath": "web/kundenportal", "mergeRequests": {
                "count": 5,
                "pageInfo": { "hasNextPage": false, "endCursor": "x" },
                "nodes": [
                    { "id": "a", "iid": "96", "author": { "username": "alehmann" } },
                    { "id": "b", "iid": "99", "author": { "username": "taltmann" } }
                ] } },
            "p1": null
        });
        let projects = parse_uninvolved_page(&data, ME);
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].mrs.len(), 1);
        assert_eq!(
            projects[0].mrs[0].project_path.as_deref(),
            Some("web/kundenportal")
        );
    }

    #[test]
    fn reduce_prunes_empty_parents_and_fragments() {
        let mut query = dashboard();
        for field in ["status", "path"] {
            assert!(query.reduce(&removed("Pipeline", field)));
        }
        assert!(!query.text().contains("headPipeline"));
        // gleicher Fehler noch einmal: keine Änderung
        assert!(!query.reduce(&removed("Pipeline", "status")));

        // Fragment leer → Fragment, Spread und `nodes` verschwinden
        let mut query = dashboard();
        let fragment = query.fragments[0].clone();
        for item in &fragment.sel.items {
            if let Item::Field(f) = item {
                query.reduce(&removed("MergeRequest", f.name));
            }
        }
        let text = query.text();
        assert!(
            !text.contains("fragment") && !text.contains("..."),
            "{text}"
        );
        assert!(text.contains("authored: authoredMergeRequests(state: opened, first: 50, sort: UPDATED_DESC) { pageInfo { hasNextPage endCursor } }"), "{text}");
        // ohne `pageInfo` auch die Listen
        query.reduce(&removed("PageInfo", "hasNextPage"));
        query.reduce(&removed("PageInfo", "endCursor"));
        let text = query.text();
        assert_eq!(
            text,
            "query GmDashboard { metadata { version enterprise } currentUser { username name } }"
        );

        // Argumentfehler an einem Input-Objekt: nur optionale Argumente
        let mut query = uninvolved(&strings(&["a/b"]), ME).remove(0);
        let err = SchemaError {
            code: SchemaErrorCode::ArgumentNotAccepted,
            type_name: "InputObject".to_string(),
            name: "authorUsername".to_string(),
            field: Some("MergeRequestsResolverNegatedParams".to_string()),
        };
        assert!(query.reduce(&err));
        assert!(!query.text().contains("not:"));
        let required = SchemaError {
            code: SchemaErrorCode::ArgumentNotAccepted,
            type_name: "Field".to_string(),
            name: "fullPaths".to_string(),
            field: Some("projects".to_string()),
        };
        assert!(!query.reduce(&required));

        // `state` an `todos` ist Pflicht: ohne fehlten erledigte Erwähnungen
        let mut query = mentions(None);
        for code in [
            SchemaErrorCode::ArgumentNotAccepted,
            SchemaErrorCode::ArgumentLiteralsIncompatible,
        ] {
            let state = SchemaError {
                code,
                type_name: "Field".to_string(),
                name: "state".to_string(),
                field: Some("todos".to_string()),
            };
            assert!(!query.reduce(&state));
        }
        assert!(query.text().contains("state: [pending, done]"));
    }

    #[test]
    fn argument_error_on_alias_hits_only_that_field() {
        // GitLab nennt bei Feldern mit Alias den Alias (`authored`)
        let runner = gated();
        let body = serde_json::json!({ "errors": [{
            "message": "Field 'authored' doesn't accept argument 'sort'",
            "extensions": { "code": "argumentNotAccepted", "name": "authored", "typeName": "Field", "argumentName": "sort" }
        }] });
        runner.inner().push(reply(&body.to_string()));
        runner.inner().push(reply(DASHBOARD_EE));
        let mut schema = HostSchema::default();
        run_query(&runner, HOST, &mut schema, dashboard()).expect("Erfolg");
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        let (text, _) = sent(&calls[1]);
        assert!(
            text.contains("authored: authoredMergeRequests(state: opened, first: 50) {"),
            "{text}"
        );
        assert!(
            text.contains(
                "reviewing: reviewRequestedMergeRequests(state: opened, first: 50, sort: UPDATED_DESC)"
            ),
            "{text}"
        );

        // Literalfehler: Feld aus der Meldung, ebenfalls der Alias
        let literal = serde_json::json!({ "errors": [{
            "message": "Argument 'sort' on Field 'reviewing' has an invalid value (UPDATED_DESC). Expected type 'MergeRequestSort'.",
            "extensions": { "code": "argumentLiteralsIncompatible", "typeName": "Field", "argumentName": "sort" }
        }] });
        let mut query = dashboard();
        for err in schema_errors(&literal) {
            assert!(query.reduce(&err));
        }
        let text = query.text();
        assert!(
            text.contains("reviewing: reviewRequestedMergeRequests(state: opened, first: 50) {"),
            "{text}"
        );
        assert!(text.contains("first: 50, sort: UPDATED_DESC"), "{text}");
    }

    #[test]
    fn learned_reductions_survive_process_error() {
        let runner = gated();
        runner.inner().push(reply(DASHBOARD_CE_UNDEFINED));
        runner
            .inner()
            .push(FakeReply::error(GlabError::Timeout(Duration::from_secs(
                45,
            ))));
        let mut schema = HostSchema::default();
        assert_eq!(
            run_query(&runner, HOST, &mut schema, dashboard()),
            Err(GlabErrorKind::Timeout)
        );
        assert_eq!(runner.inner().take_calls().len(), 2);
        assert!(schema
            .removed
            .contains(&removed("MergeRequest", "approvalsRequired")));
        assert!(schema
            .removed
            .contains(&removed("MergeRequest", "approvalsLeft")));
    }

    #[test]
    fn run_query_with_prepares_every_attempt() {
        let runner = gated();
        runner.inner().push(reply(DASHBOARD_CE_UNDEFINED));
        runner.inner().push(reply(DASHBOARD_CE));
        let mut schema = HostSchema::default();
        let prepare = |req: GlabRequest| {
            req.with_env("GLAB_API_PROTOCOL", EnvValue::Plain("http".to_string()))
        };
        run_query_with(&runner, HOST, &mut schema, dashboard(), &prepare).expect("Erfolg");
        let calls = runner.inner().take_calls();
        assert_eq!(calls.len(), 2);
        for call in &calls {
            assert_eq!(
                call.env.get("GLAB_API_PROTOCOL").map(String::as_str),
                Some("http")
            );
        }
    }
}
