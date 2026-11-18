//! The words `export` and `import`: they work on the database FILE, so no service may hold it (one owner at a time): when a service
//! answers on the socket they refuse with `store-busy`. Import runs all its writes in one transaction. (A running service would need a
//! contract call for this: request to B0/B2, in the report.)
//!
//! Pattern: Command (one function per word); the owner check is a guard before the store is opened.

use std::fs::File;
use std::path::Path;

use crate::cli_files::{format_of, socket_for, write_new};
use crate::clock::system_clock;
use crate::connect::manage;
use crate::db_store::DbSettingsStore;
use crate::exchange::{ExportScope, ImportEnv, ImportMode, export, import};
use crate::instance_source::NoInstance;
use crate::legacy::{LegacyEnv, import_legacy};
use crate::paths::DbPlace;
use crate::ports::{Catalog, Coded, Health, ProgramName, SettingCode, SettingsStore, params};
use crate::reply::{Reply, code_line};
use crate::serve::legacy_lines;
use crate::words::{Args, Usage};

fn usage(word: &str) -> Usage {
    Usage {
        word: word.to_owned(),
    }
}

/// The database of the words `--data`, `--settings`, `--instance`, opened only when no service holds it.
fn open_direct(args: &Args) -> Result<DbSettingsStore, Coded<SettingCode>> {
    let instance = args.flag("--instance");
    let place = match (args.get("--data"), args.get("--settings")) {
        (_, Some(file)) => DbPlace::File(file.into()),
        (Some(dir), None) => DbPlace::Dir(dir.into()),
        (None, None) => DbPlace::Convention { instance },
    };
    let socket = socket_for(args.get("--socket"), instance)?;
    if manage(&socket).health() == Health::Ok {
        // A service answers: it owns the file. Never open it a second time.
        return Err(
            Coded::new(SettingCode::StoreBusy).with_text(params::SOCKET, socket.to_string_lossy())
        );
    }
    let store = DbSettingsStore::open(&place, instance, Box::new(NoInstance), system_clock())?;
    match store.health() {
        Health::Defaults(codes) if !codes.is_empty() => Err(codes[0].clone()),
        _ => Ok(store),
    }
}

pub(crate) fn do_export(args: &Args) -> Result<Reply, Usage> {
    args.only(&[
        "--format",
        "--out",
        "--program",
        "--device",
        "--instance",
        "--socket",
        "--data",
        "--settings",
    ])?;
    let out = args.need("--out")?;
    let format = format_of(args, Some(out))?;
    let instance = args.flag("--instance");
    let store = match open_direct(args) {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    let mut programs: Vec<ProgramName> = args
        .all("--program")
        .into_iter()
        .filter_map(|p| ProgramName::new(p).ok())
        .collect();
    if programs.is_empty() {
        programs = match store.known_programs() {
            Ok(p) => p,
            Err(c) => return Ok(Reply::failed(&c)),
        };
    }
    let what = if instance {
        ExportScope::All(programs)
    } else {
        ExportScope::Person(programs)
    };
    let mut buffer = Vec::new();
    let result = export(
        &store,
        &what,
        args.flag("--device"),
        format,
        Path::new(out),
        &mut buffer,
    );
    Ok(
        match result.and_then(|()| write_new(Path::new(out), &buffer)) {
            Ok(()) => Reply::done(vec![format!("exported file={out} bytes={}", buffer.len())]),
            Err(c) => Reply::failed(&c),
        },
    )
}

pub(crate) fn do_import(args: &Args) -> Result<Reply, Usage> {
    args.only(&[
        "--dry-run",
        "--legacy",
        "--device",
        "--instance",
        "--format",
        "--socket",
        "--data",
        "--settings",
    ])?;
    let instance = args.flag("--instance");
    let store = match open_direct(args) {
        Ok(s) => s,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    if args.flag("--legacy") {
        return Ok(Reply::done(legacy_lines(&import_legacy(
            &store,
            &LegacyEnv::from_process(),
        ))));
    }
    let [file] = args.positional.as_slice() else {
        return Err(usage("FILE"));
    };
    let format = format_of(args, Some(file))?;
    // The actor of instance writes is the OS user of the caller, from the system; person values need none.
    let actor = if instance {
        match crate::actor::os_actor() {
            Ok(a) => Some(a),
            Err(c) => return Ok(Reply::failed(&c)),
        }
    } else {
        None
    };
    let mode = if args.flag("--dry-run") {
        ImportMode::DryRun
    } else {
        ImportMode::Apply
    };
    // This program knows no declarations of other programs yet: the catalog is empty, so NOTHING undeclared is ever written; every
    // value is refused (`import-conflict{cause=not-declared}`) and the command ends with 1.
    let catalog = Catalog::new();
    let env = ImportEnv {
        catalog: &catalog,
        by: actor.as_ref(),
        device: args.flag("--device"),
    };
    let Ok(mut src) = File::open(file) else {
        let c = Coded::new(SettingCode::FileUnreadable).with_text(params::FILE, file.as_str());
        return Ok(Reply::failed(&c));
    };
    Ok(
        match import(&store, &mut src, Path::new(file), format, mode, &env) {
            Ok(r) => {
                let word = if mode == ImportMode::DryRun {
                    "dry-run"
                } else {
                    "imported"
                };
                let mut lines = vec![format!(
                    "{word} applied={} same={} refused={} skipped-device={}",
                    r.applied,
                    r.same,
                    r.refused.len(),
                    r.skipped_device
                )];
                let refused: Vec<String> = r.refused.iter().map(code_line).collect();
                Reply {
                    exit: i32::from(!refused.is_empty()),
                    out: std::mem::take(&mut lines),
                    err: refused,
                }
            }
            Err(c) => Reply::failed(&c),
        },
    )
}
