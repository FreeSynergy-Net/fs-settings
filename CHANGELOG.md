# Changelog

Version = tag; all crates of the workspace carry the same version.

## Unreleased

### English

- New program `fs-settings` (probe C of the trait-delta `fs-settings-store`, not compiled yet): `fs-settings serve` owns the settings database; `DbSettingsStore` implements the port over `fs-objects` `v5::setting`; emergency mode with the visible codes `store-broken` and `store-newer` (the file stays byte-identical); `repair`; `instance set|clear|lock|unlock|log` (reason mandatory, also for unlock; time and OS user set by the service); export and import as TOML and JSON (round trip, one deterministic text); one-time read of the published language files and the frontend choice of fs-tasks (never written, never deleted). No import of `settings.toml` or `instance.toml` (never published).

### Deutsch

- Neues Programm `fs-settings` (Probe C des trait-delta `fs-settings-store`, noch nicht übersetzt): `fs-settings serve` besitzt die Einstellungs-Datenbank; `DbSettingsStore` setzt den Port über `fs-objects` `v5::setting` um; Notbetrieb mit den sichtbaren Codes `store-broken` und `store-newer` (die Datei bleibt byte-gleich); `repair`; `instance set|clear|lock|unlock|log` (Grund Pflicht, auch beim Entsperren; Zeit und Betriebssystem-Nutzer setzt der Dienst); Export und Import als TOML und JSON (Rundlauf, ein deterministischer Text); einmaliges Lesen der veröffentlichten Sprachdateien und der Frontend-Wahl von fs-tasks (nie geschrieben, nie gelöscht). Kein Import von `settings.toml` oder `instance.toml` (nie veröffentlicht).
