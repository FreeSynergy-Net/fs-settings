//! One form per user (pure functions; no environment, no files): the actor is made of the ID (Unix uid, Windows SID); a name, even one with
//! spaces, is display only and never the actor; a user without a passwd line keeps its uid as the actor.

use fs_types::BasicValue;

/// The actor is the id, never the name.
#[test]
fn the_actor_is_the_id_and_never_the_name() {
    let unix = fs_settings_server::actor_of_user("1000", Some("ada".to_owned()));
    assert_eq!(unix.canonical(), "urn:fs:os-user:1000");
    let windows = fs_settings_server::actor_of_user(
        "S-1-5-21-111-222-333-1001",
        Some("Ada Lovelace".to_owned()),
    );
    assert_eq!(
        windows.canonical(),
        "urn:fs:os-user:S-1-5-21-111-222-333-1001"
    );
    let nameless = fs_settings_server::actor_of_user("1000", None);
    assert_eq!(nameless.canonical(), "urn:fs:os-user:1000");
}

/// A user the passwd text names gets its name; one it does not name (NSS or LDAP only) is NOT refused: the uid is the actor, the name is
/// none. The passwd text is injected, so this needs no such user on the machine.
#[cfg(unix)]
#[test]
fn a_user_without_a_passwd_line_keeps_the_uid() {
    let passwd = "root:x:0:0:root:/root:/bin/bash\nada:x:1000:1000:Ada L:/home/ada:/bin/sh\n# comment\n\nbroken line\n";
    assert_eq!(
        fs_settings_server::user_of(1000, Some(passwd)),
        ("1000".to_owned(), Some("ada".to_owned()))
    );
    assert_eq!(
        fs_settings_server::user_of(4242, Some(passwd)),
        ("4242".to_owned(), None)
    );
    assert_eq!(
        fs_settings_server::user_of(4242, None),
        ("4242".to_owned(), None)
    );
    // and its actor is the uid
    let (id, name) = fs_settings_server::user_of(4242, Some(passwd));
    assert_eq!(
        fs_settings_server::actor_of_user(id, name).canonical(),
        "urn:fs:os-user:4242"
    );
}
