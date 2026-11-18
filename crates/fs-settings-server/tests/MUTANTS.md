# Mutants of the program fs-settings

Each mutant is made in a COPY of the tree (never in the checkout); the named test must turn red. 15 of 16 ran red at the head of the branch
`unreviewed/probe-fs-settings-serve-2` (Y5 is equivalent, see below). MC11 (a mutex around the store's runtime) is gone with the
mutex: tokio documents that a current-thread runtime may be entered by several threads, `kc6b_many_threads_many_rounds` is the evidence.

| Mutant | Edit | Red test |
|---|---|---|
| MC1 | `db_store.rs` `start`: ignore the result of `Store::inspect` (always `Absent`) | `db_start` kc4 folder, kc2 |
| MC2 | `db_store.rs`: a broken file is opened anyway | `db_start` kc4 folder |
| MC3 | `repair.rs`: `remove_file` instead of `rename` | `db_start` kc5 |
| MC4 | `exchange/doc.rs`: the TOML text with its lines reversed | `exchange` kc8 |
| MC5 | `exchange/import.rs`: a dry run applies | `exchange` kc9 |
| MC6 | `legacy.rs`: the "key is there" check dropped | `legacy` kc11 |
| MC7 | `legacy.rs`: writes the old file | `legacy` kc11 |
| MC8 | `words.rs`: `--by` is a word of `instance set` again (actor from an argument) | `instance_cmds` kc12, `cli` kc12 |
| MC9 | `db_instance.rs`: the time from the caller | `instance_cmds` kc12 |
| MC10 | `db_instance.rs`: an unchanged value reports a change | `instance_cmds` kc12 |
| MC12 | `clock.rs`: the OS user from `USER` | `os_user` (both tests) |
| MC13 | `db_batch.rs`: the batch in two transactions | `exchange` kc10, y2, kc9 |
| Y2 | `exchange/plan.rs`: a held instance lock is planned as a write | `import_checks` y2 |
| Y4 | `cli_db.rs`: no check whether a service holds the file | `service` y4 |
| Y5 | `serve.rs`: the service takes the CLI-side OS user, not its own | EQUIVALENT since B2 f3f4b374: both give the same form (name, else number), so no test can tell them apart; `service` y5 still pins the form |
| Y6 | `exchange/plan.rs`: the declaration check off | `import_checks` y6, `service` (import on the command line) |
| Y7 | `serve.rs`: `claimed_by` not wired (a claimed `by` is not logged) | `claims` a_claimed_by_is_logged_and_never_the_actor |
