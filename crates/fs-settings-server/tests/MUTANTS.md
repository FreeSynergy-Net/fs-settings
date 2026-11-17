# Mutants of the program fs-settings (ENTWURF C.9)

Each mutant is made in a COPY of the tree (never in the checkout) and one named test must turn red. Not run yet (nothing is compiled).

| Mutant | Edit | Must turn red |
|---|---|---|
| MC1 | `db_store.rs` `start`: skip `Store::inspect`, always `open_rows` | `db_start::kc4_emergency_leaves_the_file_alone` |
| MC2 | `db_store.rs` `writable`: return the store even in emergency mode (create one) | `db_start::kc4_emergency_leaves_the_file_alone` |
| MC3 | `repair.rs`: `remove_file` instead of `rename` | `db_start::kc5_repair_moves_aside` |
| MC4 | `exchange/export.rs`: build the maps in reverse order (unsorted text) | `exchange::kc8_round_trip` |
| MC5 | `exchange/import.rs`: apply before `plan` (write while checking) | `exchange::kc9_errors_dry_run_and_locks`, `exchange::kc10_a_fault_in_the_middle_changes_nothing` |
| MC6 | `legacy.rs` `take`: drop the "key already there" check | `legacy::kc11_legacy_files` |
| MC7 | `legacy.rs`: write the old file back after reading | `legacy::kc11_legacy_files` (bytes) |
| MC8 | `words.rs` `instance_cmd`: `--by` optional (empty default) | `instance_cmds::kc12_words_and_exit_codes`, `fs-settings-bin` `kc12_usage_exits_with_2` |
| MC9 | `db_instance.rs`: take `at` from `Stampd.at` instead of the clock | `instance_cmds::kc12_words_and_exit_codes` (the log time) |
| MC10 | `db_instance.rs` `changed`: always `true` | `instance_cmds::kc12_words_and_exit_codes` (`same`, one log row) |
| MC11 | `db_port.rs`: `rt.block_on` without the gate and called on a runtime thread (no `spawn_blocking` in the test) | `db_ops::kc6_eight_concurrent_writes` |
