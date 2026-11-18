//! Windows only (not run on Linux; the Windows CI is the evidence): the actor of the command line is `urn:fs:os-user:S-1-...` and equals
//! the actor `fs-settings-rpc` derives for a REAL pipe connection of this same user, so the command line and the service name one user
//! one way (the SID), the account name being display only.
#![cfg(windows)]

use fs_rpc::CallerSource;
use fs_types::BasicValue;
use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

#[tokio::test]
async fn the_command_line_actor_is_the_sid_the_service_derives_from_the_pipe() {
    let path = format!(r"\\.\pipe\fs-settings-test-actor-{}", std::process::id());
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&path)
        .unwrap();
    let _client = ClientOptions::new().open(&path).unwrap();
    server.connect().await.unwrap();
    let caller = server.caller().unwrap();
    let from_pipe = fs_settings_rpc::actor_of(&caller);
    let from_command_line = fs_settings_server::os_actor().unwrap();
    assert!(
        from_command_line
            .canonical()
            .starts_with("urn:fs:os-user:S-1-"),
        "{}",
        from_command_line.canonical()
    );
    assert_eq!(from_command_line, from_pipe);
}
