//! The kit of the service tests: an isolated command, a running service that is ended by a guard, the client limits.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use fs_settings_rpc::ClientTimes;

pub fn bin(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fs-settings"));
    for (name, sub) in [
        ("HOME", "home"),
        ("XDG_RUNTIME_DIR", "run"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_STATE_HOME", "state"),
    ] {
        let dir = home.join(sub);
        std::fs::create_dir_all(&dir).unwrap();
        cmd.env(name, dir);
    }
    cmd
}

/// A running service; ended (stop, else kill) when dropped.
pub struct Service {
    child: Option<Child>,
    pub socket: PathBuf,
}

impl Service {
    pub fn start(home: &Path, data: &Path, instance: bool) -> Self {
        Self::start_logged(home, data, instance, Stdio::null())
    }

    /// As `start`, with the service's stderr going to `log` (the program's log: claims are written there).
    pub fn start_logged(home: &Path, data: &Path, instance: bool, log: Stdio) -> Self {
        let socket = home.join("settings.sock");
        let mut cmd = bin(home);
        cmd.args(["serve", "--socket"])
            .arg(&socket)
            .arg("--data")
            .arg(data);
        if instance {
            cmd.arg("--instance");
        }
        let child = cmd
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let service = Self {
            child: Some(child),
            socket,
        };
        let end = Instant::now() + Duration::from_secs(10);
        while !service.socket.exists() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(service.socket.exists(), "the service did not appear");
        service
    }

    /// `Control.stop`, then the exit code.
    pub fn stop(mut self) -> i32 {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async {
            fs_rpc::stop_server(&self.socket).await.unwrap();
        });
        let mut child = self.child.take().unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status.code().unwrap_or(-1);
            }
            assert!(Instant::now() < end, "the service did not stop");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn times() -> ClientTimes {
    ClientTimes {
        call: Duration::from_secs(10),
        retry: Duration::from_secs(1),
    }
}
