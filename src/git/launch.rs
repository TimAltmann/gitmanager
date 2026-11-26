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

/// Setzt `s` in `quote`, wenn `force` gilt oder eines der `meta`-Zeichen
/// vorkommt; ein inneres `quote` wird verdoppelt.
fn shell_escape(s: &str, quote: char, meta: &[char], force: bool) -> String {
    if force || s.contains(meta) {
        let doubled = format!("{quote}{quote}");
        format!("{quote}{}{quote}", s.replace(quote, &doubled))
    } else {
        s.to_string()
    }
}

pub(super) fn escape_for_powershell(s: &str) -> String {
    const META: &[char] = &[' ', '\'', '"', '&', '|', ';', '>', '<', '^', '`', '$'];
    shell_escape(s, '\'', META, false)
}

#[cfg(not(test))]
fn escape_for_powershell_force(s: &str) -> String {
    shell_escape(s, '\'', &[], true)
}

/// Für `cmd /K`
fn escape_for_cmd(s: &str) -> String {
    shell_escape(s, '"', &[' ', '&', '|', '<', '>', '^', '"'], false)
}

/// Agent ohne Args: die fertige `command`-Zeile (Platzhalter ersetzt), sonst
/// das vom Aufrufer aufbereitete Programm.
fn agent_without_args(agent: &AgentProfile, repo_path: &Path, program: String) -> String {
    match &agent.command {
        Some(cmd) => substitute_placeholders(cmd, repo_path, repo_path),
        None => program,
    }
}

fn build_agent_command(
    agent: &AgentProfile,
    repo_path: &Path,
    escape_fn: fn(&str) -> String,
    prefix: &str,
) -> String {
    let program = format!("{prefix}{}", escape_fn(&agent.program));
    if agent.args.is_empty() {
        return agent_without_args(agent, repo_path, program);
    }
    let args_str = agent
        .args
        .iter()
        .map(|a| escape_fn(&substitute_placeholders(a, repo_path, repo_path)))
        .collect::<Vec<_>>()
        .join(" ");
    if args_str.is_empty() {
        program
    } else {
        format!("{program} {args_str}")
    }
}

fn build_agent_cmd_raw(agent: &AgentProfile, repo_path: &Path) -> String {
    // Nur für `TerminalPreference::Custom` (`cmd.arg(&agent_cmd_raw)` in
    // `launch_agent`). `Command::arg` übergibt ohne Shell-Parsing – zusätzliche
    // Quotes würden literal Teil des Arguments (z. B. `"C:\Program Files\..."`).
    // Daher program NICHT quoten.
    if agent.args.is_empty() {
        return agent_without_args(agent, repo_path, agent.program.clone());
    }
    let args_str = agent
        .args
        .iter()
        .map(|a| quote_if_needed(&substitute_placeholders(a, repo_path, repo_path)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", agent.program, args_str)
}

pub(super) fn build_powershell_script(agent: &AgentProfile, repo_path: &Path) -> String {
    let prog = &agent.program;
    let needs_amp =
        prog.contains(' ') || prog.contains('\\') || prog.contains('/') || prog.contains('\'');
    let prefix = if needs_amp { "& " } else { "" };
    build_agent_command(agent, repo_path, escape_for_powershell, prefix)
}

fn build_cmd_agent_string(agent: &AgentProfile, repo_path: &Path) -> String {
    build_agent_command(agent, repo_path, escape_for_cmd, "")
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
    if ide.use_shell && !ide.allow_unsafe {
        reject_shell_chars(
            &args,
            &effective_program,
            "(shell-Zeichen) – aktiviere allow_unsafe wenn beabsichtigt",
        )?;
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
                    reject_shell_chars(
                        &args,
                        &effective_program,
                        "für Shell-Fallback (allow_unsafe erforderlich)",
                    )?;
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

/// Shell-Zeichen in Args und Programm ablehnen, bevor eine Shell sie
/// auswertet; `hint` ergänzt die Meldung um den Weg (`use_shell` bzw.
/// `cmd /C`-Fallback).
fn reject_shell_chars(args: &[String], program: &str, hint: &str) -> anyhow::Result<()> {
    const SHELL_CHARS: &[char] = &['&', '|', ';', '>', '<'];
    if let Some(arg) = args.iter().find(|a| a.contains(SHELL_CHARS)) {
        anyhow::bail!("Ungültige Zeichen in IDE-Args {hint}: {arg}");
    }
    if program.contains(SHELL_CHARS) {
        anyhow::bail!("Ungültige Zeichen in IDE-Programm {hint}: {program}");
    }
    Ok(())
}

/// Gemeinsamer Abschluss der Terminal-Starts: Arbeitsordner, Fenster-Flags,
/// Start und Fehlertext („{what} starten fehlgeschlagen“).
#[cfg(not(test))]
fn spawn_in(
    mut cmd: std::process::Command,
    repo_path: &Path,
    creation_flags: u32,
    what: &str,
) -> anyhow::Result<()> {
    cmd.current_dir(repo_path);
    set_creation_flags(&mut cmd, creation_flags);
    cmd.spawn_locked()
        .map(|_| ())
        .map_err(|e| anyhow::anyhow!("{what} starten fehlgeschlagen: {e}"))
}

/// Windows Terminal im Repo; mit `agent` läuft darin `cmd /k <agent>`.
#[cfg(not(test))]
fn spawn_wt(agent: Option<&str>, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("wt");
    cmd.arg("-d").arg(repo_path);
    if let Some(agent) = agent {
        cmd.args(["--", "cmd", "/k", agent]);
    }
    spawn_in(
        cmd,
        repo_path,
        process::CREATE_NEW_CONSOLE,
        "Windows Terminal",
    )
}

/// PowerShell im Repo (`pwsh` bevorzugt); mit `script` läuft es darin.
#[cfg(not(test))]
fn spawn_powershell(script: Option<&str>, repo_path: &Path) -> anyhow::Result<()> {
    let has_pwsh = is_program_available("pwsh");
    let mut cmd = std::process::Command::new(if has_pwsh { "pwsh" } else { "powershell" });
    cmd.args(["-NoLogo", "-NoExit"]);
    if has_pwsh {
        cmd.arg("-WorkingDirectory").arg(repo_path);
        if let Some(script) = script {
            cmd.args(["-Command", script]);
        }
    } else {
        // Windows PowerShell 5 kennt `-WorkingDirectory` nicht
        let mut full = format!(
            "Set-Location {}",
            escape_for_powershell_force(&repo_path.display().to_string())
        );
        if let Some(script) = script {
            full.push_str("; ");
            full.push_str(script);
        }
        cmd.args(["-Command", &full]);
    }
    spawn_in(cmd, repo_path, process::CREATE_NEW_CONSOLE, "Powershell")
}

/// Neues `cmd`-Fenster im Repo; `agent` (leer = nur die Shell) läuft darin.
#[cfg(not(test))]
fn spawn_cmd(agent: &str, repo_path: &Path) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", "start", "", "/D"]).arg(repo_path);
    cmd.args(["cmd", "/K", agent]);
    spawn_in(cmd, repo_path, CREATE_NO_WINDOW, "cmd")
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

    #[cfg(test)]
    {
        let _ = (&ps_script, &cmd_agent_string, &agent_cmd_raw, &pref);
        Ok(())
    }

    #[cfg(not(test))]
    {
        let wt = || spawn_wt(Some(&cmd_agent_string), repo_path);
        let powershell = || spawn_powershell(Some(&ps_script), repo_path);
        let cmd = || spawn_cmd(&cmd_agent_string, repo_path);
        let has_wt = is_program_available("wt");
        match pref {
            TerminalPreference::WindowsTerminal if has_wt => wt(),
            TerminalPreference::Powershell => powershell(),
            TerminalPreference::Auto if !has_wt => powershell().or_else(|_| cmd()),
            TerminalPreference::Auto => wt().or_else(|_| powershell()).or_else(|_| cmd()),
            TerminalPreference::Custom(custom) => {
                let mut command = std::process::Command::new(custom);
                if agent.args.is_empty() {
                    command.arg(&agent_cmd_raw);
                } else {
                    command.arg(&agent.program);
                    for a in &agent.args {
                        command.arg(substitute_placeholders(a, repo_path, repo_path));
                    }
                }
                spawn_custom(command, custom, repo_path)
            }
            // `Cmd` und Windows Terminal ohne `wt`
            _ => cmd(),
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
        let wt = || spawn_wt(None, repo_path);
        let powershell = || spawn_powershell(None, repo_path);
        let cmd = || spawn_cmd("", repo_path);
        let has_wt = is_program_available("wt");
        match terminal_pref {
            TerminalPreference::WindowsTerminal if has_wt => wt(),
            TerminalPreference::Powershell => powershell(),
            TerminalPreference::Auto if !has_wt => powershell().or_else(|_| cmd()),
            TerminalPreference::Auto => wt().or_else(|_| powershell()).or_else(|_| cmd()),
            TerminalPreference::Custom(custom) => {
                spawn_custom(std::process::Command::new(custom), custom, repo_path)
            }
            // `Cmd` und Windows Terminal ohne `wt`
            _ => cmd(),
        }
    }
}

/// Eigenes Terminalprogramm aus den Einstellungen.
#[cfg(not(test))]
fn spawn_custom(cmd: std::process::Command, custom: &str, repo_path: &Path) -> anyhow::Result<()> {
    spawn_in(
        cmd,
        repo_path,
        process::CREATE_NEW_CONSOLE,
        &format!("Custom Terminal '{custom}'"),
    )
}
