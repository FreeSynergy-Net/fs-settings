# FreeSynergy Settings

**All settings of a person in one place, and in one database that one process owns.**

`fs-settings serve` owns the settings database of a person (and, started with `--instance`, the defaults and locks of an instance). Every program reaches it through the contract `settings` (Cap'n Proto, draft) and never opens the file itself. If the file is broken or newer than the program, the service says so openly (`store-broken`, `store-newer`) and serves the built-in defaults; the file is never touched, never deleted. Only the person's own `fs-settings repair` moves it aside.

Status: **probe.** Compiled and tested on Linux against tags of FreeSynergy-Net (see `Cargo.toml`: fs-objects v0.0.10, fs-components v0.0.35, fs-rpc v0.0.8, fs-types v0.0.6; one source per library): clippy pedantic clean, the gates green, 23 of 25 mutants of the filed set caught and 2 equivalent (OC6 and OC10: one user per transport today, empty catalog), plus the review's own mutants (definitions kept outside the tree). Who is calling: the service takes the actor of every request from the connection only (`fs_rpc::Caller::id`: Unix uid, Windows SID; the account name is display only); a `by` a client claims is logged (`claimed-by="..." ignored`) and never takes effect; the command line names its own user the same way, from the system, never from an argument or an environment variable. `import` needs declarations: this program has none of other programs yet, so an import REFUSES every value (`import-conflict{cause=not-declared}`, exit 1) and writes nothing; it runs in one database transaction. Transition until the contract 0.3 (a second cascade): `export` and `import` work on the database file and refuse (`store-busy`) while a service holds it, and `import --instance` names its own user from the system (`export` reads and needs none); with 0.3 they run through the service, `by` leaves the contract, and no own lookup is planned except possibly for `repair` (the emergency path, which has none today). Windows: the Windows code (SID of the process token) is linted and compiled by the Windows CI only, not run by the author. Written with the help of Claude (Anthropic); every line is reviewed by a person before release.

## Where the database lives

| Who | Linux | Windows |
|---|---|---|
| Person | `$XDG_DATA_HOME/freesynergy/settings/settings.db` (else `$HOME/.local/share/...`) | `%LOCALAPPDATA%\FreeSynergy\settings\settings.db` |
| Instance (`--instance`) | `/var/lib/freesynergy/instance/settings.db` | `%ProgramData%\FreeSynergy\instance\settings.db` |

Changed only by a start word: `--data DIR` (`DIR/settings.db`) or `--settings FILE`.

## Words

```text
fs-settings --version | --help
fs-settings serve [--instance] [--data DIR | --settings FILE] [--socket PATH]
fs-settings instance show | set KEY VALUE | clear KEY | lock KEY VALUE --reason TEXT | unlock KEY --reason TEXT | log [--after ID] [--size N]   (the actor is the OS user, never a word)
fs-settings export --format toml|json --out FILE [--program P] [--device] [--instance]
fs-settings import FILE [--dry-run] [--legacy] [--device] [--instance]
fs-settings repair [--data DIR | --settings FILE]
```

Exit codes: 0 done (also `same`: nothing changed), 1 failed (printed as `setting-error-<code> key=value ...`, no sentence), 2 wrong words.
TOML and JSON are for exchange only (export, import, round trip); the store is the database. Programs get the start word `--settings-socket PATH`.

## For developers

Crates: `fs-settings-server` (library: `DbSettingsStore`, paths, repair, exchange, legacy import, the words), `fs-settings-bin` (the command `fs-settings`). The port (`SettingsStore`, `InstanceStore`) is `fs-settings` of fs-components, the rows are `fs-objects` `v5::setting`, the wire is `fs-settings-rpc` of fs-rpc. Test notes and the mutants: `crates/fs-settings-server/tests/MUTANTS.md`. Licence: MIT OR Apache-2.0.
