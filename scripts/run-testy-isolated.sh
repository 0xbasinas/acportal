#!/usr/bin/env bash
# Run the pinned agent suite with a fresh /tmp; the upstream agent uses fixed paths.
set -euo pipefail
test "$(uname -s)" = Linux
: "${ACPD_TESTY_BIN:?Build testy first with scripts/build-testy.sh}"
test -f "$ACPD_TESTY_BIN"
test_bin="$(cargo test --locked -p acpd --test testy --no-run --message-format=json |
  jq -r 'select(.reason == "compiler-artifact" and .profile.test and .target.name == "testy") | .executable')"
test -x "$test_bin"
test_bin="$(realpath "$test_bin")"
agent_bin="$(realpath "$ACPD_TESTY_BIN")"
# Only mount setup runs as root. Tests and agent run as the invoking user. Namespace
# teardown frees all fixture files, including after a panic or failed assertion.
sudo unshare --mount --propagation private bash -c '
  set -euo pipefail
  mount -t tmpfs -o mode=1777,nosuid,nodev tmpfs /tmp
  printf "acpd-testy isolated fixture\n" > /tmp/acpd-testy-private-mount
  chmod 444 /tmp/acpd-testy-private-mount
  exec setpriv --reuid "$SUDO_UID" --regid "$SUDO_GID" --init-groups \
    env ACPD_TESTY_PRIVATE_TMP=1 ACPD_TESTY_BIN="$1" "$2" --nocapture
' bash "$agent_bin" "$test_bin"
