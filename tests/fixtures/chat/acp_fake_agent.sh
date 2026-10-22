#!/bin/sh
# Fake ACP agent for src/chat/acp.rs tests: sh acp_fake_agent.sh FIXTURE CLIENT_LOG
# Fixture lines: "< json" = send to the client; "> text" = the next line from the client must
# contain text; ">> a b" = the next two lines, in either order, contain a and b. Every line the
# client sends is appended to CLIENT_LOG.
FIX=$1; LOG=$2
fail() { echo "fake agent: $*" >&2; exit 9; }
has() { case "$1" in *"$2"*) return 0 ;; esac; return 1; }
while IFS= read -r l <&3; do
  case "$l" in
    '<'*) printf '%s\n' "${l#< }" ;;
    '>>'*)
      set -- ${l#>>}
      IFS= read -r a || fail "stdin closed"
      IFS= read -r b || fail "stdin closed"
      printf '%s\n%s\n' "$a" "$b" >> "$LOG"
      for p in "$@"; do has "$a" "$p" || has "$b" "$p" || fail "expected $p in: $a / $b"; done ;;
    '>'*)
      IFS= read -r got || fail "stdin closed"
      printf '%s\n' "$got" >> "$LOG"
      has "$got" "${l#> }" || fail "expected ${l#> } got $got" ;;
  esac
done 3< "$FIX"
