//! The file-side words: `export`, `import`, `repair` (pure dispatch over the connect and exchange modules).

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::clock::system_clock;
use crate::connect::{admin, ensure, manage};
use crate::exchange::{ExportScope, Format, ImportEnv, ImportMode, export, import};
use crate::legacy::{LegacyEnv, import_legacy};
use crate::paths::{DbPlace, resolve};
use crate::ports::{BasicValue, Coded, ProgramName, Reference, SettingCode, params};
use crate::repair::repair;
use crate::reply::{Reply, code_line};
use crate::serve::{legacy_lines, socket_of};
use crate::words::{Args, ServeWords, Usage, serve_words};

/// The socket of the commands: `--socket` (taken out of the words) or the convention.
pub(crate) fn socket_for(
    socket: Option<&str>,
    instance: bool,
) -> Result<PathBuf, Coded<SettingCode>> {
    let words = ServeWords {
        place: DbPlace::Convention { instance },
        instance,
        socket: socket.map(PathBuf::from),
    };
    socket_of(&words)
}

fn format_of(args: &Args, file: Option<&str>) -> Result<Format, Usage> {
    let word = args.get("--format").map(str::to_owned).or_else(|| {
        file.and_then(|f| Path::new(f).extension())
            .and_then(|e| e.to_str())
            .map(str::to_owned)
    });
    word.as_deref().and_then(Format::parse).ok_or(Usage {
        word: "--format".to_owned(),
    })
}

pub(crate) fn do_export(args: &Args) -> Result<Reply, Usage> {
    args.only(&[
        "--format",
        "--out",
        "--program",
        "--device",
        "--instance",
        "--socket",
    ])?;
    let out = args.need("--out")?;
    let format = format_of(args, Some(out))?;
    let instance = args.flag("--instance");
    let programs: Vec<ProgramName> = args
        .all("--program")
        .into_iter()
        .filter_map(|p| ProgramName::new(p).ok())
        .collect();
    let socket = match socket_for(args.get("--socket"), instance)
        .and_then(|s| ensure(&s, instance).map(|()| s))
    {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    let what = if instance {
        ExportScope::Instance
    } else {
        ExportScope::Person(programs)
    };
    let mut buffer = Vec::new();
    let result = if instance {
        // Instance reads need no actor; the admin client is only the carrier of the read side.
        let by = Reference::parse("urn:fs:export").map_err(|_| Usage {
            word: "--by".to_owned(),
        })?;
        export(
            &admin(&socket, &by),
            &what,
            args.flag("--device"),
            format,
            Path::new(out),
            &mut buffer,
        )
    } else {
        export(
            &manage(&socket),
            &what,
            args.flag("--device"),
            format,
            Path::new(out),
            &mut buffer,
        )
    };
    Ok(
        match result.and_then(|()| write_new(Path::new(out), &buffer)) {
            Ok(()) => Reply::done(vec![format!("exported file={out} bytes={}", buffer.len())]),
            Err(c) => Reply::failed(&c),
        },
    )
}

/// Writes the export file; an existing file is never overwritten (nothing is deleted or replaced silently).
fn write_new(file: &Path, bytes: &[u8]) -> Result<(), Coded<SettingCode>> {
    use std::io::Write;
    let failed = |kind: &str| {
        Coded::new(SettingCode::ExportFailed)
            .with_text(params::FILE, file.to_string_lossy())
            .with_text(params::KIND, kind)
    };
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .map_err(|_| failed("exists-or-unwritable"))?;
    f.write_all(bytes).map_err(|_| failed("write"))
}

pub(crate) fn do_import(args: &Args) -> Result<Reply, Usage> {
    args.only(&[
        "--dry-run",
        "--legacy",
        "--device",
        "--instance",
        "--by",
        "--format",
        "--socket",
    ])?;
    let instance = args.flag("--instance");
    let socket = match socket_for(args.get("--socket"), instance)
        .and_then(|s| ensure(&s, instance).map(|()| s))
    {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    let store = manage(&socket);
    if args.flag("--legacy") {
        return Ok(Reply::done(legacy_lines(&import_legacy(
            &store,
            &LegacyEnv::from_process(),
        ))));
    }
    let [file] = args.positional.as_slice() else {
        return Err(Usage {
            word: "FILE".to_owned(),
        });
    };
    let format = format_of(args, Some(file))?;
    let by = args
        .get("--by")
        .map(|b| {
            Reference::parse(b).map_err(|_| Usage {
                word: "--by".to_owned(),
            })
        })
        .transpose()?;
    if instance && by.is_none() {
        return Err(Usage {
            word: "--by".to_owned(),
        });
    }
    let mode = if args.flag("--dry-run") {
        ImportMode::DryRun
    } else {
        ImportMode::Apply
    };
    let env = ImportEnv {
        catalog: None,
        by: by.as_ref(),
        now: system_clock()(),
        device: args.flag("--device"),
    };
    let Ok(mut src) = File::open(file) else {
        let c = Coded::new(SettingCode::FileUnreadable).with_text(params::FILE, file.as_str());
        return Ok(Reply::failed(&c));
    };
    let admin_client = by.as_ref().filter(|_| instance).map(|b| admin(&socket, b));
    let result = import(
        &store,
        admin_client
            .as_ref()
            .map(|a| a as &dyn crate::ports::InstanceStore),
        &mut src,
        Path::new(file),
        format,
        mode,
        &env,
    );
    Ok(match result {
        Ok(r) => {
            let mut lines = vec![format!(
                "{} applied={} same={} refused={} unchecked={} skipped-device={}",
                if mode == ImportMode::DryRun {
                    "dry-run"
                } else {
                    "imported"
                },
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

pub(crate) fn do_repair(args: &Args) -> Result<Reply, Usage> {
    let words = serve_words(args)?;
    if words.instance || words.socket.is_some() {
        return Err(Usage {
            word: "--instance".to_owned(),
        });
    }
    let file = match resolve(&words.place) {
        Ok(f) => f,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    Ok(match repair(&file, system_clock()()) {
        Ok(r) => {
            let mut lines = vec![format!("repaired moved-to={}", r.moved_to.display())];
            lines.extend(
                r.offer
                    .map(|o| format!("offer import-file={}", o.display())),
            );
            Reply::done(lines)
        }
        Err(c) => Reply::failed(&c),
    })
}
