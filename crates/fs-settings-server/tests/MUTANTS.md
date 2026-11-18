# Mutants of the program fs-settings

The definitions and the runner live OUTSIDE the tree (`/srv/FreeSynergy/.scratch/mutanten/fs-settings/fs-settings-server/run-mutants.py`, status
per head in `status-<head>.tsv`). Each mutant is applied to a COPY of the tree; the named test must turn red. At the head of the branch
`unreviewed/probe-fs-settings-serve-4` all 18 turned red.

The per-connection logic (one hub per connection, the actor from `Caller::id`, a refused caller logged) lives in `fs-settings-rpc` (B2-4);
its mutants ("actor from the name", "first connection for all", "caller error ignored") are defined and tested THERE. Here only the wiring
is mutated: the service's OS-user source (Y5a), the claim hook (Y7), the actor of a command without a connection (Y12).

| Mutant | Edit | Red test |
|---|---|---|
| MC1 | `db_store.rs`: the result of `Store::inspect` ignored | `db_start` kc4 folder, kc2 |
| MC2 | `db_store.rs`: a broken file is opened anyway | `db_start` kc4 folder |
| MC3 | `repair.rs`: `remove_file` instead of `rename` | `db_start` kc5 |
| MC4 | `exchange/doc.rs`: the TOML text with its lines reversed | `exchange` kc8 |
| MC5 | `exchange/import.rs`: a dry run applies | `exchange` kc9 |
| MC6 | `legacy.rs`: the "key is there" check dropped | `legacy` kc11 |
| MC7 | `legacy.rs`: writes the old file | `legacy` kc11 |
| MC8 | `words.rs`: `--by` is a word of `instance set` again | `instance_cmds` kc12, `cli` kc12 |
| MC9 | `db_instance.rs`: the time from the caller | `instance_cmds` kc12 |
| MC10 | `db_instance.rs`: an unchanged value reports a change | `instance_cmds` kc12 |
| MC12 | `clock.rs`: the OS user from `USER` | `os_user` (both tests) |
| MC13 | `db_batch.rs`: the batch in two transactions | `exchange` kc10, kc9 |
| Y2 | `exchange/plan.rs`: a held instance lock is planned as a write | `import_checks` y2 |
| Y4 | `cli_db.rs`: no check whether a service holds the file | `service` y4 |
| Y5a | `serve.rs`: the store's OS user is not the one the service set for the connection | `service` y5 |
| Y6 | `exchange/plan.rs`: the declaration check off | `import_checks` y6, `service` (import on the command line) |
| Y7 | `serve.rs`: the claim hook switched off | `claims` |
| Y12 | `actor.rs`: the actor of a command line is a fixed value | `os_user` (command-line actor) |
