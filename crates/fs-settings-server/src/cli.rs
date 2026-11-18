//! The command `fs-settings`: dispatch of the words. Exit: 0 done (also `same`), 1 failed (a code line), 2 wrong words.
//!
//! Pattern: Command + Facade. Changes always go to the SERVICE (never to the file: one process owns the database); only `repair`
//! touches the file, and only when the person runs it.

use crate::cli_db::{do_export, do_import};
use crate::cli_files::{do_repair, socket_for};
use crate::clock::system_clock;
use crate::connect::{admin, ensure};
use crate::instance_cmd::run_instance;
use crate::reply::Reply;
use crate::serve::{ServeError, serve};
use crate::words::{Args, Usage, instance_cmd};

const HELP: &str = "fs-settings --version | --help\n\
fs-settings serve [--instance] [--data DIR | --settings FILE] [--socket PATH]\n\
fs-settings instance show | set KEY VALUE | clear KEY | lock KEY VALUE --reason TEXT | unlock KEY --reason TEXT | log [--after ID] [--size N]\n\
    (the actor is the OS user of the caller, never a word; lock and unlock need --reason; --socket PATH names the instance service)\n\
fs-settings export --format toml|json --out FILE [--program P] [--device] [--instance]\n\
fs-settings import FILE [--dry-run] [--legacy] [--device] [--instance]\n\
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

fn do_instance(words: &[String]) -> Result<Reply, Usage> {
    let args = Args::parse(words.get(1..).unwrap_or_default())?;
    let socket_word = args.get("--socket").map(str::to_owned);
    let rest: Vec<String> = {
        let mut it = words.iter().skip(1);
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
    // The actor is the OS user of the caller, asked from the system: never a word of the command line.
    let actor = match crate::actor::os_actor() {
        Ok(a) => a,
        Err(c) => return Ok(Reply::failed(&c)),
    };
    let socket =
        match socket_for(socket_word.as_deref(), true).and_then(|s| ensure(&s, true).map(|()| s)) {
            Ok(s) => s,
            Err(c) => return Ok(Reply::failed(&c)),
        };
    Ok(run_instance(
        &cmd,
        &admin(&socket, &actor),
        &actor,
        system_clock()(),
    ))
}

/// Runs `args` (the words after the program name); returns the exit code.
#[must_use]
pub fn run(args: &[String]) -> i32 {
    let Some(first) = args.first() else {
        return emit(&no_words());
    };
    let rest = &args[1..];
    let parsed = Args::parse(rest);
    let reply = match (first.as_str(), parsed) {
        ("--version" | "-V", _) if args.len() == 1 => {
            Reply::done(vec![format!("fs-settings {}", env!("CARGO_PKG_VERSION"))])
        }
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
