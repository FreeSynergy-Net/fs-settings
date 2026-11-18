//! The file-side words: `export`, `import`, `repair` (pure dispatch over the connect and exchange modules).
//! Pattern: Command (one function per file-side word).

use std::path::{Path, PathBuf};

use crate::clock::system_clock;
use crate::exchange::Format;
use crate::paths::{DbPlace, resolve};
use crate::ports::{Coded, SettingCode, params};
use crate::repair::repair;
use crate::reply::Reply;
use crate::serve::socket_of;
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

pub(crate) fn format_of(args: &Args, file: Option<&str>) -> Result<Format, Usage> {
    let word = args.get("--format").map(str::to_owned).or_else(|| {
        file.and_then(|f| Path::new(f).extension())
            .and_then(|e| e.to_str())
            .map(str::to_owned)
    });
    word.as_deref().and_then(Format::parse).ok_or(Usage {
        word: "--format".to_owned(),
    })
}

/// Writes the export file; an existing file is never overwritten (nothing is deleted or replaced silently).
pub(crate) fn write_new(file: &Path, bytes: &[u8]) -> Result<(), Coded<SettingCode>> {
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
