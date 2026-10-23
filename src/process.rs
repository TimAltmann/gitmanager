//! Externe Prozesse mit Obergrenzen (System-`git`, glab): Timeout mit
//! `kill` und Ernten, stdout/stderr in eigenen Threads (eine volle Pipe
//! blockiert sonst das Kind), stdin aus einem eigenen Thread und kurzer
//! Nachlauf nach dem Exit, damit ein Enkelprozess mit geerbter Pipe (ssh)
//! den Aufruf nicht festhält. Ohne Konsolenfenster startet der Aufrufer
//! (`CREATE_NO_WINDOW` beim Bauen des `Command`).

use std::fmt;
use std::io::{Read, Write};
use std::process::{Child, Command, ExitStatus};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Windows: Konsolenprogramm ohne eigenes Konsolenfenster starten.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Abfrageintervall für `try_wait`.
const POLL_INTERVAL: Duration = Duration::from_millis(10);
/// Nachlauf für die Pipe-Leser nach dem Exit (siehe [`Keep`]).
const PIPE_GRACE: Duration = Duration::from_secs(1);

/// Was ein Pipe-Leser behält und wie lange [`finish`] nach dem Exit auf ihn
/// wartet.
#[derive(Debug, Clone, Copy)]
pub enum Keep {
    /// Nutzdaten: Anfang bis zur Grenze, mehr Daten → `OutputTooLarge`.
    /// Nach dem Exit bis zum Timeout (mindestens `PIPE_GRACE`), sonst
    /// `Timeout`.
    Head(usize),
    /// Meldungen: nur das Ende, alles wird gelesen. Nach dem Exit höchstens
    /// `PIPE_GRACE`, sonst leer.
    Tail(usize),
}

/// Grenzen eines Aufrufs.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    pub stdout: Keep,
    pub stderr: Keep,
}

/// Ausgabe eines beendeten Prozesses.
#[derive(Debug)]
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Fehler eines Aufrufs; ein gestarteter Prozess ist dann beendet und
/// geerntet.
#[derive(Debug)]
pub enum RunError {
    /// `spawn` gescheitert (nur [`output`])
    Spawn(std::io::Error),
    /// Timeout überschritten (`kill`) bzw. `Keep::Head` danach unvollständig
    Timeout(Duration),
    /// `try_wait` gescheitert
    Wait(std::io::Error),
    /// Mehr Daten als `Keep::Head` erlaubt
    OutputTooLarge,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "spawn failed: {e}"),
            Self::Timeout(timeout) => write!(f, "timed out after {timeout:?}"),
            Self::Wait(e) => write!(f, "wait failed: {e}"),
            Self::OutputTooLarge => f.write_str("output too large"),
        }
    }
}

/// Startet `cmd` (ohne stdin) und wartet wie [`finish`].
pub fn output(cmd: &mut Command, limits: Limits) -> Result<Output, RunError> {
    let child = cmd.spawn().map_err(RunError::Spawn)?;
    finish(child, None::<&[u8]>, limits)
}

/// Liest stdout und stderr parallel, schreibt `stdin` in einem eigenen
/// Thread und wartet höchstens `limits.timeout`; danach `kill()` und
/// Ernten. Nach dem Exit wartet es auf die Pipe-Leser wie bei [`Keep`]
/// beschrieben.
pub fn finish<B>(mut child: Child, stdin: Option<B>, limits: Limits) -> Result<Output, RunError>
where
    B: AsRef<[u8]> + Send + 'static,
{
    let deadline = Instant::now() + limits.timeout;
    let stdout = read_pipe(child.stdout.take(), limits.stdout);
    let stderr = read_pipe(child.stderr.take(), limits.stderr);
    if let (Some(mut pipe), Some(body)) = (child.stdin.take(), stdin) {
        std::thread::spawn(move || {
            // Endet das Kind vor dem Lesen, scheitert das Schreiben
            // (BrokenPipe); dann zählen Exit-Code und stderr. Drop schließt
            // stdin (EOF).
            let _ = pipe.write_all(body.as_ref());
        });
    }
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL_INTERVAL),
            Ok(None) => {
                kill_and_reap(&mut child);
                return Err(RunError::Timeout(limits.timeout));
            }
            Err(e) => {
                kill_and_reap(&mut child);
                return Err(RunError::Wait(e));
            }
        }
    };
    // Ein Enkelprozess mit geerbter Pipe darf den Aufruf nicht über den
    // Nachlauf bzw. (Nutzdaten) den Timeout hinaus festhalten.
    let grace_end = Instant::now() + PIPE_GRACE;
    let collect = |rx: mpsc::Receiver<PipeData>, keep: Keep| {
        let end = match keep {
            Keep::Head(_) => grace_end.max(deadline),
            Keep::Tail(_) => grace_end,
        };
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok(data) if data.overflow => Err(RunError::OutputTooLarge),
            Ok(data) => Ok(data.bytes),
            Err(_) => match keep {
                Keep::Head(_) => Err(RunError::Timeout(limits.timeout)),
                Keep::Tail(_) => Ok(Vec::new()),
            },
        }
    };
    // stderr (Meldungen) zuerst: seine kurze Frist liefe sonst ab, während
    // auf die Nutzdaten gewartet wird.
    let stderr = collect(stderr, limits.stderr)?;
    let stdout = collect(stdout, limits.stdout)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Default)]
struct PipeData {
    bytes: Vec<u8>,
    overflow: bool,
}

/// Liest eine Pipe in einem eigenen Thread; das Ergebnis kommt über den Kanal.
fn read_pipe<R: Read + Send + 'static>(pipe: Option<R>, keep: Keep) -> mpsc::Receiver<PipeData> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut data = PipeData::default();
        if let Some(mut pipe) = pipe {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let n = match pipe.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                };
                match keep {
                    Keep::Head(limit) => {
                        let room = limit - data.bytes.len();
                        data.bytes.extend_from_slice(&buf[..n.min(room)]);
                        if n > room {
                            // Pipe schließen: das Kind bekommt EPIPE und endet.
                            data.overflow = true;
                            break;
                        }
                    }
                    Keep::Tail(limit) => {
                        data.bytes.extend_from_slice(&buf[..n]);
                        if data.bytes.len() > 2 * limit {
                            data.bytes.drain(..data.bytes.len() - limit);
                        }
                    }
                }
            }
            if let Keep::Tail(limit) = keep {
                let excess = data.bytes.len().saturating_sub(limit);
                data.bytes.drain(..excess);
            }
        }
        let _ = tx.send(data);
    });
    rx
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn limits(timeout: Duration, stdout: Keep) -> Limits {
        Limits {
            timeout,
            stdout,
            stderr: Keep::Tail(8),
        }
    }

    /// Ein hängender Prozess endet nach dem Timeout (`kill`) mit Fehler,
    /// statt den Aufrufer (Worker samt Ref-Lock) für immer zu halten; ein
    /// Enkelprozess mit geerbter Pipe hält den Aufruf nach dem Exit nicht
    /// fest.
    #[test]
    fn output_is_bounded() {
        let start = Instant::now();
        let err = output(
            &mut sh("sleep 5"),
            limits(Duration::from_millis(200), Keep::Tail(1024)),
        )
        .expect_err("Timeout");
        assert!(matches!(err, RunError::Timeout(_)), "{err:?}");
        assert!(err.to_string().starts_with("timed out after"), "{err}");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );

        let start = Instant::now();
        let out = output(
            &mut sh("echo fehler >&2; sleep 5 & exit 3"),
            limits(Duration::from_secs(20), Keep::Tail(1024)),
        )
        .expect("beendet");
        assert_eq!(out.status.code(), Some(3));
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );

        let out = output(
            &mut sh("echo ok; echo warn >&2"),
            limits(Duration::from_secs(20), Keep::Tail(1024)),
        )
        .expect("beendet");
        assert!(out.status.success());
        assert_eq!(out.stdout, b"ok\n");
        assert_eq!(out.stderr, b"warn\n");
    }

    /// Nutzdaten (`Head`) kommen ganz oder als Fehler; Meldungen (`Tail`)
    /// behalten nur das Ende.
    #[test]
    fn head_complete_or_error_tail_keeps_end() {
        let run = |script: &str, stdout: Keep| {
            output(&mut sh(script), limits(Duration::from_secs(20), stdout))
        };
        let out = run("printf 1234567890; printf abcdefghij >&2", Keep::Head(10)).unwrap();
        assert_eq!(out.stdout, b"1234567890");
        assert_eq!(out.stderr, b"cdefghij");
        let err = run("printf 12345678901", Keep::Head(10)).unwrap_err();
        assert!(matches!(err, RunError::OutputTooLarge), "{err:?}");
        let out = run("printf 1234567890", Keep::Tail(4)).unwrap();
        assert_eq!(out.stdout, b"7890");

        // Enkelprozess hält stdout offen: Nutzdaten unvollständig → Timeout
        let start = Instant::now();
        let err = output(
            &mut sh("printf teil; sleep 5 & exit 0"),
            limits(Duration::from_millis(1500), Keep::Head(1024)),
        )
        .unwrap_err();
        assert!(matches!(err, RunError::Timeout(_)), "{err:?}");
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn stdin_until_eof() {
        let child = sh("cat").spawn().unwrap();
        let body = "x".repeat(200_000);
        let out = finish(
            child,
            Some(body.clone()),
            limits(Duration::from_secs(20), Keep::Head(1 << 20)),
        )
        .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, body.as_bytes());
    }
}
