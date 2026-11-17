//! The command `fs-settings`: dispatch of the words. Exit: 0 done (also `same`), 1 failed (a code line), 2 wrong words.
//!
//! Pattern: Command + Facade. Changes always go to the SERVICE (never to the file: one process owns the database); only `repair`
//! touches the file, and only when the person runs it.

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::clock::system_clock;
use crate::connect::{admin, ensure, manage};
use crate::exchange::{ExportScope, Format, ImportEnv, ImportMode, export, import};
use crate::instance_cmd::run_instance;
use crate::legacy::{LegacyEnv, import_legacy};
use crate::paths::{DbPlace, resolve};
use crate::ports::{BasicValue, Coded, ProgramName, Reference, SettingCode, params};
use crate::repair::repair;
use crate::reply::{Reply, code_line};
use crate::serve::{ServeError, legacy_lines, serve, socket_of};
use crate::words::{Args, ServeWords, Usage, instance_cmd, serve_words};

const HELP: &str = "fs-settings --version | --help\n\
fs-settings serve [--instance] [--data DIR | --settings FILE] [--socket PATH]\n\
fs-settings instance show | set KEY VALUE | clear KEY | lock KEY VALUE --reason TEXT | unlock KEY --reason TEXT | log [--after ID] [--size N]\n\
    (every change needs --by IRI; lock and unlock need --reason; --socket PATH names the instance service)\n\
fs-settings export --format toml|json --out FILE [--program P] [--device] [--instance]\n\
fs-settings import FILE [--dry-run] [--legacy] [--device] [--instance --by IRI]\n\
fs-settings repair [--data DIR | --settings FILE]\n\
Without words: makes sure the settings service runs.\n\
Exit codes: 0 done (also `same`), 1 failed (setting-error-<code> key=value ...), 2 wrong words";

fn emit(reply: &Reply) -> i32 {
    for line in &reply.out {
        println!("{line}");
    }
    for line in &reply.err {
        eprintln!("{line}");
    }
    reply.exit
}

fn usage(u: &Usage) -> Reply {
    Reply::usage(&u.word)
}

/// The socket of the commands: `--socket` (taken out of the words) or the convention.
fn socket_for(socket: Option<&str>, instance: bool) -> Result<PathBuf, Coded<SettingCode>> {
    let words = ServeWords { place: DbPlace::Convention { instance }, instance, socket: socket.map(PathBuf::from) };
    socket_of(&words)
}

fn format_of(args: &Args, file: Option<&str>) -> Result<Format, Usage> {
    let word = args.get("--format").map(str::to_owned).or_else(|| {
        file.and_then(|f| Path::new(f).extension()).and_then(|e| e.to_str()).map(str::to_owned)
    });
    word.as_deref().and_then(Format::parse).ok_or(Usage { word: "--format".to_owned() })
}

fn do_export(args: &Args) -> Result<Reply, Usage> {
    args.only(&["--format", "--out", "--program", "--device", "--instance", "--socket"])?;
    let out = args.need("--out")?;
    let format = format_of(args, Some(out))?;
    let instance = args.flag("--instance");
    let programs: Vec<ProgramName> = args.all("--program").into_iter().filter_map(|p| ProgramName::new(p).ok()).collect();
    let socket = match socket_for(args.get("--socket"), instance).and_then(|s| ensure(&s, instance).map(|()| s)) {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    let what = if instance { ExportScope::Instance } else { ExportScope::Person(programs) };
    let mut buffer = Vec::new();
    let result = if instance {
        // Instance reads need no actor; the admin client is only the carrier of the read side.
        let by = Reference::parse("urn:fs:export").map_err(|_| Usage { word: "--by".to_owned() })?;
        export(&admin(&socket, &by), &what, args.flag("--device"), format, Path::new(out), &mut buffer)
    } else {
        export(&manage(&socket), &what, args.flag("--device"), format, Path::new(out), &mut buffer)
    };
    Ok(match result.and_then(|()| write_new(Path::new(out), &buffer)) {
        Ok(()) => Reply::done(vec![format!("exported file={out} bytes={}", buffer.len())]),
        Err(c) => Reply::failed(&c),
    })
}

/// Writes the export file; an existing file is never overwritten (nothing is deleted or replaced silently).
fn write_new(file: &Path, bytes: &[u8]) -> Result<(), Coded<SettingCode>> {
    use std::io::Write;
    let failed = |kind: &str| Coded::new(SettingCode::ExportFailed).with_text(params::FILE, file.to_string_lossy()).with_text(params::KIND, kind);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(file).map_err(|_| failed("exists-or-unwritable"))?;
    f.write_all(bytes).map_err(|_| failed("write"))
}

fn do_import(args: &Args) -> Result<Reply, Usage> {
    args.only(&["--dry-run", "--legacy", "--device", "--instance", "--by", "--format", "--socket"])?;
    let instance = args.flag("--instance");
    let socket = match socket_for(args.get("--socket"), instance).and_then(|s| ensure(&s, instance).map(|()| s)) {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    let store = manage(&socket);
    if args.flag("--legacy") {
        return Ok(Reply::done(legacy_lines(&import_legacy(&store, &LegacyEnv::from_process()))));
    }
    let [file] = args.positional.as_slice() else { return Err(Usage { word: "FILE".to_owned() }) };
    let format = format_of(args, Some(file))?;
    let by = args.get("--by").map(|b| Reference::parse(b).map_err(|_| Usage { word: "--by".to_owned() })).transpose()?;
    if instance && by.is_none() {
        return Err(Usage { word: "--by".to_owned() });
    }
    let mode = if args.flag("--dry-run") { ImportMode::DryRun } else { ImportMode::Apply };
    let env = ImportEnv { catalog: None, by: by.as_ref(), now: system_clock()(), device: args.flag("--device") };
    let mut src = match File::open(file) {
        Ok(f) => f,
        Err(_) => {
            let c = Coded::new(SettingCode::FileUnreadable).with_text(params::FILE, file.as_str());
            return Ok(Reply::failed(&c));
        }
    };
    let admin_client = by.as_ref().filter(|_| instance).map(|b| admin(&socket, b));
    let result = import(&store, admin_client.as_ref().map(|a| a as &dyn crate::ports::InstanceStore), &mut src, Path::new(file), format, mode, &env);
    Ok(match result {
        Ok(r) => {
            let mut lines = vec![format!(
                "{} applied={} same={} refused={} unchecked={} skipped-device={}",
                if mode == ImportMode::DryRun { "dry-run" } else { "imported" },
                r.applied,
                r.same,
                r.refused.len(),
                r.unchecked,
                r.skipped_device
            )];
            lines.extend(r.refused.iter().map(code_line));
            Reply::done(lines)
        }
        Err(c) => Reply::failed(&c),
    })
}

fn do_repair(args: &Args) -> Result<Reply, Usage> {
    let words = serve_words(args)?;
    if words.instance || words.socket.is_some() {
        return Err(Usage { word: "--instance".to_owned() });
    }
    let file = match resolve(&words.place) {
        Ok(f) => f,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    Ok(match repair(&file, system_clock()()) {
        Ok(r) => {
            let mut lines = vec![format!("repaired moved-to={}", r.moved_to.display())];
            lines.extend(r.offer.map(|o| format!("offer import-file={}", o.display())));
            Reply::done(lines)
        }
        Err(c) => Reply::failed(&c),
    })
}

fn do_instance(words: &[String]) -> Result<Reply, Usage> {
    let args = Args::parse(words.get(1..).unwrap_or_default())?;
    let socket_word = args.get("--socket").map(str::to_owned);
    let rest: Vec<String> = {
        let mut it = words.iter().peekable();
        let mut kept = Vec::new();
        while let Some(w) = it.next() {
            if w == "--socket" {
                it.next();
            } else {
                kept.push(w.clone());
            }
        }
        kept
    };
    let cmd = instance_cmd(&rest)?;
    let by = match &cmd {
        crate::words::InstanceCmd::Set { by, .. }
        | crate::words::InstanceCmd::Clear { by, .. }
        | crate::words::InstanceCmd::Lock { by, .. }
        | crate::words::InstanceCmd::Unlock { by, .. } => by.clone(),
        _ => "urn:fs:reader".to_owned(),
    };
    let by = Reference::parse(&by).map_err(|_| Usage { word: "--by".to_owned() })?;
    let socket = match socket_for(socket_word.as_deref(), true).and_then(|s| ensure(&s, true).map(|()| s)) {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    Ok(run_instance(&cmd, &admin(&socket, &by), system_clock()()))
}

/// Runs `args` (the words after the program name); returns the exit code.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    let Some(first) = args.first() else { return emit(&no_words()) };
    let rest = &args[1..];
    let parsed = Args::parse(rest);
    let reply = match (first.as_str(), parsed) {
        ("--version" | "-V", _) if args.len() == 1 => Reply::done(vec![format!("fs-settings {}", env!("CARGO_PKG_VERSION"))]),
        ("--help" | "-h", _) if args.len() == 1 => Reply::done(vec![HELP.to_owned()]),
        ("serve", _) => {
            return match serve(rest) {
                Ok(()) => 0,
                Err(ServeError::Usage(m)) => {
                    eprintln!("{m}");
                    2
                }
                Err(ServeError::Failed(m)) => {
                    eprintln!("{m}");
                    1
                }
            };
        }
        ("instance", _) => do_instance(args).unwrap_or_else(|u| usage(&u)),
        ("export", Ok(a)) => do_export(&a).unwrap_or_else(|u| usage(&u)),
        ("import", Ok(a)) => do_import(&a).unwrap_or_else(|u| usage(&u)),
        ("repair", Ok(a)) => do_repair(&a).unwrap_or_else(|u| usage(&u)),
        (_, Err(u)) => usage(&u),
        (other, _) => Reply::usage(other),
    };
    emit(&reply)
}

/// Without words: make sure the person's service runs (the program starts through fs-starter, which asks for exactly this).
fn no_words() -> Reply {
    match socket_for(None, false).and_then(|s| ensure(&s, false).map(|()| s)) {
        Ok(s) => Reply::done(vec![format!("running socket={}", s.display())]),
        Err(c) => Reply::failed(&c),
    }
}

