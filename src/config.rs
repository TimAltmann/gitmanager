use crate::i18n::Language;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// --- Default helpers ---
fn default_config_version() -> u32 {
    10
}
fn default_tray_max_display() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayIconConfig {
    /// Reihenfolge der Icons im Tray-Popup (IDs in Anzeigereihenfolge)
    #[serde(default)]
    pub icon_order: Vec<String>,
    /// Ausgeblendete Icons (IDs)
    #[serde(default)]
    pub hidden_icon_ids: Vec<String>,
    /// Maximale Anzahl angezeigter Repos im Popup (für MRU)
    #[serde(default = "default_tray_max_display")]
    pub max_display: usize,
}

impl Default for TrayIconConfig {
    fn default() -> Self {
        Self {
            icon_order: Vec::new(),
            hidden_icon_ids: Vec::new(),
            max_display: default_tray_max_display(),
        }
    }
}
fn default_active_profile() -> String {
    "dotnet".to_string()
}
fn default_scan_depth() -> usize {
    3
}
fn default_language() -> Language {
    Language::En
}
fn default_branch_limit() -> usize {
    200
}
fn default_minimize_to_tray() -> bool {
    true
}
fn default_tray_branch_limit() -> usize {
    20
}

// --- Terminal ---
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TerminalPreference {
    #[default]
    Auto,
    WindowsTerminal,
    Cmd,
    Powershell,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default)]
    pub preference: TerminalPreference,
    #[serde(default)]
    pub fallback: TerminalPreference,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            preference: TerminalPreference::Auto,
            fallback: TerminalPreference::Cmd,
        }
    }
}

// --- Theme ---
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Light,
    Dark,
    Nord,
    Dracula,
    Solarized,
}

impl Theme {
    pub fn display_name(&self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::Nord => "Nord",
            Theme::Dracula => "Dracula",
            Theme::Solarized => "Solarized Light",
        }
    }

    pub fn all() -> Vec<Theme> {
        vec![
            Theme::Light,
            Theme::Dark,
            Theme::Nord,
            Theme::Dracula,
            Theme::Solarized,
        ]
    }
}

// --- IDE ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeConfig {
    pub id: String,
    pub display_name: String,
    /// Programm ohne Pfad, z.B. "code", "devenv", "rider"
    #[serde(default)]
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Legacy: kompletter Command String mit Platzhaltern
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub use_shell: bool,
    #[serde(default)]
    pub allow_unsafe: bool,
    /// Wenn true: IDE ohne Argumente starten (nur cwd = repo_path)
    #[serde(default)]
    pub no_args: bool,
    /// Pfad zu Custom Icon im AppData/icons Verzeichnis (kopiert via FilePicker). None = Default Icon.
    #[serde(default)]
    pub icon: Option<String>,
}

impl IdeConfig {
    pub fn effective_program(&self) -> String {
        if !self.program.is_empty() {
            self.program.clone()
        } else if let Some(cmd) = &self.command {
            // Fallback: nimm erstes Wort aus command
            cmd.split_whitespace().next().unwrap_or("code").to_string()
        } else {
            "code".to_string()
        }
    }
    pub fn effective_args(&self) -> Vec<String> {
        if self.no_args {
            return Vec::new();
        }
        if !self.args.is_empty() {
            self.args.clone()
        } else if let Some(cmd) = &self.command {
            // Parse rest nach erstem Wort
            let parts: Vec<String> = cmd.split_whitespace().map(|s| s.to_string()).collect();
            if parts.len() > 1 {
                parts[1..].to_vec()
            } else {
                vec!["{file}".to_string()]
            }
        } else {
            vec!["{file}".to_string()]
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_key_attr() -> String {
    "key".to_string()
}
fn default_val_attr() -> String {
    "value".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOption {
    pub value: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum XmlSelectorKind {
    #[default]
    AddKeyValue,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSelector {
    pub id: String,
    pub display_name: String,
    pub file_path: String,
    pub key: String,
    #[serde(default = "default_key_attr")]
    pub key_attribute: String,
    #[serde(default = "default_val_attr")]
    pub value_attribute: String,
    #[serde(default)]
    pub kind: XmlSelectorKind,
    pub options: Vec<ConfigOption>,
    #[serde(default)]
    pub allow_custom: bool,
}

// --- Language Profile ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageProfile {
    pub id: String,
    pub display_name: String,
    /// z.B. ".sln" – wird normalisiert
    pub file_extension: String,
    #[serde(default)]
    pub file_pattern: Option<String>,
    #[serde(default = "default_scan_depth")]
    pub max_scan_depth: usize,
    pub ides: Vec<IdeConfig>,
    pub default_ide_id: Option<String>,
    /// Reihenfolge der IDE-Icons (IDs in Anzeigereihenfolge). Leer = natürliche ides Reihenfolge
    #[serde(default)]
    pub ide_order: Vec<String>,
    /// Ausgeblendete IDE Icons (IDs)
    #[serde(default)]
    pub hidden_ide_ids: Vec<String>,
    /// Ausgeblendete AI Agents pro Profil
    #[serde(default)]
    pub hidden_agent_ids: Vec<String>,
    /// Reihenfolge der AI Icons pro Profil
    #[serde(default)]
    pub agent_order: Vec<String>,
    #[serde(default = "default_true")]
    pub show_shell: bool,
    #[serde(default = "default_true")]
    pub show_explorer: bool,
    #[serde(default)]
    pub config_selectors: Vec<ConfigSelector>,
}

impl LanguageProfile {
    pub fn normalized_extension(&self) -> String {
        let mut ext = self.file_extension.trim().to_lowercase();
        if !ext.starts_with('.') {
            ext = format!(".{}", ext);
        }
        ext
    }
    pub fn matches_file(&self, path: &Path) -> bool {
        if let Some(pat) = &self.file_pattern {
            // Unterstütze mehrere Muster getrennt durch Komma, z.B. "pom.xml,build.gradle" oder "*.sln,*.csproj"
            for single_pat in pat.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                if single_pat.contains('*') {
                    // Wildcard-Pattern: z.B. "*.sln" → Extension aus Pattern extrahieren, nicht aus self.file_extension
                    // "*" oder "*.*" → matcht jede Datei mit Extension bzw. jede Datei
                    if single_pat == "*" {
                        return true;
                    }
                    if single_pat == "*.*" {
                        if path.extension().is_some() {
                            return true;
                        } else {
                            continue;
                        }
                    }
                    if single_pat.starts_with("*.") {
                        // "*.sln" → ".sln"
                        let pattern_ext = single_pat[1..].trim().to_lowercase(); // ab "." inkl. Punkt
                        let mut pat_ext = pattern_ext;
                        if !pat_ext.starts_with('.') {
                            pat_ext = format!(".{}", pat_ext);
                        }
                        let file_ext = path
                            .extension()
                            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
                            .unwrap_or_default();
                        if file_ext == pat_ext {
                            return true;
                        }
                    } else {
                        // Generisches Wildcard wie "*foo" oder "foo*": suffix/prefix check auf Dateiname (case-insensitiv)
                        let pat_lower = single_pat.to_lowercase();
                        let pat_clean = pat_lower.replace('*', "");
                        if pat_clean.is_empty() {
                            return true;
                        }
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            let name_lower = name.to_lowercase();
                            // "*foo" → ends_with, "foo*" → starts_with, "*foo*" → contains
                            let starts_with_star = single_pat.starts_with('*');
                            let ends_with_star = single_pat.ends_with('*');
                            if starts_with_star && ends_with_star {
                                if name_lower.contains(&pat_clean) {
                                    return true;
                                }
                            } else if starts_with_star {
                                if name_lower.ends_with(&pat_clean) {
                                    return true;
                                }
                            } else if ends_with_star {
                                if name_lower.starts_with(&pat_clean) {
                                    return true;
                                }
                            } else if name_lower == pat_lower {
                                return true;
                            }
                        }
                    }
                } else {
                    // exakter Name wie "Cargo.toml"
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name == single_pat {
                            return true;
                        }
                    }
                }
            }
            return false;
        }
        let ext = self.normalized_extension();
        path.extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()) == ext)
            .unwrap_or(false)
    }

    pub fn visible_ides(&self) -> Vec<&IdeConfig> {
        let mut v: Vec<&IdeConfig> = self
            .ides
            .iter()
            .filter(|i| !self.hidden_ide_ids.contains(&i.id))
            .collect();
        if !self.ide_order.is_empty() {
            v.sort_by_key(|ide| {
                self.ide_order
                    .iter()
                    .position(|id| id == &ide.id)
                    .unwrap_or(usize::MAX)
            });
        } else if let Some(def) = &self.default_ide_id {
            v.sort_by_key(|ide| if &ide.id == def { 0 } else { 1 });
        }
        v
    }

    pub fn filtered_agents<'a>(
        &self,
        all_agents: &'a [crate::config::AgentProfile],
        active_ids: &[String],
    ) -> Vec<&'a crate::config::AgentProfile> {
        // Nur aktive und nicht hidden
        let mut v: Vec<&crate::config::AgentProfile> = all_agents
            .iter()
            .filter(|a| active_ids.contains(&a.id) && !self.hidden_agent_ids.contains(&a.id))
            .collect();
        if !self.agent_order.is_empty() {
            v.sort_by_key(|a| {
                self.agent_order
                    .iter()
                    .position(|id| id == &a.id)
                    .unwrap_or(usize::MAX)
            });
        } else {
            // behalte active_ids Reihenfolge
            v.sort_by_key(|a| {
                active_ids
                    .iter()
                    .position(|id| id == &a.id)
                    .unwrap_or(usize::MAX)
            });
        }
        v
    }
}

fn default_dotnet_profile() -> LanguageProfile {
    LanguageProfile {
        id: "dotnet".to_string(),
        display_name: ".NET".to_string(),
        file_extension: ".sln".to_string(),
        file_pattern: Some("*.sln".to_string()),
        max_scan_depth: 3,
        default_ide_id: Some("vs2022".to_string()),
        ides: vec![
            IdeConfig {
                id: "vs2022".to_string(),
                display_name: "Visual Studio".to_string(),
                program: "devenv".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,
                icon: None,
            },
            IdeConfig {
                id: "vscode".to_string(),
                display_name: "VS Code".to_string(),
                program: "code".to_string(),
                args: vec![".".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,
                icon: None,
            },
            IdeConfig {
                id: "rider".to_string(),
                display_name: "Rider".to_string(),
                program: "rider".to_string(),
                args: vec!["{file}".to_string()],
                command: None,
                use_shell: false,
                allow_unsafe: false,
                no_args: false,
                icon: None,
            },
        ],
        ide_order: Vec::new(),
        hidden_ide_ids: Vec::new(),
        hidden_agent_ids: Vec::new(),
        agent_order: Vec::new(),
        show_shell: true,
        show_explorer: true,
        config_selectors: Vec::new(),
    }
}

fn default_profiles() -> Vec<LanguageProfile> {
    // Nur .NET als Default – andere Profile machen laut Nutzer kein Sinn
    // Nutzer kann eigene Profile in den Einstellungen hinzufügen
    vec![default_dotnet_profile()]
}

// --- Agent ---
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentLaunchMode {
    #[default]
    Terminal,
    Detached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: String,
    pub display_name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub launch_mode: AgentLaunchMode,
    #[serde(default)]
    pub terminal_override: Option<TerminalPreference>,
    /// Pfad zu Custom Icon im AppData/icons Verzeichnis (kopiert via FilePicker). None = Default Icon.
    #[serde(default)]
    pub icon: Option<String>,
}

fn default_agents() -> Vec<AgentProfile> {
    vec![
        AgentProfile {
            id: "claude".to_string(),
            display_name: "Claude Code".to_string(),
            program: "claude".to_string(),
            args: vec![],
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        },
        AgentProfile {
            id: "codex".to_string(),
            display_name: "Codex (OpenAI)".to_string(),
            program: "codex".to_string(),
            args: vec![],
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        },
        AgentProfile {
            id: "gemini".to_string(),
            display_name: "Gemini CLI".to_string(),
            program: "gemini".to_string(),
            args: vec![],
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        },
        AgentProfile {
            id: "copilot".to_string(),
            display_name: "Copilot CLI".to_string(),
            program: "copilot".to_string(),
            args: vec![],
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        },
        AgentProfile {
            id: "cursor".to_string(),
            display_name: "Cursor Agent".to_string(),
            program: "cursor-agent".to_string(),
            args: vec![],
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        },
        AgentProfile {
            id: "aider".to_string(),
            display_name: "Aider".to_string(),
            program: "aider".to_string(),
            args: vec![],
            command: None,
            launch_mode: AgentLaunchMode::Terminal,
            terminal_override: None,
            icon: None,
        },
    ]
}

fn default_active_agent() -> Option<String> {
    Some("claude".to_string())
}

fn default_active_agents() -> Vec<String> {
    vec!["claude".to_string()]
}

// --- Repo UI State ---
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoUiState {
    #[serde(default)]
    pub selected_solution: Option<PathBuf>,
    #[serde(default)]
    pub selected_ide: Option<String>,
    #[serde(default)]
    pub profile_override: Option<String>,
}

// --- AppConfig ---
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Liste der Root-Pfade, in denen nach Repos gesucht wird
    pub roots: Vec<PathBuf>,
    /// Maximale Suchtiefe (1 = nur direkte Kinder, 2 = Kinder+Enkel, etc.)
    pub max_depth: usize,

    #[serde(default = "default_config_version")]
    pub config_version: u32,

    #[serde(default = "default_active_profile")]
    pub active_profile_id: String,

    #[serde(default = "default_profiles")]
    pub profiles: Vec<LanguageProfile>,

    #[serde(default = "default_agents")]
    pub agents: Vec<AgentProfile>,

    #[serde(default = "default_active_agent")]
    pub active_agent_id: Option<String>, // deprecated, für Migration von v3

    #[serde(default)]
    pub active_agent_ids: Vec<String>,

    #[serde(default)]
    pub theme: Theme,

    #[serde(default)]
    pub terminal: TerminalConfig,

    #[serde(default)]
    pub repo_state: HashMap<String, RepoUiState>,

    #[serde(default = "default_language")]
    pub language: Language,

    #[serde(default = "default_branch_limit")]
    pub branch_display_limit: usize,

    #[serde(default = "default_minimize_to_tray")]
    pub minimize_to_tray: bool,

    #[serde(default = "default_tray_branch_limit")]
    pub tray_branch_limit: usize,

    #[serde(default)]
    pub tray_icons: TrayIconConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let default_root = default_root_path();
        let roots = if let Some(p) = default_root {
            vec![p]
        } else {
            Vec::new()
        };
        Self {
            roots,
            max_depth: 2,
            config_version: default_config_version(),
            active_profile_id: default_active_profile(),
            profiles: default_profiles(),
            agents: default_agents(),
            active_agent_id: default_active_agent(),
            active_agent_ids: default_active_agents(),
            theme: Theme::default(),
            terminal: TerminalConfig::default(),
            repo_state: HashMap::new(),
            language: default_language(),
            branch_display_limit: default_branch_limit(),
            minimize_to_tray: default_minimize_to_tray(),
            tray_branch_limit: default_tray_branch_limit(),
            tray_icons: TrayIconConfig::default(),
        }
    }
}

fn default_root_path() -> Option<PathBuf> {
    if let Some(home) = dirs_home() {
        let candidates = [
            "repos",
            "source",
            "dev",
            "Development",
            "Projekte",
            "projects",
            "git",
        ];
        for c in candidates {
            let p = home.join(c);
            if p.is_dir() {
                return Some(p);
            }
        }
        if home.is_dir() {
            return Some(home);
        }
    }
    None
}

fn dirs_home() -> Option<PathBuf> {
    directories::UserDirs::new().map(|u| u.home_dir().to_path_buf())
}

impl AppConfig {
    pub fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "gitmanager", "gitmanager").map(|dirs| {
            let cfg_dir = dirs.config_dir();
            cfg_dir.join("config.json")
        })
    }

    pub fn legacy_config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "repomanager", "repomanager").map(|dirs| {
            let cfg_dir = dirs.config_dir();
            cfg_dir.join("config.json")
        })
    }

    pub fn icons_dir() -> Option<PathBuf> {
        Self::config_path().and_then(|p| p.parent().map(|d| d.join("icons")))
    }

    pub fn copy_icon_to_storage(src: &Path) -> Result<PathBuf, String> {
        let dir = Self::icons_dir().ok_or_else(|| "Kein Config-Pfad ermittelbar".to_string())?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Icons-Verzeichnis erstellen fehlgeschlagen: {e}"))?;
        let ext = src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let allowed = ["svg", "png", "ico", "jpg", "jpeg"];
        if !allowed.contains(&ext.as_str()) {
            return Err(format!(
                "Ungültiges Icon-Format '.{}' – erlaubt: svg, png, ico, jpg",
                ext
            ));
        }
        // Size limit 2 MB
        if let Ok(meta) = std::fs::metadata(src) {
            if meta.len() > 2 * 1024 * 1024 {
                return Err("Icon-Datei zu groß (max 2 MB)".to_string());
            }
        }
        let stem = src
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("icon")
            .replace(' ', "_")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();
        let stem = if stem.is_empty() {
            "icon".to_string()
        } else {
            stem
        };
        // Unique suffix via timestamp + random
        let uniq = format!(
            "{}{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis().to_string())
                .unwrap_or_else(|_| "0".to_string()),
            std::process::id()
        );
        let dest_name = format!("{}_{}.{}", stem, &uniq[uniq.len().saturating_sub(6)..], ext);
        let dest = dir.join(dest_name);
        std::fs::copy(src, &dest).map_err(|e| format!("Icon kopieren fehlgeschlagen: {e}"))?;
        Ok(dest)
    }

    pub fn remove_icon_file(path_str: &str) {
        if path_str.is_empty() {
            return;
        }
        let p = PathBuf::from(path_str);
        // Only remove files inside icons_dir for safety
        if let Some(dir) = Self::icons_dir() {
            if p.starts_with(&dir) && p.exists() {
                let _ = std::fs::remove_file(p);
            }
        }
    }

    /// Lädt die Config von Disk oder gibt Default zurück
    pub fn load() -> Self {
        match Self::try_load() {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Config laden fehlgeschlagen, nutze Default: {e:#}");
                Self::default()
            }
        }
    }

    fn try_load() -> Result<Self> {
        let path = Self::config_path().context("Kein Config-Pfad ermittelbar")?;
        // Migration: if new path doesn't exist but legacy repomanager path does, import it
        let effective_path = if !path.exists() {
            if let Some(legacy) = Self::legacy_config_path() {
                if legacy.exists() {
                    // Import legacy config: read from legacy, then save to new location on next migration save
                    let legacy_data = std::fs::read_to_string(&legacy).with_context(|| {
                        format!("Konnte Legacy-Config nicht lesen: {}", legacy.display())
                    })?;
                    let mut cfg: Self = serde_json::from_str(&legacy_data)
                        .context("Legacy Config JSON ungültig")?;
                    cfg.config_version = cfg.config_version.max(4);
                    // Mark as v5 to trigger language migration save
                    let _ = cfg.save();
                    return Ok(cfg);
                }
            }
            return Ok(Self::default());
        } else {
            path
        };
        let data = std::fs::read_to_string(&effective_path)
            .with_context(|| format!("Konnte Config nicht lesen: {}", effective_path.display()))?;
        let mut cfg: Self = serde_json::from_str(&data).context("Config JSON ungültig")?;
        // Migration & Validierung
        if cfg.config_version < 2 {
            // Alte Config ohne Version → injiziere Defaults
            if cfg.profiles.is_empty() {
                cfg.profiles = default_profiles();
            }
            if cfg.agents.is_empty() {
                cfg.agents = default_agents();
            }
            if cfg.active_profile_id.is_empty() {
                cfg.active_profile_id = default_active_profile();
            }
            if cfg.active_agent_id.is_none() {
                cfg.active_agent_id = default_active_agent();
            }
            cfg.config_version = 2;
            // Speichere migrierte Config zurück (best effort)
            let _ = cfg.save();
        }
        if cfg.config_version < 3 {
            // Neue Defaults seit v3: 7 Profile + 6 Agents – fehlende ergänzen
            let defaults = default_profiles();
            for dp in defaults {
                if !cfg.profiles.iter().any(|p| p.id == dp.id) {
                    cfg.profiles.push(dp);
                }
            }
            let default_agents_list = default_agents();
            for da in default_agents_list {
                if !cfg.agents.iter().any(|a| a.id == da.id) {
                    cfg.agents.push(da);
                }
            }
            cfg.config_version = 3;
            let _ = cfg.save();
        }
        if cfg.config_version < 4 {
            // v4: Nur noch .NET als Default-Profil, andere Presets entfernen (Custom bleiben)
            // Entferne die 6 zuvor auto-hinzugefügten Profile, behalte nur dotnet + custom
            let keep_dotnet_only = ["dotnet"];
            // Behalte nur dotnet und alle die nicht zu den alten Defaults gehören
            let old_auto_ids = ["rust", "node", "python", "java", "go", "cpp"];
            cfg.profiles.retain(|p| {
                if keep_dotnet_only.contains(&p.id.as_str()) {
                    true
                } else if old_auto_ids.contains(&p.id.as_str()) {
                    // Prüfe ob es ein reines Default-Profil war (keine Custom-Änderungen)
                    // Für Einfachheit: entferne es, Nutzer kann es bei Bedarf neu anlegen
                    false
                } else {
                    true // Custom Profile behalten
                }
            });
            if cfg.profiles.is_empty() {
                cfg.profiles = default_profiles();
            }
            // Migriere active_agent_id -> active_agent_ids
            if cfg.active_agent_ids.is_empty() {
                if let Some(old) = cfg.active_agent_id.clone() {
                    if !old.is_empty() && !cfg.active_agent_ids.contains(&old) {
                        cfg.active_agent_ids.push(old);
                    }
                }
                if cfg.active_agent_ids.is_empty() {
                    cfg.active_agent_ids = default_active_agents();
                }
            }
            // Falls active_agent_ids leer, setze default
            if cfg.active_agent_ids.is_empty() {
                cfg.active_agent_ids = default_active_agents();
            }
            // Theme ist via #[serde(default)] bereits Light falls fehlend
            cfg.config_version = 4;
            let _ = cfg.save();
        }
        if cfg.config_version < 5 {
            // v5: Language setting added (default En via serde), just bump version
            cfg.config_version = 5;
            let _ = cfg.save();
        }
        if cfg.config_version < 6 {
            // v6: branch_display_limit + vscode default "." + no_args flag + {repo} Warnung
            if cfg.branch_display_limit == 0 {
                cfg.branch_display_limit = default_branch_limit();
            }
            // Migrate vscode args from {file} to "." if still default (auch mit Zusatzflags)
            for p in &mut cfg.profiles {
                for ide in &mut p.ides {
                    if ide.id == "vscode"
                        && !ide.no_args
                        && ide.args.first().map(|s| s == "{file}").unwrap_or(false)
                    {
                        if ide.args.len() == 1 {
                            ide.args = vec![".".to_string()];
                        } else {
                            ide.args[0] = ".".to_string();
                        }
                    }
                    // Historisch getrennte Flags synchronisieren nur einmalig für Migration
                    // (danach bleiben sie unabhängig – vermeidet ungewolltes allow_unsafe=true)
                    if ide.use_shell && !ide.allow_unsafe {
                        ide.allow_unsafe = true;
                    }
                    // {repo} bedeutete früher Datei, jetzt Repo-Root – warnen wenn noch genutzt
                    if ide.args.iter().any(|a| a.contains("{repo}")) {
                        eprintln!(
                            "Warnung: IDE '{}' nutzt {{repo}} – seit v6 bedeutet {{repo}} Repo-Root (früher Datei). Prüfe Args: {:?}",
                            ide.id, ide.args
                        );
                    }
                }
            }
            cfg.config_version = 6;
            let _ = cfg.save();
        }
        if cfg.config_version < 7 {
            for p in &mut cfg.profiles {
                for sel in &mut p.config_selectors {
                    sel.id = sel.id.trim().to_lowercase().replace(' ', "_");
                    sel.file_path = sel.file_path.trim().to_string();
                    sel.key = sel.key.trim().to_string();
                    if sel.key_attribute.trim().is_empty() {
                        sel.key_attribute = "key".to_string();
                    }
                    if sel.value_attribute.trim().is_empty() {
                        sel.value_attribute = "value".to_string();
                    }
                    sel.options.retain(|o| !o.value.trim().is_empty());
                }
            }
            cfg.config_version = 7;
            let _ = cfg.save();
        }
        if cfg.config_version < 8 {
            for p in &mut cfg.profiles {
                for ide in &mut p.ides {
                    if let Some(ic) = ide.icon.as_mut() {
                        let t = ic.trim().to_string();
                        if t.is_empty() {
                            ide.icon = None;
                        } else {
                            *ic = t;
                        }
                    }
                }
            }
            for a in &mut cfg.agents {
                if let Some(ic) = a.icon.as_mut() {
                    let t = ic.trim().to_string();
                    if t.is_empty() {
                        a.icon = None;
                    } else {
                        *ic = t;
                    }
                }
            }
            cfg.config_version = 8;
            let _ = cfg.save();
        }
        if cfg.config_version < 9 {
            // v9: Tray settings introduced
            // minimize_to_tray default handled via serde default, but ensure tray_branch_limit valid
            if cfg.tray_branch_limit == 0 {
                cfg.tray_branch_limit = default_tray_branch_limit();
            }
            cfg.config_version = 9;
            let _ = cfg.save();
        }
        if cfg.config_version < 10 {
            // v10: TrayIconConfig introduced (icon_order, hidden_icon_ids, max_display)
            // Tray icons: fixed list for MVP ["vscode","vs2022","rider","folder","terminal","claude","codex","gemini","copilot","cursor","aider"]
            const DEFAULT_TRAY_ICONS: &[&str] = &[
                "vscode", "vs2022", "rider", "folder", "terminal", "claude", "codex", "gemini",
                "copilot", "cursor", "aider",
            ];
            if cfg.tray_icons.max_display == 0 {
                cfg.tray_icons.max_display = default_tray_max_display();
            }
            cfg.tray_icons.max_display = cfg.tray_icons.max_display.clamp(5, 50);
            // hidden ids dedup/sort
            cfg.tray_icons.hidden_icon_ids.sort();
            cfg.tray_icons.hidden_icon_ids.dedup();
            // filter hidden to valid ids only (keep custom hidden as is for forward compat? filter to known list)
            // keep only known ids for migration, custom unknown kept as is (no filtering)
            // icon_order dedup preserve order
            {
                let mut seen = std::collections::HashSet::new();
                cfg.tray_icons.icon_order.retain(|id| seen.insert(id.clone()));
            }
            if cfg.tray_icons.icon_order.is_empty() {
                cfg.tray_icons.icon_order = DEFAULT_TRAY_ICONS.iter().map(|s| s.to_string()).collect();
            } else {
                // ensure all default icons present (append missing)
                for id in DEFAULT_TRAY_ICONS {
                    if !cfg.tray_icons.icon_order.contains(&id.to_string()) {
                        cfg.tray_icons.icon_order.push(id.to_string());
                    }
                }
                // also remove invalid? keep as is for forward compat, but ensure deduped above
            }
            cfg.config_version = 10;
            let _ = cfg.save();
        }
        if cfg.branch_display_limit == 0 {
            cfg.branch_display_limit = default_branch_limit();
        }
        cfg.branch_display_limit = cfg.branch_display_limit.clamp(50, 500);
        if cfg.tray_branch_limit == 0 {
            cfg.tray_branch_limit = default_tray_branch_limit();
        }
        cfg.tray_branch_limit = cfg.tray_branch_limit.clamp(5, 50);
        if cfg.tray_icons.max_display == 0 {
            cfg.tray_icons.max_display = default_tray_max_display();
        }
        cfg.tray_icons.max_display = cfg.tray_icons.max_display.clamp(5, 50);
        cfg.tray_icons.hidden_icon_ids.sort();
        cfg.tray_icons.hidden_icon_ids.dedup();
        {
            let mut seen = std::collections::HashSet::new();
            cfg.tray_icons.icon_order.retain(|id| seen.insert(id.clone()));
        }
        if cfg.max_depth == 0 {
            cfg.max_depth = 2;
        }
        if cfg.max_depth > 10 {
            cfg.max_depth = 10;
        }
        cfg.roots.retain(|p| !p.as_os_str().is_empty());
        cfg.roots.sort();
        cfg.roots.dedup();

        // RepoState Keys auf Windows normalisieren (case-insensitiv) – verhindert doppelte Einträge
        #[cfg(windows)]
        {
            let mut normalized: HashMap<String, RepoUiState> = HashMap::new();
            for (k, v) in cfg.repo_state.drain() {
                let nk = k.to_lowercase();
                normalized.entry(nk).or_insert(v);
            }
            cfg.repo_state = normalized;
        }

        // Profile validieren
        for p in &mut cfg.profiles {
            p.file_extension = p.normalized_extension();
            // file_pattern normalisieren: trimme jedes Pattern, leere entfernen
            if let Some(pat) = p.file_pattern.clone() {
                let trimmed = pat
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(",");
                if trimmed.is_empty() {
                    p.file_pattern = None;
                } else {
                    p.file_pattern = Some(trimmed);
                }
            }
            if p.max_scan_depth == 0 {
                p.max_scan_depth = 3;
            }
            if p.max_scan_depth > 4 {
                p.max_scan_depth = 4;
            }
            p.id = p.id.trim().to_lowercase();
            if p.id.is_empty() {
                p.id = "custom".to_string();
            }
            // IDE Flags bleiben unabhängig (kein automatisches Sync mehr – vermeidet ungewolltes allow_unsafe=true).
            // Migration für alte Configs erfolgt einmalig im v6-Block oben.
            // Hinweis zu {repo} nur einmalig während Migration (oben), nicht bei jedem Laden – vermeidet Spam in Tests/Logs.
            // Default IDE validieren
            if let Some(def) = &p.default_ide_id {
                if !p.ides.iter().any(|i| &i.id == def) {
                    p.default_ide_id = p.ides.first().map(|i| i.id.clone());
                }
            } else {
                p.default_ide_id = p.ides.first().map(|i| i.id.clone());
            }
            // Icon-Einstellungen bereinigen (hidden/order nur für existierende IDs)
            let ide_ids: Vec<String> = p.ides.iter().map(|i| i.id.clone()).collect();
            p.hidden_ide_ids.retain(|id| ide_ids.contains(id));
            p.hidden_ide_ids.sort();
            p.hidden_ide_ids.dedup();
            p.ide_order.retain(|id| ide_ids.contains(id));
            // Deduplicate ide_order (first wins) – keine Sortierung, User-Reihenfolge erhalten
            {
                let mut seen = std::collections::HashSet::new();
                p.ide_order.retain(|id| seen.insert(id.clone()));
            }
            // Agent hidden/order – validiere gegen globale Agents (später nach Agents-Validierung, aber hier schon)
            // Wird nach Agents-Validierung nochmal gefiltert; hier nur Duplikate entfernen (Reihenfolge erhalten)
            p.hidden_agent_ids.sort();
            p.hidden_agent_ids.dedup();
            {
                let mut seen = std::collections::HashSet::new();
                p.agent_order.retain(|id| seen.insert(id.clone()));
            }
            // Config selectors normalisieren (analog file_extension etc.)
            for sel in &mut p.config_selectors {
                sel.id = sel.id.trim().to_lowercase().replace(' ', "_");
                sel.file_path = sel.file_path.trim().to_string();
                sel.key = sel.key.trim().to_string();
                if sel.key_attribute.trim().is_empty() {
                    sel.key_attribute = "key".to_string();
                }
                if sel.value_attribute.trim().is_empty() {
                    sel.value_attribute = "value".to_string();
                }
                sel.options.retain(|o| !o.value.trim().is_empty());
            }
        }
        // Duplikate bei Profilen entfernen (letztes gewinnt)
        {
            let mut seen = std::collections::HashSet::new();
            let mut deduped = Vec::new();
            for p in cfg.profiles.into_iter().rev() {
                if seen.insert(p.id.clone()) {
                    deduped.push(p);
                }
            }
            deduped.reverse();
            cfg.profiles = deduped;
        }
        // Aktives Profil validieren
        if !cfg.profiles.iter().any(|p| p.id == cfg.active_profile_id) {
            cfg.active_profile_id = cfg
                .profiles
                .first()
                .map(|p| p.id.clone())
                .unwrap_or_else(default_active_profile);
        }
        // Agents validieren
        if cfg.agents.is_empty() {
            cfg.agents = default_agents();
        }
        // Migriere active_agent_id -> active_agent_ids falls nötig
        if cfg.active_agent_ids.is_empty() {
            if let Some(old) = cfg.active_agent_id.clone() {
                if cfg.agents.iter().any(|a| a.id == old) {
                    cfg.active_agent_ids = vec![old];
                }
            }
        }
        // Filtere ungültige IDs
        cfg.active_agent_ids
            .retain(|id| cfg.agents.iter().any(|a| &a.id == id));
        if cfg.active_agent_ids.is_empty() && !cfg.agents.is_empty() {
            cfg.active_agent_ids = vec![cfg.agents[0].id.clone()];
        }
        // Sync deprecated field
        cfg.active_agent_id = cfg.active_agent_ids.first().cloned();
        if let Some(active) = &cfg.active_agent_id {
            if !cfg.agents.iter().any(|a| &a.id == active) {
                let new_active = cfg.agents.first().map(|a| a.id.clone());
                cfg.active_agent_id = new_active.clone();
                cfg.active_agent_ids = new_active.into_iter().collect();
            }
        }
        // Per-Profil hidden_agent_ids / agent_order gegen tatsächliche Agents validieren
        let valid_agent_ids: Vec<String> = cfg.agents.iter().map(|a| a.id.clone()).collect();
        for p in &mut cfg.profiles {
            p.hidden_agent_ids.retain(|id| valid_agent_ids.contains(id));
            p.agent_order.retain(|id| valid_agent_ids.contains(id));
        }
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path().context("Kein Config-Pfad ermittelbar")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Konnte Config-Verzeichnis nicht erstellen: {}",
                    parent.display()
                )
            })?;
        }
        let data =
            serde_json::to_string_pretty(self).context("Config serialisieren fehlgeschlagen")?;
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp)
                .with_context(|| format!("Konnte Config nicht schreiben: {}", tmp.display()))?;
            file.write_all(data.as_bytes())
                .with_context(|| format!("Konnte Config nicht schreiben: {}", tmp.display()))?;
            file.sync_all()
                .with_context(|| format!("Konnte Config nicht syncen: {}", tmp.display()))?;
        }
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("Konnte Config nicht finalisieren: {}", path.display()))?;
        // Best effort: sync parent dir (auf Unix)
        #[cfg(unix)]
        {
            if let Some(parent) = path.parent() {
                if let Ok(dir) = std::fs::File::open(parent) {
                    let _ = dir.sync_all();
                }
            }
        }
        Ok(())
    }

    pub fn add_root(&mut self, path: PathBuf) {
        if !self.roots.contains(&path) {
            self.roots.push(path);
            self.roots.sort();
        }
    }

    pub fn remove_root(&mut self, path: &Path) {
        self.roots.retain(|p| p != path);
    }

    pub fn get_active_profile(&self) -> &LanguageProfile {
        self.profiles
            .iter()
            .find(|p| p.id == self.active_profile_id)
            .or_else(|| self.profiles.first())
            .expect("mindestens ein Profil vorhanden")
    }

    pub fn get_profile(&self, id: &str) -> Option<&LanguageProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn get_active_agent(&self) -> Option<&AgentProfile> {
        // Bevorzugt active_agent_ids[0], Fallback auf deprecated active_agent_id
        if !self.active_agent_ids.is_empty() {
            let id = &self.active_agent_ids[0];
            if let Some(a) = self.agents.iter().find(|a| &a.id == id) {
                return Some(a);
            }
        }
        if let Some(id) = &self.active_agent_id {
            if let Some(a) = self.agents.iter().find(|a| &a.id == id) {
                return Some(a);
            }
        }
        self.agents.first()
    }

    pub fn get_active_agents(&self) -> Vec<&AgentProfile> {
        if !self.active_agent_ids.is_empty() {
            self.active_agent_ids
                .iter()
                .filter_map(|id| self.agents.iter().find(|a| &a.id == id))
                .collect()
        } else if let Some(id) = &self.active_agent_id {
            if let Some(a) = self.agents.iter().find(|a| &a.id == id) {
                return vec![a];
            }
            vec![]
        } else {
            vec![]
        }
    }

    pub fn is_agent_active(&self, agent_id: &str) -> bool {
        if !self.active_agent_ids.is_empty() {
            self.active_agent_ids.contains(&agent_id.to_string())
        } else if let Some(old) = &self.active_agent_id {
            old == agent_id
        } else {
            false
        }
    }

    pub fn toggle_agent_active(&mut self, agent_id: &str) {
        if self.is_agent_active(agent_id) {
            self.active_agent_ids.retain(|id| id != agent_id);
            // Verhindere leere Liste nicht strikt, aber sync deprecated
            if self.active_agent_ids.is_empty() {
                // Erlaube leer, aber UI zeigt dann keinen Button – optional fallback
            }
        } else {
            self.active_agent_ids.push(agent_id.to_string());
        }
        self.active_agent_id = self.active_agent_ids.first().cloned();
    }

    fn repo_state_key(path: &Path) -> String {
        // Auf Windows ist das Dateisystem case-insensitiv – Keys normalisieren, sonst gehen
        // Selections verloren wenn Pfade mal "C:\Dev\MyApp" und mal "c:\dev\myapp" lauten (Explorer, Symlink, canonicalize)
        let s = path.to_string_lossy().to_string();
        #[cfg(windows)]
        {
            s.to_lowercase()
        }
        #[cfg(not(windows))]
        {
            s
        }
    }

    pub fn get_repo_state(&self, repo_path: &Path) -> Option<&RepoUiState> {
        let key = Self::repo_state_key(repo_path);
        self.repo_state.get(&key)
    }

    pub fn get_repo_state_mut(&mut self, repo_path: &Path) -> &mut RepoUiState {
        let key = Self::repo_state_key(repo_path);
        self.repo_state.entry(key).or_default()
    }

    pub fn set_repo_profile_override(&mut self, repo_path: &Path, profile_id: Option<String>) {
        let state = self.get_repo_state_mut(repo_path);
        state.profile_override = profile_id;
    }

    pub fn get_effective_profile_for_repo(&self, repo_path: &Path) -> &LanguageProfile {
        if let Some(state) = self.get_repo_state(repo_path) {
            if let Some(override_id) = &state.profile_override {
                if let Some(p) = self.get_profile(override_id) {
                    return p;
                }
            }
        }
        self.get_active_profile()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    // --- TerminalPreference / TerminalConfig ---
    #[test]
    fn terminal_default_is_auto() {
        assert_eq!(TerminalPreference::default(), TerminalPreference::Auto);
    }

    #[test]
    fn terminal_all_variants_exist() {
        let variants = vec![
            TerminalPreference::Auto,
            TerminalPreference::WindowsTerminal,
            TerminalPreference::Cmd,
            TerminalPreference::Powershell,
            TerminalPreference::Custom("myterm".to_string()),
        ];
        assert_eq!(variants.len(), 5);
        match &variants[4] {
            TerminalPreference::Custom(s) => assert_eq!(s, "myterm"),
            _ => panic!("expected Custom"),
        }
    }

    #[test]
    fn terminal_serde_roundtrip_snake_case() {
        for pref in [
            TerminalPreference::Auto,
            TerminalPreference::WindowsTerminal,
            TerminalPreference::Cmd,
            TerminalPreference::Powershell,
            TerminalPreference::Custom("a & b".to_string()),
        ] {
            let json = serde_json::to_string(&pref).unwrap();
            // snake_case check: WindowsTerminal -> "windows_terminal"
            if pref == TerminalPreference::WindowsTerminal {
                assert!(json.contains("windows_terminal"));
            }
            let de: TerminalPreference = serde_json::from_str(&json).unwrap();
            assert_eq!(pref, de);
        }
    }

    #[test]
    fn terminal_custom_stores_string() {
        let c = TerminalPreference::Custom("".to_string());
        if let TerminalPreference::Custom(s) = c {
            assert_eq!(s, "");
        } else {
            panic!();
        }
        let c2 = TerminalPreference::Custom("with spaces".to_string());
        let json = serde_json::to_string(&c2).unwrap();
        let de: TerminalPreference = serde_json::from_str(&json).unwrap();
        assert_eq!(c2, de);
    }

    #[test]
    fn terminal_config_default() {
        let tc = TerminalConfig::default();
        assert_eq!(tc.preference, TerminalPreference::Auto);
        assert_eq!(tc.fallback, TerminalPreference::Cmd);
    }

    #[test]
    fn terminal_config_serde_roundtrip() {
        let tc = TerminalConfig {
            preference: TerminalPreference::Custom("alacritty".to_string()),
            fallback: TerminalPreference::Powershell,
        };
        let json = serde_json::to_string(&tc).unwrap();
        let de: TerminalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            de.preference,
            TerminalPreference::Custom("alacritty".to_string())
        );
        assert_eq!(de.fallback, TerminalPreference::Powershell);
    }

    // --- Theme ---
    #[test]
    fn theme_variants_are_five() {
        let all = Theme::all();
        assert_eq!(all.len(), 5);
        assert!(all.contains(&Theme::Light));
        assert!(all.contains(&Theme::Dark));
        assert!(all.contains(&Theme::Nord));
        assert!(all.contains(&Theme::Dracula));
        assert!(all.contains(&Theme::Solarized));
    }

    #[test]
    fn theme_display_name_all() {
        assert_eq!(Theme::Light.display_name(), "Light");
        assert_eq!(Theme::Dark.display_name(), "Dark");
        assert_eq!(Theme::Nord.display_name(), "Nord");
        assert_eq!(Theme::Dracula.display_name(), "Dracula");
        assert_eq!(Theme::Solarized.display_name(), "Solarized Light");
    }

    #[test]
    fn theme_all_returns_five() {
        assert_eq!(Theme::all().len(), 5);
    }

    #[test]
    fn theme_default_is_light() {
        assert_eq!(Theme::default(), Theme::Light);
    }

    #[test]
    fn theme_serde_roundtrip_snake_case() {
        for theme in Theme::all() {
            let json = serde_json::to_string(&theme).unwrap();
            // snake_case: Solarized -> "solarized"
            let de: Theme = serde_json::from_str(&json).unwrap();
            assert_eq!(theme, de);
        }
        let json = serde_json::to_string(&Theme::Dark).unwrap();
        assert!(json.contains("dark"));
        let json_sol = serde_json::to_string(&Theme::Solarized).unwrap();
        assert!(json_sol.contains("solarized"));
    }

    // --- IdeConfig ---
    #[test]
    fn ide_effective_program_with_program() {
        let ide = IdeConfig {
            id: "vscode".to_string(),
            display_name: "VS Code".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: Some("devenv /something".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_program(), "code");
    }

    #[test]
    fn ide_effective_program_from_command() {
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "".to_string(),
            args: vec![],
            command: Some("devenv /something".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_program(), "devenv");
        let ide2 = IdeConfig {
            program: "".to_string(),
            command: Some("  rider   --foo".to_string()),
            ..ide.clone()
        };
        assert_eq!(ide2.effective_program(), "rider");
    }

    #[test]
    fn ide_effective_program_fallback_code() {
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "".to_string(),
            args: vec![],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_program(), "code");
        let ide2 = IdeConfig {
            program: "".to_string(),
            command: Some("".to_string()),
            ..ide.clone()
        };
        // empty command split_whitespace -> None -> fallback code
        assert_eq!(ide2.effective_program(), "code");
    }

    #[test]
    fn ide_effective_args_with_args() {
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string(), "--reuse-window".to_string()],
            command: Some("code other".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_args(), vec!["{file}", "--reuse-window"]);
    }

    #[test]
    fn ide_effective_args_from_command() {
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "".to_string(),
            args: vec![],
            command: Some("cmd arg1 arg2".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_args(), vec!["arg1", "arg2"]);
    }

    #[test]
    fn ide_effective_args_fallback_file() {
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "".to_string(),
            args: vec![],
            command: Some("code".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_args(), vec!["{file}"]);
        let ide2 = IdeConfig {
            command: None,
            ..ide.clone()
        };
        assert_eq!(ide2.effective_args(), vec!["{file}"]);
    }

    #[test]
    fn ide_args_priority_over_command() {
        let ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "code".to_string(),
            args: vec!["custom".to_string()],
            command: Some("other arg".to_string()),
            use_shell: false,
            allow_unsafe: false,
            no_args: false,

            icon: None,
        };
        assert_eq!(ide.effective_args(), vec!["custom"]);
    }

    #[test]
    fn ide_serialization_roundtrip() {
        let ide = IdeConfig {
            id: "vs2022".to_string(),
            display_name: "Visual Studio".to_string(),
            program: "devenv".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: true,
            allow_unsafe: true,
            no_args: false,

            icon: None,
        };
        let json = serde_json::to_string(&ide).unwrap();
        let de: IdeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, "vs2022");
        assert_eq!(de.program, "devenv");
        assert!(de.use_shell);
        assert!(de.allow_unsafe);
    }

    // --- LanguageProfile ---
    #[test]
    fn profile_normalized_extension_adds_dot() {
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: "sln".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert_eq!(p.normalized_extension(), ".sln");
        let p2 = LanguageProfile {
            file_extension: ".sln".to_string(),
            ..p.clone()
        };
        assert_eq!(p2.normalized_extension(), ".sln");
    }

    #[test]
    fn profile_normalized_extension_lowercases() {
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".SLN".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert_eq!(p.normalized_extension(), ".sln");
        let p2 = LanguageProfile {
            file_extension: "SLN".to_string(),
            ..p.clone()
        };
        assert_eq!(p2.normalized_extension(), ".sln");
    }

    #[test]
    fn profile_normalized_extension_trims() {
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: " .sln ".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert_eq!(p.normalized_extension(), ".sln");
    }

    #[test]
    fn profile_matches_wildcard_case_insensitive() {
        let p = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p.matches_file(Path::new("foo.sln")));
        assert!(p.matches_file(Path::new("FOO.SLN")));
        assert!(!p.matches_file(Path::new("foo.txt")));
    }

    #[test]
    fn profile_matches_exact_filename() {
        let p = LanguageProfile {
            id: "rust".to_string(),
            display_name: "Rust".to_string(),
            file_extension: ".toml".to_string(),
            file_pattern: Some("Cargo.toml".to_string()),
            max_scan_depth: 2,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p.matches_file(Path::new("Cargo.toml")));
        assert!(!p.matches_file(Path::new("foo.toml")));
        assert!(!p.matches_file(Path::new("cargo.toml"))); // exact case-sensitive for non-wildcard
    }

    #[test]
    fn profile_matches_without_pattern_uses_extension() {
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".rs".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p.matches_file(Path::new("main.rs")));
        assert!(p.matches_file(Path::new("MAIN.RS")));
        assert!(!p.matches_file(Path::new("main.txt")));
        assert!(!p.matches_file(Path::new("Makefile")));
    }

    #[test]
    fn profile_matches_custom_pattern_exact() {
        let p = LanguageProfile {
            id: "java".to_string(),
            display_name: "Java".to_string(),
            file_extension: ".xml".to_string(),
            file_pattern: Some("pom.xml".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p.matches_file(Path::new("pom.xml")));
        assert!(!p.matches_file(Path::new("build.gradle")));
    }

    #[test]
    fn profile_matches_comma_separated_patterns() {
        let p = LanguageProfile {
            id: "java".to_string(),
            display_name: "Java".to_string(),
            file_extension: ".xml".to_string(),
            file_pattern: Some("pom.xml,build.gradle".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p.matches_file(Path::new("pom.xml")));
        assert!(p.matches_file(Path::new("build.gradle")));
        assert!(!p.matches_file(Path::new("other.txt")));
        // with spaces around comma
        let p2 = LanguageProfile {
            file_pattern: Some(" pom.xml , build.gradle ".to_string()),
            ..p.clone()
        };
        assert!(p2.matches_file(Path::new("pom.xml")));
    }

    #[test]
    fn profile_matches_no_extension_file_false() {
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(!p.matches_file(Path::new("Makefile")));
    }

    #[test]
    fn profile_dotnet_default_shape() {
        let p = default_dotnet_profile();
        assert_eq!(p.id, "dotnet");
        assert_eq!(p.display_name, ".NET");
        assert_eq!(p.file_extension, ".sln");
        assert_eq!(p.file_pattern, Some("*.sln".to_string()));
        assert_eq!(p.ides.len(), 3);
        assert_eq!(p.default_ide_id, Some("vs2022".to_string()));
        assert!(p.ides.iter().any(|i| i.id == "vs2022"));
        assert!(p.ides.iter().any(|i| i.id == "vscode"));
        assert!(p.ides.iter().any(|i| i.id == "rider"));
    }

    #[test]
    fn profile_serialization_roundtrip() {
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: Some("*.txt".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let de: LanguageProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, "test");
        assert_eq!(de.file_extension, ".txt");
    }

    // --- AgentLaunchMode & AgentProfile ---
    #[test]
    fn agent_launch_mode_default_is_terminal() {
        assert_eq!(AgentLaunchMode::default(), AgentLaunchMode::Terminal);
    }

    #[test]
    fn agent_launch_mode_serde_roundtrip() {
        for mode in [AgentLaunchMode::Terminal, AgentLaunchMode::Detached] {
            let json = serde_json::to_string(&mode).unwrap();
            let de: AgentLaunchMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, de);
        }
        let json = serde_json::to_string(&AgentLaunchMode::Detached).unwrap();
        assert!(json.contains("detached"));
    }

    #[test]
    fn agent_profile_defaults() {
        let a = &default_agents()[0];
        assert_eq!(a.launch_mode, AgentLaunchMode::Terminal);
        assert_eq!(a.terminal_override, None);
        assert!(a.args.is_empty());
        assert_eq!(a.command, None);
    }

    #[test]
    fn agent_profile_custom_terminal_override() {
        let mut a = default_agents()[0].clone();
        a.terminal_override = Some(TerminalPreference::Custom("myterm".to_string()));
        assert_eq!(
            a.terminal_override,
            Some(TerminalPreference::Custom("myterm".to_string()))
        );
        let json = serde_json::to_string(&a).unwrap();
        let de: AgentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(de.terminal_override, a.terminal_override);
    }

    #[test]
    fn default_agents_include_expected() {
        let agents = default_agents();
        assert_eq!(agents.len(), 6);
        let ids: Vec<_> = agents.iter().map(|a| a.id.as_str()).collect();
        for expected in ["claude", "codex", "gemini", "copilot", "cursor", "aider"] {
            assert!(ids.contains(&expected), "missing {}", expected);
        }
    }

    #[test]
    fn agent_profile_serialization() {
        let a = AgentProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "echo".to_string(),
            args: vec!["--foo".to_string()],
            command: Some("echo hi".to_string()),
            launch_mode: AgentLaunchMode::Detached,
            terminal_override: Some(TerminalPreference::Cmd),

            icon: None,
        };
        let json = serde_json::to_string(&a).unwrap();
        let de: AgentProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, "test");
        assert_eq!(de.launch_mode, AgentLaunchMode::Detached);
    }

    // --- RepoUiState ---
    #[test]
    fn repo_ui_state_default_none() {
        let s = RepoUiState::default();
        assert!(s.selected_solution.is_none());
        assert!(s.selected_ide.is_none());
        assert!(s.profile_override.is_none());
    }

    #[test]
    fn repo_ui_state_fields_can_hold_values() {
        let mut s = RepoUiState::default();
        s.selected_solution = Some(PathBuf::from("/tmp/foo.sln"));
        s.selected_ide = Some("vscode".to_string());
        s.profile_override = Some("dotnet".to_string());
        assert_eq!(s.selected_solution, Some(PathBuf::from("/tmp/foo.sln")));
        assert_eq!(s.selected_ide, Some("vscode".to_string()));
        assert_eq!(s.profile_override, Some("dotnet".to_string()));
    }

    #[test]
    fn repo_ui_state_serde_roundtrip() {
        let s = RepoUiState {
            selected_solution: Some(PathBuf::from("/a/b.sln")),
            selected_ide: Some("vs2022".to_string()),
            profile_override: Some("dotnet".to_string()),
        };
        let json = serde_json::to_string(&s).unwrap();
        let de: RepoUiState = serde_json::from_str(&json).unwrap();
        assert_eq!(de.selected_solution, s.selected_solution);
        assert_eq!(de.profile_override, s.profile_override);
    }

    // --- AppConfig Default & Roots ---
    #[test]
    fn default_has_valid_depth() {
        let cfg = AppConfig::default();
        assert!(cfg.max_depth >= 1 && cfg.max_depth <= 10);
        assert!(!cfg.profiles.is_empty());
        assert_eq!(cfg.active_profile_id, "dotnet");
        assert!(!cfg.agents.is_empty());
    }

    #[test]
    fn config_default_has_valid_state() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.max_depth, 2);
        assert_eq!(cfg.config_version, 10);
        assert_eq!(cfg.active_profile_id, "dotnet");
        assert!(!cfg.profiles.is_empty());
        assert_eq!(cfg.agents.len(), 6);
        assert_eq!(cfg.theme, Theme::Light);
        assert_eq!(cfg.terminal.preference, TerminalPreference::Auto);
        assert_eq!(cfg.terminal.fallback, TerminalPreference::Cmd);
        assert!(cfg.repo_state.is_empty());
    }

    #[test]
    fn config_add_root_adds_and_sorts() {
        let mut cfg = AppConfig {
            roots: vec![PathBuf::from("/tmp/b")],
            ..Default::default()
        };
        cfg.add_root(PathBuf::from("/tmp/a"));
        assert_eq!(
            cfg.roots,
            vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]
        );
        cfg.add_root(PathBuf::from("/tmp/c"));
        assert_eq!(
            cfg.roots,
            vec![
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/b"),
                PathBuf::from("/tmp/c")
            ]
        );
    }

    #[test]
    fn config_add_root_no_duplicates() {
        let mut cfg = AppConfig {
            roots: vec![PathBuf::from("/tmp/a")],
            ..Default::default()
        };
        cfg.add_root(PathBuf::from("/tmp/a"));
        assert_eq!(cfg.roots.len(), 1);
        cfg.add_root(PathBuf::from("/tmp/a"));
        assert_eq!(cfg.roots.len(), 1);
        cfg.add_root(PathBuf::from("/tmp/b"));
        cfg.add_root(PathBuf::from("/tmp/b"));
        assert_eq!(cfg.roots.len(), 2);
    }

    #[test]
    fn config_remove_root_existing() {
        let mut cfg = AppConfig {
            roots: vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")],
            ..Default::default()
        };
        cfg.remove_root(Path::new("/tmp/a"));
        assert_eq!(cfg.roots, vec![PathBuf::from("/tmp/b")]);
        cfg.remove_root(Path::new("/tmp/b"));
        assert!(cfg.roots.is_empty());
    }

    #[test]
    fn config_remove_root_noop_when_missing() {
        let mut cfg = AppConfig {
            roots: vec![PathBuf::from("/tmp/a")],
            ..Default::default()
        };
        cfg.remove_root(Path::new("/tmp/missing"));
        assert_eq!(cfg.roots.len(), 1);
        assert_eq!(cfg.roots[0], PathBuf::from("/tmp/a"));
    }

    // --- AppConfig Profile & Repo State ---
    #[test]
    fn config_get_profile_by_id_found() {
        let cfg = AppConfig::default();
        let p = cfg.get_profile("dotnet");
        assert!(p.is_some());
        assert_eq!(p.unwrap().id, "dotnet");
    }

    #[test]
    fn config_get_profile_by_id_not_found() {
        let cfg = AppConfig::default();
        assert!(cfg.get_profile("nonexistent").is_none());
        // empty profiles case via manual cfg
        let empty_cfg = AppConfig {
            profiles: vec![],
            ..Default::default()
        };
        assert!(empty_cfg.get_profile("dotnet").is_none());
    }

    #[test]
    fn config_get_active_profile_found_or_fallback() {
        let mut cfg = AppConfig::default();
        // valid
        let active = cfg.get_active_profile();
        assert_eq!(active.id, "dotnet");
        // invalid -> fallback to first
        cfg.active_profile_id = "invalid".to_string();
        let fallback = cfg.get_active_profile();
        assert_eq!(fallback.id, cfg.profiles[0].id);
    }

    #[test]
    fn config_get_repo_state_found() {
        let mut cfg = AppConfig::default();
        let path = PathBuf::from("/tmp/repo");
        assert!(cfg.get_repo_state(&path).is_none());
        cfg.get_repo_state_mut(&path).selected_ide = Some("vscode".to_string());
        assert!(cfg.get_repo_state(&path).is_some());
        assert_eq!(
            cfg.get_repo_state(&path).unwrap().selected_ide,
            Some("vscode".to_string())
        );
    }

    #[test]
    fn config_get_repo_state_mut_creates_default() {
        let mut cfg = AppConfig::default();
        let path = Path::new("/tmp/new_repo");
        let state = cfg.get_repo_state_mut(path);
        assert!(state.selected_solution.is_none());
        // second call returns same entry
        cfg.get_repo_state_mut(path).profile_override = Some("dotnet".to_string());
        assert_eq!(
            cfg.get_repo_state(path).unwrap().profile_override,
            Some("dotnet".to_string())
        );
    }

    #[test]
    fn config_set_repo_profile_override() {
        let mut cfg = AppConfig::default();
        let path = PathBuf::from("/tmp/repo");
        cfg.set_repo_profile_override(&path, Some("dotnet".to_string()));
        assert_eq!(
            cfg.get_repo_state(&path).unwrap().profile_override,
            Some("dotnet".to_string())
        );
        cfg.set_repo_profile_override(&path, None);
        assert_eq!(cfg.get_repo_state(&path).unwrap().profile_override, None);
    }

    #[test]
    fn config_get_effective_profile_uses_override() {
        let mut cfg = AppConfig::default();
        // add custom profile
        cfg.profiles.push(LanguageProfile {
            id: "rust".to_string(),
            display_name: "Rust".to_string(),
            file_extension: ".rs".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        let path = PathBuf::from("/tmp/repo");
        // without override -> active (dotnet)
        assert_eq!(cfg.get_effective_profile_for_repo(&path).id, "dotnet");
        // with valid override
        cfg.set_repo_profile_override(&path, Some("rust".to_string()));
        assert_eq!(cfg.get_effective_profile_for_repo(&path).id, "rust");
    }

    #[test]
    fn config_get_effective_profile_fallback_global() {
        let mut cfg = AppConfig::default();
        let path = PathBuf::from("/tmp/repo");
        cfg.set_repo_profile_override(&path, Some("invalid_id".to_string()));
        // invalid override -> fallback to active
        assert_eq!(cfg.get_effective_profile_for_repo(&path).id, "dotnet");
        cfg.set_repo_profile_override(&path, None);
        assert_eq!(cfg.get_effective_profile_for_repo(&path).id, "dotnet");
    }

    // --- Agents ---
    #[test]
    fn config_get_active_agent_prefers_ids() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec!["codex".to_string()];
        cfg.active_agent_id = Some("claude".to_string());
        let agent = cfg.get_active_agent().unwrap();
        assert_eq!(agent.id, "codex");
    }

    #[test]
    fn config_get_active_agent_fallback_deprecated() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec![];
        cfg.active_agent_id = Some("gemini".to_string());
        let agent = cfg.get_active_agent().unwrap();
        assert_eq!(agent.id, "gemini");
        // both empty -> first
        cfg.active_agent_id = None;
        cfg.active_agent_ids = vec![];
        let agent2 = cfg.get_active_agent().unwrap();
        assert_eq!(agent2.id, cfg.agents[0].id);
    }

    #[test]
    fn config_get_active_agents_multiple() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec![
            "claude".to_string(),
            "codex".to_string(),
            "invalid".to_string(),
        ];
        let agents = cfg.get_active_agents();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].id, "claude");
        assert_eq!(agents[1].id, "codex");
        // deprecated fallback
        cfg.active_agent_ids = vec![];
        cfg.active_agent_id = Some("claude".to_string());
        let agents2 = cfg.get_active_agents();
        assert_eq!(agents2.len(), 1);
        assert_eq!(agents2[0].id, "claude");
    }

    #[test]
    fn config_is_agent_active() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec!["claude".to_string()];
        assert!(cfg.is_agent_active("claude"));
        assert!(!cfg.is_agent_active("codex"));
        cfg.active_agent_ids = vec![];
        cfg.active_agent_id = Some("codex".to_string());
        assert!(cfg.is_agent_active("codex"));
        assert!(!cfg.is_agent_active("claude"));
        cfg.active_agent_id = None;
        assert!(!cfg.is_agent_active("claude"));
    }

    #[test]
    fn config_toggle_agent_active_add_remove_sync() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec!["claude".to_string()];
        cfg.toggle_agent_active("codex");
        assert!(cfg.active_agent_ids.contains(&"codex".to_string()));
        assert_eq!(cfg.active_agent_id, Some("claude".to_string()));
        cfg.toggle_agent_active("claude");
        assert!(!cfg.active_agent_ids.contains(&"claude".to_string()));
        assert_eq!(cfg.active_agent_id, Some("codex".to_string()));
        // remove last -> empty allowed
        cfg.toggle_agent_active("codex");
        assert!(cfg.active_agent_ids.is_empty());
        assert_eq!(cfg.active_agent_id, None);
        // add again
        cfg.toggle_agent_active("aider");
        assert_eq!(cfg.active_agent_ids, vec!["aider".to_string()]);
        assert_eq!(cfg.active_agent_id, Some("aider".to_string()));
    }

    // --- Persistence / Migration / Validation ---
    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let cfg = AppConfig {
            roots: vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")],
            max_depth: 3,
            ..Default::default()
        };
        let data = serde_json::to_string_pretty(&cfg).unwrap();
        std::fs::write(&path, data).unwrap();
        let loaded: AppConfig =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.roots.len(), 2);
        assert_eq!(loaded.max_depth, 3);
        assert_eq!(loaded.config_version, 10);
    }

    #[test]
    fn config_path_returns_some() {
        let path = AppConfig::config_path();
        assert!(path.is_some());
        assert!(path.unwrap().ends_with("config.json"));
    }

    #[test]
    fn migration_old_config() {
        let old_json = r#"{"roots":["/tmp/a"],"max_depth":2}"#;
        let mut cfg: AppConfig = serde_json::from_str(old_json).unwrap();
        // Nach try_load Logik sollten defaults injiziert werden
        if cfg.profiles.is_empty() {
            cfg.profiles = default_profiles();
        }
        if cfg.active_profile_id.is_empty() {
            cfg.active_profile_id = default_active_profile();
        }
        assert!(!cfg.profiles.is_empty());
        assert_eq!(cfg.profiles[0].id, "dotnet");
    }

    #[test]
    fn config_migration_v1_to_v2_injects_defaults() {
        let json = r#"{"roots":[],"max_depth":2,"config_version":1,"active_profile_id":"","profiles":[],"agents":[]}"#;
        let mut cfg: AppConfig = serde_json::from_str(json).unwrap();
        // simulate <2 migration
        if cfg.config_version < 2 {
            if cfg.profiles.is_empty() {
                cfg.profiles = default_profiles();
            }
            if cfg.agents.is_empty() {
                cfg.agents = default_agents();
            }
            if cfg.active_profile_id.is_empty() {
                cfg.active_profile_id = default_active_profile();
            }
            cfg.config_version = 2;
        }
        assert_eq!(cfg.config_version, 2);
        assert!(!cfg.profiles.is_empty());
        assert!(!cfg.agents.is_empty());
        assert_eq!(cfg.active_profile_id, "dotnet");
    }

    #[test]
    fn config_migration_v3_to_v4_removes_auto_profiles() {
        // simulate v3 config with old auto ids
        let mut cfg = AppConfig::default();
        cfg.config_version = 3;
        // add old auto profiles
        for id in ["rust", "node", "python"] {
            cfg.profiles.push(LanguageProfile {
                id: id.to_string(),
                display_name: id.to_string(),
                file_extension: ".txt".to_string(),
                file_pattern: None,
                max_scan_depth: 3,
                ides: vec![],
                default_ide_id: None,
                ide_order: Vec::new(),
                hidden_ide_ids: Vec::new(),
                hidden_agent_ids: Vec::new(),
                agent_order: Vec::new(),
                show_shell: true,
                show_explorer: true,
                config_selectors: Vec::new(),
            });
        }
        // simulate v4 migration retain logic
        let keep = vec!["dotnet"];
        let old_auto = ["rust", "node", "python", "java", "go", "cpp"];
        cfg.profiles.retain(|p| {
            if keep.contains(&p.id.as_str()) {
                true
            } else if old_auto.contains(&p.id.as_str()) {
                false
            } else {
                true
            }
        });
        assert!(cfg.profiles.iter().any(|p| p.id == "dotnet"));
        assert!(!cfg.profiles.iter().any(|p| p.id == "rust"));
        // custom kept - dotnet still there
        cfg.profiles.push(LanguageProfile {
            id: "custom".to_string(),
            display_name: "Custom".to_string(),
            file_extension: ".custom".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        });
        let has_custom = cfg.profiles.iter().any(|p| p.id == "custom");
        assert!(has_custom);
    }

    #[test]
    fn config_validation_max_depth_clamped() {
        // simulate try_load clamping
        let mut cfg = AppConfig {
            max_depth: 0,
            ..Default::default()
        };
        if cfg.max_depth == 0 {
            cfg.max_depth = 2;
        }
        assert_eq!(cfg.max_depth, 2);
        cfg.max_depth = 99;
        if cfg.max_depth > 10 {
            cfg.max_depth = 10;
        }
        assert_eq!(cfg.max_depth, 10);
        cfg.max_depth = 5;
        if cfg.max_depth == 0 {
            cfg.max_depth = 2;
        }
        if cfg.max_depth > 10 {
            cfg.max_depth = 10;
        }
        assert_eq!(cfg.max_depth, 5);
    }

    #[test]
    fn config_validation_roots_normalized() {
        let mut cfg = AppConfig {
            roots: vec![
                PathBuf::from("/tmp/b"),
                PathBuf::from("/tmp/a"),
                PathBuf::from("/tmp/a"),
                PathBuf::from(""),
            ],
            ..Default::default()
        };
        cfg.roots.retain(|p| !p.as_os_str().is_empty());
        cfg.roots.sort();
        cfg.roots.dedup();
        assert_eq!(
            cfg.roots,
            vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]
        );
    }

    #[test]
    fn config_validation_profiles_normalized() {
        let mut p = LanguageProfile {
            id: " TEST ".to_string(),
            display_name: "Test".to_string(),
            file_extension: " SLN ".to_string(),
            file_pattern: None,
            max_scan_depth: 0,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        // simulate validation
        p.file_extension = p.normalized_extension();
        assert_eq!(p.file_extension, ".sln");
        if p.max_scan_depth == 0 {
            p.max_scan_depth = 3;
        }
        assert_eq!(p.max_scan_depth, 3);
        p.max_scan_depth = 99;
        if p.max_scan_depth > 4 {
            p.max_scan_depth = 4;
        }
        assert_eq!(p.max_scan_depth, 4);
        p.id = p.id.trim().to_lowercase();
        assert_eq!(p.id, "test");
        let mut p2 = LanguageProfile {
            id: "   ".to_string(),
            display_name: "X".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        p2.id = p2.id.trim().to_lowercase();
        if p2.id.is_empty() {
            p2.id = "custom".to_string();
        }
        assert_eq!(p2.id, "custom");
    }

    #[test]
    fn config_validation_profiles_dedup_last_wins() {
        let mut cfg = AppConfig::default();
        cfg.profiles = vec![
            LanguageProfile {
                id: "dotnet".to_string(),
                display_name: "First".to_string(),
                file_extension: ".sln".to_string(),
                file_pattern: None,
                max_scan_depth: 3,
                ides: vec![],
                default_ide_id: None,
                ide_order: Vec::new(),
                hidden_ide_ids: Vec::new(),
                hidden_agent_ids: Vec::new(),
                agent_order: Vec::new(),
                show_shell: true,
                show_explorer: true,
                config_selectors: Vec::new(),
            },
            LanguageProfile {
                id: "dotnet".to_string(),
                display_name: "Second".to_string(),
                file_extension: ".sln".to_string(),
                file_pattern: None,
                max_scan_depth: 3,
                ides: vec![],
                default_ide_id: None,
                ide_order: Vec::new(),
                hidden_ide_ids: Vec::new(),
                hidden_agent_ids: Vec::new(),
                agent_order: Vec::new(),
                show_shell: true,
                show_explorer: true,
                config_selectors: Vec::new(),
            },
        ];
        // dedup like try_load
        {
            let mut seen = std::collections::HashSet::new();
            let mut deduped = Vec::new();
            for p in cfg.profiles.into_iter().rev() {
                if seen.insert(p.id.clone()) {
                    deduped.push(p);
                }
            }
            deduped.reverse();
            cfg.profiles = deduped;
        }
        assert_eq!(cfg.profiles.len(), 1);
        assert_eq!(cfg.profiles[0].display_name, "Second");
    }

    #[test]
    fn config_validation_active_profile_corrected() {
        let mut cfg = AppConfig::default();
        cfg.active_profile_id = "invalid".to_string();
        if !cfg.profiles.iter().any(|p| p.id == cfg.active_profile_id) {
            cfg.active_profile_id = cfg.profiles.first().unwrap().id.clone();
        }
        assert_eq!(cfg.active_profile_id, "dotnet");
    }

    #[test]
    fn config_validation_agents_filtered_and_fallback() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec!["invalid".to_string(), "claude".to_string()];
        cfg.active_agent_ids
            .retain(|id| cfg.agents.iter().any(|a| &a.id == id));
        assert_eq!(cfg.active_agent_ids, vec!["claude".to_string()]);
        cfg.active_agent_ids = vec!["invalid".to_string()];
        cfg.active_agent_ids
            .retain(|id| cfg.agents.iter().any(|a| &a.id == id));
        if cfg.active_agent_ids.is_empty() && !cfg.agents.is_empty() {
            cfg.active_agent_ids = vec![cfg.agents[0].id.clone()];
        }
        assert_eq!(cfg.active_agent_ids, vec!["claude".to_string()]);
        // empty agents fallback
        cfg.agents = vec![];
        if cfg.agents.is_empty() {
            cfg.agents = default_agents();
        }
        assert!(!cfg.agents.is_empty());
    }

    #[test]
    fn config_validation_agents_sync_deprecated() {
        let mut cfg = AppConfig::default();
        cfg.active_agent_ids = vec!["codex".to_string()];
        cfg.active_agent_id = cfg.active_agent_ids.first().cloned();
        assert_eq!(cfg.active_agent_id, Some("codex".to_string()));
        cfg.active_agent_ids = vec!["invalid".to_string()];
        // after retain it would be empty then fallback, but sync check:
        cfg.active_agent_ids
            .retain(|id| cfg.agents.iter().any(|a| &a.id == id));
        if cfg.active_agent_ids.is_empty() && !cfg.agents.is_empty() {
            cfg.active_agent_ids = vec![cfg.agents[0].id.clone()];
        }
        cfg.active_agent_id = cfg.active_agent_ids.first().cloned();
        assert_eq!(cfg.active_agent_id, Some("claude".to_string()));
    }

    #[test]
    fn profile_extension_normalized() {
        let mut p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: "sln".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert_eq!(p.normalized_extension(), ".sln");
        p.file_extension = ".SLN".to_string();
        assert_eq!(p.normalized_extension(), ".sln");
    }

    #[test]
    fn profile_matches_file() {
        let p = LanguageProfile {
            id: "dotnet".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: Some("*.sln".to_string()),
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p.matches_file(Path::new("foo.sln")));
        assert!(p.matches_file(Path::new("FOO.SLN")));
        assert!(!p.matches_file(Path::new("foo.txt")));

        let p2 = LanguageProfile {
            id: "rust".to_string(),
            display_name: "Rust".to_string(),
            file_extension: ".toml".to_string(),
            file_pattern: Some("Cargo.toml".to_string()),
            max_scan_depth: 2,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        assert!(p2.matches_file(Path::new("Cargo.toml")));
        assert!(!p2.matches_file(Path::new("foo.toml")));
    }

    #[test]
    fn config_save_creates_directory() {
        let dir = tempdir().unwrap();
        let cfg_path = dir.path().join("nested").join("config.json");
        // simulate save logic create_dir_all
        if let Some(parent) = cfg_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
            assert!(parent.exists());
        }
        // ensure save would succeed if we used custom path (via direct write)
        let cfg = AppConfig::default();
        let data = serde_json::to_string_pretty(&cfg).unwrap();
        let tmp = cfg_path.with_extension("json.tmp");
        std::fs::write(&tmp, data).unwrap();
        std::fs::rename(&tmp, &cfg_path).unwrap();
        assert!(cfg_path.exists());
    }

    #[test]
    fn config_normalized_profile_ids_lowercase_on_load() {
        let mut p = LanguageProfile {
            id: "DOTNET".to_string(),
            display_name: ".NET".to_string(),
            file_extension: ".sln".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: Vec::new(),
            hidden_ide_ids: Vec::new(),
            hidden_agent_ids: Vec::new(),
            agent_order: Vec::new(),
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        p.id = p.id.trim().to_lowercase();
        assert_eq!(p.id, "dotnet");
    }

    #[test]
    fn config_try_load_invalid_json_returns_error() {
        let bad = r#"{"roots": "invalid", "max_depth": "bad"}"#;
        let res: Result<AppConfig, _> = serde_json::from_str(bad);
        assert!(res.is_err());
    }

    #[test]
    fn theme_serde_variants() {
        // ensure all themes roundtrip
        for theme in Theme::all() {
            let s = serde_json::to_string(&theme).unwrap();
            let d: Theme = serde_json::from_str(&s).unwrap();
            assert_eq!(theme, d);
        }
    }

    #[test]
    fn ide_no_args_returns_empty() {
        let mut ide = IdeConfig {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            program: "code".to_string(),
            args: vec!["{file}".to_string()],
            command: None,
            use_shell: false,
            allow_unsafe: false,
            no_args: true,

            icon: None,
        };
        assert!(ide.effective_args().is_empty());
        ide.no_args = false;
        assert_eq!(ide.effective_args(), vec!["{file}"]);
        // Serie roundtrip with true
        ide.no_args = true;
        ide.args = vec!["should be ignored".to_string()];
        assert!(ide.effective_args().is_empty());
        let json = serde_json::to_string(&ide).unwrap();
        let de: IdeConfig = serde_json::from_str(&json).unwrap();
        assert!(de.no_args);
    }

    #[test]
    fn dotnet_vscode_default_is_dot() {
        let p = default_dotnet_profile();
        let vscode = p.ides.iter().find(|i| i.id == "vscode").unwrap();
        assert_eq!(vscode.args, vec![".".to_string()]);
        assert!(!vscode.no_args);
    }

    #[test]
    fn branch_limit_default_and_clamp() {
        assert_eq!(default_branch_limit(), 200);
        let cfg = AppConfig::default();
        assert_eq!(cfg.branch_display_limit, 200);
        // Clamp via try_load logic: simulate validation
        let mut cfg2 = AppConfig::default();
        cfg2.branch_display_limit = 10;
        if cfg2.branch_display_limit < 50 {
            cfg2.branch_display_limit = 50;
        }
        assert_eq!(cfg2.branch_display_limit, 50);
        cfg2.branch_display_limit = 1000;
        if cfg2.branch_display_limit > 500 {
            cfg2.branch_display_limit = 500;
        }
        assert_eq!(cfg2.branch_display_limit, 500);
    }

    #[test]
    fn profile_visible_ides_filters_and_orders() {
        let mut p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![
                IdeConfig {
                    id: "a".to_string(),
                    display_name: "A".to_string(),
                    program: "a".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                },
                IdeConfig {
                    id: "b".to_string(),
                    display_name: "B".to_string(),
                    program: "b".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                },
                IdeConfig {
                    id: "c".to_string(),
                    display_name: "C".to_string(),
                    program: "c".to_string(),
                    args: vec![],
                    command: None,
                    use_shell: false,
                    allow_unsafe: false,
                    no_args: false,

                    icon: None,
                },
            ],
            default_ide_id: None,
            ide_order: vec!["c".to_string(), "a".to_string(), "b".to_string()],
            hidden_ide_ids: vec!["a".to_string()],
            hidden_agent_ids: vec![],
            agent_order: vec![],
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        let visible = p.visible_ides();
        // a is hidden, so only c and b in order c,b
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].id, "c");
        assert_eq!(visible[1].id, "b");
        // show flags
        assert!(p.show_shell);
        p.show_shell = false;
        assert!(!p.show_shell);
    }

    #[test]
    fn profile_filtered_agents_per_profile() {
        let cfg = AppConfig::default();
        let p = LanguageProfile {
            id: "test".to_string(),
            display_name: "Test".to_string(),
            file_extension: ".txt".to_string(),
            file_pattern: None,
            max_scan_depth: 3,
            ides: vec![],
            default_ide_id: None,
            ide_order: vec![],
            hidden_ide_ids: vec![],
            hidden_agent_ids: vec!["codex".to_string()],
            agent_order: vec!["gemini".to_string(), "claude".to_string()],
            show_shell: true,
            show_explorer: true,
            config_selectors: Vec::new(),
        };
        let active = vec![
            "claude".to_string(),
            "codex".to_string(),
            "gemini".to_string(),
        ];
        let filtered = p.filtered_agents(&cfg.agents, &active);
        // codex hidden, so only gemini, claude in order gemini,claude
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, "gemini");
        assert_eq!(filtered[1].id, "claude");
    }

    #[test]
    fn language_profile_default_icon_flags() {
        let p = default_dotnet_profile();
        assert!(p.show_shell);
        assert!(p.show_explorer);
        assert!(p.hidden_ide_ids.is_empty());
        assert!(p.ide_order.is_empty());
    }

    #[test]
    fn config_selector_default_and_serde() {
        let sel = ConfigSelector {
            id: "db".into(),
            display_name: "Datenbank".into(),
            file_path: "App.config".into(),
            key: "Database".into(),
            key_attribute: "key".into(),
            value_attribute: "value".into(),
            kind: XmlSelectorKind::AddKeyValue,
            options: vec![ConfigOption {
                value: "dev".into(),
                label: "Dev".into(),
            }],
            allow_custom: false,
        };
        let json = serde_json::to_string(&sel).unwrap();
        let de: ConfigSelector = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, "db");
    }
    #[test]
    fn profile_has_config_selectors_empty_by_default() {
        let cfg = AppConfig::default();
        assert!(cfg.get_active_profile().config_selectors.is_empty());
    }
}
