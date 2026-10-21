#!/usr/bin/env bash
# Symlink the playbook skills into the agent skill directories so edits in this
# repo take effect immediately. Idempotent; refuses to overwrite a real directory.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
targets=("$HOME/.claude/skills" "$HOME/.agents/skills")

for dest in "${targets[@]}"; do
  [ -d "$dest" ] || { echo "skip $dest (missing)"; continue; }
  for skill in "$root"/skills/*/; do
    name="$(basename "$skill")"
    link="$dest/$name"
    if [ -L "$link" ]; then
      ln -sfn "${skill%/}" "$link"
      echo "updated $link"
    elif [ -e "$link" ]; then
      echo "REFUSE $link exists and is not a symlink" >&2
    else
      ln -s "${skill%/}" "$link"
      echo "linked  $link"
    fi
  done
done
