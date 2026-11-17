#!/usr/bin/env bash
# Instance defaults and locks of this server, as lines of `fs-settings instance ...`. Run by a HUMAN (the administrator), never by an agent.
# Every change needs an actor (--by, an IRI) and, for lock and unlock, a reason. Running it twice changes nothing (`same`).
set -euo pipefail

BY="${FS_SETTINGS_BY:?set FS_SETTINGS_BY to your own IRI, e.g. https://example.org/users/admin}"

fs-settings instance set fs.shell.language de --by "$BY"
fs-settings instance lock fs.shell.style calm --reason "organisation policy" --by "$BY"
# to release it again (a reason is needed here too):
# fs-settings instance unlock fs.shell.style --reason "policy changed" --by "$BY"
fs-settings instance show
