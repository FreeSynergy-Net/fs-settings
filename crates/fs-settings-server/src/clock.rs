//! The clock and the OS user, as injectable values (tests give their own; the service never takes either from the caller).
//! Pattern: Strategy (the clock and the OS user are values the caller injects).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ports::Utc;

/// A clock: UTC milliseconds.
pub type Clock = Arc<dyn Fn() -> Utc + Send + Sync>;

/// Who the OS says the user is.
pub type OsUser = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// The system clock.
#[must_use]
pub fn system_clock() -> Clock {
    Arc::new(|| {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        Utc::from_millis(ms)
    })
}

/// A clock that always says `at` (tests, `repair`).
#[must_use]
pub fn fixed_clock(at: Utc) -> Clock {
    Arc::new(move || at)
}

/// The OS user of this process, from the operating system and NEVER from an environment variable (`USER`, `LOGNAME`, `USERNAME`
/// can be set by anyone, and the log must not be forgeable). Inside a service call the service asks its own, per-call answer
/// (`fs_settings_rpc::current_os_user`); this is for the CLI and for stores opened outside a call.
#[must_use]
pub fn system_os_user() -> OsUser {
    Arc::new(os_user_now)
}

/// Unix: the real user id of the process (`getuid`), as the name the user database (`/etc/passwd`) gives it; the number itself when
/// there is no such line.
#[cfg(unix)]
fn os_user_now() -> Option<String> {
    let uid = rustix::process::getuid().as_raw();
    let name = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let mut fields = line.split(':');
                let name = fields.next()?;
                let id = fields.nth(1)?;
                (id.parse::<u32>().ok() == Some(uid)).then(|| name.to_owned())
            })
        });
    Some(name.unwrap_or_else(|| uid.to_string()))
}

/// Windows: `GetUserNameW` (the account of the process token). Written for the Windows runner of the CI; not run on Linux.
#[cfg(windows)]
fn os_user_now() -> Option<String> {
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;
    let mut buffer = [0_u16; 257];
    let mut len = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is writable for `len` UTF-16 units; the call writes at most that many and sets `len` (with the final NUL).
    let done = unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut len) };
    if done == 0 {
        return None;
    }
    String::from_utf16(&buffer[..usize::try_from(len).ok()?.saturating_sub(1)]).ok()
}
