//! IDE, Agent, Terminal und Explorer starten (Platzhalter, Quoting,
//! Terminal-Auswahl).

// Die eigentlichen Starts sind `cfg(not(test))`: im Test-Build ungenutzt.
#[cfg(not(test))]
use super::is_program_available;
use super::{is_rider_program, is_vs_program, resolve_rider_path, resolve_vs_path};
use crate::config::{AgentProfile, IdeConfig, TerminalPreference};
#[cfg(not(test))]
use crate::process;
use crate::process::{set_creation_flags, SpawnLocked, CREATE_NO_WINDOW};
use std::path::{Path, PathBuf};

/// Öffnet den Explorer / Dateimanager im Repo-Verzeichnis
pub fn open_in_explorer(path: &Path) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        // In tests: don't actually spawn file manager windows
        // Just verify path handling, return Ok to avoid stray windows staying open
        let _ = path;
        Ok(())
    }
    #[cfg(not(test))]
    {
        #[cfg(windows)]
        {
            // explorer.exe versteht nur `\`: mit `/` (Worktree-Pfade aus
            // libgit2) öffnet er stattdessen „Dokumente“
            let path = path.to_string_lossy().replace('/', "\\");
            std::process::Command::new("explorer")
                .arg(&path)
                .spawn_locked()
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("Explorer konnte nicht geöffnet werden: {e}"))
        }
        #[cfg(not(windows))]
        {
            // Linux: xdg-open, WSL: explorer.exe, macOS: open – plus gio als Fallback
            std::process::Command::new("xdg-open")
                .arg(path)
                .spawn_locked()
                .or_else(|_| {
                    std::process::Command::new("explorer.exe")
                        .arg(path)
                        .spawn_locked()
                })
                .or_else(|_| std::process::Command::new("open").arg(path).spawn_locked())
                .or_else(|_| {
                    std::process::Command::new("gio")
                        .args(["open", &path.display().to_string()])
                        .spawn_locked()
                })
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("Dateimanager konnte nicht geöffnet werden: {e}"))
        }
    }
}

pub(crate) fn substitute_placeholders(template: &str, file: &Path, repo: &Path) -> String {
    let file_str = file.display().to_string();
    let repo_str = repo.display().to_string();
    // {dir} = parent of file (solution dir) when file != repo, else repo itself (for agent case where file==repo)
    let dir_str = if file == repo {
        repo_str.clone()
    } else {
        file.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| repo_str.clone())
    };
    template
        .replace("{file}", &file_str)
        .replace("{dir}", &dir_str)
        .replace("{repo}", &repo_str)
}

pub(super) fn quote_if_needed(s: &str) -> String {
    if s.contains(' ') && !(s.starts_with('"') && s.ends_with('"')) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

pub(super) fn escape_for_powershell(s: &str) -> String {
    let needs_quote = s.contains(' ')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('&')
        || s.contains('|')
        || s.contains(';')
        || s.contains('>')
        || s.contains('<')
        || s.contains('^')
        || s.contains('`')
        || s.contains('$');
    if needs_quote {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

#[cfg(not(test))]
fn escape_for_powershell_force(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn escape_for_cmd(s: &str) -> String {
    // Für cmd /K: wrap in double quotes if contains space or shell meta, escape inner quotes
    let needs_quote = s.contains(' ')
        || s.contains('&')
        || s.contains('|')
        || s.contains('<')
        || s.contains('>')
        || s.contains('^')
        || s.contains('"');
    if needs_quote {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn has_placeholders(s: &str) -> bool {
    s.contains("{file}") || s.contains("{dir}") || s.contains("{repo}")
}

fn build_agent_command<F>(
    agent: &AgentProfile,
    repo_path: &Path,
    escape_fn: F,
    prefix: &str,
    substitute_args: bool,
) -> String
where
    F: Fn(&str) -> String,
{
    if !agent.args.is_empty() {
        let escaped_program = escape_fn(&agent.program);
        let args_str = agent
            .args
            .iter()
            .map(|a| {
                let substituted = if substitute_args {
                    substitute_placeholders(a, repo_path, repo_path)
                } else {
                    a.clone()
                };
                escape_fn(&substituted)
            })
            .collect::<Vec<_>>()
            .join(" ");
        if args_str.is_empty() {
            format!("{prefix}{escaped_program}")
        } else {
            format!("{prefix}{escaped_program} {args_str}")
        }
    } else if let Some(cmd) = &agent.command {
        if has_placeholders(cmd) {
            substitute_placeholders(cmd, repo_path, repo_path)
        } else {
            cmd.clone()
        }
    } else {
        format!("{}{}", prefix, escape_fn(&agent.program))
    }
}

fn build_agent_cmd_raw(agent: &AgentProfile, repo_path: &Path) -> String {
    // Wird nur für TerminalPreference::Custom via `cmd.arg(&agent_cmd_raw)` verwendet
    // (`src/git.rs:841`). `Command::arg` übergibt ohne Shell-Parsing – zusätzliche
    // Quotes würden literal Teil des Arguments (z. B. `"C:\Program Files\..."`).
    // Daher program NICHT quoten (ursprüngliches Verhalten: `program.clone()`).
    if !agent.args.is_empty() {
        let args_str = agent
            .args
            .iter()
            .map(|a| {
                let substituted = substitute_placeholders(a, repo_path, repo_path);
                quote_if_needed(&substituted)
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {}", agent.program, args_str)
    } else if let Some(cmd) = &agent.command {
        if has_placeholders(cmd) {
            substitute_placeholders(cmd, repo_path, repo_path)
        } else {
            cmd.clone()
        }
    } else {
        agent.program.clone()
    }
}

pub(super) fn build_powershell_script(agent: &AgentProfile, repo_path: &Path) -> String {
    let prog = &agent.program;
    let needs_amp =
        prog.contains(' ') || prog.contains('\\') || prog.contains('/') || prog.contains('\'');
    let prefix = if needs_amp { "& " } else { "" };
    build_agent_command(agent, repo_path, escape_for_powershell, prefix, true)
}

fn build_cmd_agent_string(agent: &AgentProfile, repo_path: &Path) -> String {
    build_agent_command(agent, repo_path, escape_for_cmd, "", true)
}

/// Öffnet eine Datei/Ordner mit einer IDE-Konfiguration
/// `repo_path` ist immer das Repo-Root (für cwd), `file` ist optional die Solution/Project Datei
pub fn launch_ide(ide: &IdeConfig, repo_path: &Path, file: Option<&Path>) -> anyhow::Result<()> {
    let program = ide.effective_program();
    let args_template = ide.effective_args();
    let file_for_subst: PathBuf = file
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo_path.to_path_buf());
    let args: Vec<String> = args_template
        .iter()
        .map(|a| substitute_placeholders(a, &file_for_subst, repo_path))
        .collect();

    let mut effective_program = program.clone();
    if is_vs_program(&program) {
        if let Some(vs_path) = resolve_vs_path() {
            effective_program = vs_path.to_string_lossy().to_string();
        }
    } else if is_rider_program(&program) {
        // Konsistent zu VS: immer versuchen aufzulösen, nicht nur wenn !is_program_available
        if let Some(rider_path) = resolve_rider_path() {
            effective_program = rider_path.to_string_lossy().to_string();
        }
    }

    let spawn = |prog: &str, args: &[String]| -> std::io::Result<std::process::Child> {
        let mut cmd = std::process::Command::new(prog);
        cmd.args(args);
        cmd.current_dir(repo_path);
        set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
        cmd.spawn_locked()
    };

    // Validierung: nur wenn Shell verwendet wird (use_shell) und nicht allow_unsafe, blockiere gefährliche Zeichen
    // Bei use_shell=false werden args via Command::args ohne Shell-Parsing übergeben – Zeichen wie '&' in Pfaden
    // wie "C:\Users\Tom & Jerry\app.sln" sind dann harmlos und dürfen nicht blockiert werden.
    // Bei use_shell=true wird via cmd /C gestartet und Shell-Zeichen müssen blockiert werden.
    let shell_chars = ['&', '|', ';', '>', '<'];
    if ide.use_shell && !ide.allow_unsafe {
        for a in &args {
            if a.chars().any(|c| shell_chars.contains(&c)) {
                anyhow::bail!("Ungültige Zeichen in IDE-Args (shell-Zeichen) – aktiviere allow_unsafe wenn beabsichtigt: {a}");
            }
        }
        if effective_program.chars().any(|c| shell_chars.contains(&c)) {
            anyhow::bail!(
                "Ungültige Zeichen in IDE-Programm (shell-Zeichen) – aktiviere allow_unsafe wenn beabsichtigt: {}",
                effective_program
            );
        }
    }

    #[cfg(test)]
    {
        // In tests: don't actually spawn GUI IDEs (VS Code, Visual Studio, Rider)
        // This prevents VS Code windows staying open after `cargo test` on dev machines
        let lower = effective_program.to_lowercase();
        if lower.contains("code") || lower.contains("devenv") || lower.contains("rider") {
            return Ok(());
        }
    }

    match spawn(&effective_program, &args) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(windows)]
            {
                // Fallback via cmd /C verwendet eine Shell – daher bei !allow_unsafe Shell-Zeichen blockieren,
                // unabhängig von use_shell, da hier immer eine Shell involviert ist.
                if !ide.allow_unsafe {
                    for a in &args {
                        if a.chars().any(|c| shell_chars.contains(&c)) {
                            anyhow::bail!("Ungültige Zeichen in IDE-Args für Shell-Fallback (allow_unsafe erforderlich): {a}");
                        }
                    }
                    if effective_program.chars().any(|c| shell_chars.contains(&c)) {
                        anyhow::bail!(
                            "Ungültige Zeichen in IDE-Programm für Shell-Fallback (allow_unsafe erforderlich): {}",
                            effective_program
                        );
                    }
                }
                let mut shell_args = vec!["/C".to_string(), effective_program.clone()];
                shell_args.extend(args.clone());
                let mut cmd = std::process::Command::new("cmd");
                cmd.args(&shell_args);
                cmd.current_dir(repo_path);
                set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
                cmd.spawn_locked().map(|_| ()).map_err(|e2| {
                    anyhow::anyhow!(
                        "IDE nicht gefunden (weder '{}' noch 'cmd /C'): {} / {}",
                        effective_program,
                        e,
                        e2
                    )
                })
            }
            #[cfg(not(windows))]
            {
                Err(anyhow::anyhow!(
                    "Programm '{}' nicht im PATH: {}",
                    effective_program,
                    e
                ))
            }
        }
        Err(e) => Err(anyhow::anyhow!(
            "Konnte IDE '{}' nicht starten: {}",
            effective_program,
            e
        )),
    }
}

#[cfg(not(test))]
fn spawn_wt(repo_str: &str, cmd_string: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("wt");
    cmd.args(["-d", repo_str, "--", "cmd", "/k", cmd_string]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, process::CREATE_NEW_CONSOLE);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Windows Terminal starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_powershell(ps_script: &str, repo_path: &Path) -> anyhow::Result<()> {
    let has_pwsh = is_program_available("pwsh");
    let ps = if has_pwsh { "pwsh" } else { "powershell" };
    let mut cmd = std::process::Command::new(ps);
    if has_pwsh {
        cmd.args([
            "-NoLogo",
            "-NoExit",
            "-WorkingDirectory",
            &repo_path.display().to_string(),
            "-Command",
            ps_script,
        ]);
    } else {
        let repo_escaped = escape_for_powershell_force(&repo_path.display().to_string());
        let full_script = format!("Set-Location {}; {}", repo_escaped, ps_script);
        cmd.args(["-NoLogo", "-NoExit", "-Command", &full_script]);
    }
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, process::CREATE_NEW_CONSOLE);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Powershell starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_powershell_shell(repo_path: &Path) -> anyhow::Result<()> {
    let has_pwsh = is_program_available("pwsh");
    let ps = if has_pwsh { "pwsh" } else { "powershell" };
    let mut cmd = std::process::Command::new(ps);
    if has_pwsh {
        cmd.args([
            "-NoLogo",
            "-NoExit",
            "-WorkingDirectory",
            &repo_path.display().to_string(),
        ]);
    } else {
        cmd.args([
            "-NoLogo",
            "-NoExit",
            "-Command",
            &format!(
                "Set-Location {}",
                escape_for_powershell_force(&repo_path.display().to_string())
            ),
        ]);
    }
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, process::CREATE_NEW_CONSOLE);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Powershell starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_wt_shell(repo_path: &Path) -> anyhow::Result<()> {
    let repo_str = repo_path.display().to_string();
    let mut cmd = std::process::Command::new("wt");
    cmd.args(["-d", &repo_str]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, process::CREATE_NEW_CONSOLE);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("Windows Terminal starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_cmd_shell(repo_str: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", "/D", repo_str, "cmd", "/K", ""]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("cmd starten fehlgeschlagen: {e}"))
}

#[cfg(not(test))]
fn spawn_cmd(agent_string: &str, repo_str: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", "/D", repo_str, "cmd", "/K", agent_string]);
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, CREATE_NO_WINDOW);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("cmd starten fehlgeschlagen: {e}"))
}

/// Öffnet ein Terminal und startet einen Agenten darin
pub fn launch_agent(
    agent: &AgentProfile,
    repo_path: &Path,
    terminal_pref: &TerminalPreference,
) -> anyhow::Result<()> {
    let pref = agent.terminal_override.as_ref().unwrap_or(terminal_pref);
    let ps_script = build_powershell_script(agent, repo_path);
    let cmd_agent_string = build_cmd_agent_string(agent, repo_path);
    let agent_cmd_raw = build_agent_cmd_raw(agent, repo_path);
    let repo_str = repo_path.display().to_string();

    #[cfg(test)]
    {
        let _ = (
            &ps_script,
            &cmd_agent_string,
            &agent_cmd_raw,
            &repo_str,
            &pref,
        );
        Ok(())
    }

    #[cfg(not(test))]
    {
        let has_wt = is_program_available("wt");

        match pref {
            TerminalPreference::WindowsTerminal if has_wt => {
                spawn_wt(&repo_str, &cmd_agent_string, repo_path)
            }
            TerminalPreference::Powershell => spawn_powershell(&ps_script, repo_path),
            TerminalPreference::Cmd => spawn_cmd(&cmd_agent_string, &repo_str, repo_path),
            TerminalPreference::Auto if !has_wt => spawn_powershell(&ps_script, repo_path)
                .or_else(|_| spawn_cmd(&cmd_agent_string, &repo_str, repo_path)),
            TerminalPreference::Auto => spawn_wt(&repo_str, &cmd_agent_string, repo_path)
                .or_else(|_| spawn_powershell(&ps_script, repo_path))
                .or_else(|_| spawn_cmd(&cmd_agent_string, &repo_str, repo_path)),
            TerminalPreference::Custom(custom) => {
                let mut cmd = std::process::Command::new(custom);
                if !agent.args.is_empty() {
                    cmd.arg(&agent.program);
                    for a in &agent.args {
                        let substituted = substitute_placeholders(a, repo_path, repo_path);
                        cmd.arg(substituted);
                    }
                } else {
                    cmd.arg(&agent_cmd_raw);
                }
                cmd.current_dir(repo_path);
                set_creation_flags(&mut cmd, process::CREATE_NEW_CONSOLE);
                cmd.spawn_locked().map(|_| ()).map_err(|e| {
                    anyhow::anyhow!("Custom Terminal '{}' starten fehlgeschlagen: {e}", custom)
                })
            }
            _ => spawn_cmd(&cmd_agent_string, &repo_str, repo_path),
        }
    }
}

/// Öffnet nur die Shell im Repo-Verzeichnis (ohne Agent-Command)
pub fn open_shell(repo_path: &Path, terminal_pref: &TerminalPreference) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        let _ = (repo_path, terminal_pref);
        Ok(())
    }
    #[cfg(not(test))]
    {
        let has_wt = is_program_available("wt");
        let repo_str = repo_path.display().to_string();
        match terminal_pref {
            TerminalPreference::WindowsTerminal if has_wt => spawn_wt_shell(repo_path),
            TerminalPreference::Powershell => spawn_powershell_shell(repo_path),
            TerminalPreference::Cmd => spawn_cmd_shell(&repo_str, repo_path),
            TerminalPreference::Auto if !has_wt => {
                spawn_powershell_shell(repo_path).or_else(|_| spawn_cmd_shell(&repo_str, repo_path))
            }
            TerminalPreference::Auto => spawn_wt_shell(repo_path)
                .or_else(|_| spawn_powershell_shell(repo_path))
                .or_else(|_| spawn_cmd_shell(&repo_str, repo_path)),
            TerminalPreference::Custom(custom) => {
                let mut cmd = std::process::Command::new(custom);
                cmd.current_dir(repo_path);
                set_creation_flags(&mut cmd, process::CREATE_NEW_CONSOLE);
                cmd.spawn_locked().map(|_| ()).map_err(|e| {
                    anyhow::anyhow!("Custom Terminal '{}' starten fehlgeschlagen: {e}", custom)
                })
            }
            _ => spawn_cmd_shell(&repo_str, repo_path),
        }
    }
}
