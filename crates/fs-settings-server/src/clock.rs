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

fn os_user_now() -> Option<String> {
    process_user().map(|(id, name)| name.unwrap_or(id))
}

/// The user of this process as the system says: `(id, name)`. Unix: the effective user id (`geteuid`, the semantics of `SO_PEERCRED`)
/// and its name from `/etc/passwd`. Windows: the account name of the process token (`GetUserNameW`; no SID is asked, the name is the
/// id). This is for commands WITHOUT a connection; a service takes the user of each connection from `fs_rpc::Caller`.
#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)] // the same signature as the Windows one, which can fail
pub(crate) fn process_user() -> Option<(String, Option<String>)> {
    let uid = rustix::process::geteuid().as_raw();
    let name = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let mut fields = line.split(':');
                let name = fields.next()?;
                let id = fields.nth(1)?;
                (id.parse::<u32>().ok() == Some(uid) && !name.is_empty()).then(|| name.to_owned())
            })
        });
    Some((uid.to_string(), name))
}

/// Windows: the id is the SID of the process token (`GetTokenInformation(TokenUser)`, as text), the name is the account name
/// (`GetUserNameW`, display only: it may hold spaces and is never the actor). The service takes the SID of the pipe client the same way, so
/// a command line and the service name one user one way. Written for the Windows runner of the CI; NOT run on Linux.
#[cfg(windows)]
pub(crate) fn process_user() -> Option<(String, Option<String>)> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::System::WindowsProgramming::GetUserNameW;

    let mut buffer = [0_u16; 257];
    let mut len = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is writable for `len` UTF-16 units; the call writes at most that many and sets `len` (with the final NUL).
    let named = unsafe { GetUserNameW(buffer.as_mut_ptr(), &raw mut len) };
    let name = (named != 0)
        .then(|| String::from_utf16(&buffer[..usize::try_from(len).ok()?.saturating_sub(1)]).ok())
        .flatten();

    let mut token: HANDLE = null_mut();
    // SAFETY: `token` receives a handle that is closed below; the process pseudo handle needs no closing.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return None;
    }
    let sid = (|| {
        let mut size = 0_u32;
        // SAFETY: the first call only asks for the size (it fails with "insufficient buffer" and sets `size`).
        unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &raw mut size) };
        let words = usize::try_from(size).ok()?.div_ceil(8).max(1);
        let mut buf = vec![0_u64; words];
        // SAFETY: `buf` is 8-byte aligned and at least `size` bytes; the token handle is open.
        let done = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr().cast(),
                size,
                &raw mut size,
            )
        };
        if done == 0 {
            return None;
        }
        // SAFETY: the call filled `buf` with a `TOKEN_USER`; its SID points into `buf`, which lives to the end of this closure.
        let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut wide: *mut u16 = null_mut();
        // SAFETY: `wide` receives a NUL-terminated UTF-16 string allocated by the system, freed with `LocalFree` below.
        if unsafe { ConvertSidToStringSidW(sid, &raw mut wide) } == 0 {
            return None;
        }
        let mut n = 0_usize;
        // SAFETY: `wide` is NUL-terminated; the slice covers exactly the characters before the NUL, then the string is freed.
        unsafe {
            while *wide.add(n) != 0 {
                n += 1;
            }
            let text = String::from_utf16(std::slice::from_raw_parts(wide, n)).ok();
            LocalFree(wide.cast());
            text
        }
    })();
    // SAFETY: `token` was opened above and is closed once.
    unsafe { CloseHandle(token) };
    sid.map(|id| (id, name))
}
