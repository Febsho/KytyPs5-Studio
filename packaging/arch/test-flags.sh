#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/PKGBUILD"

assert_equal() {
  local expected="$1"
  local actual="$2"
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected: %s\nactual:   %s\n' "$expected" "$actual" >&2
    exit 1
  fi
}

assert_equal \
  '-C opt-level=3 -C target-cpu=x86-64 -C strip=symbols' \
  "$(sanitize_rust_flags '-C opt-level=3 -C target-cpu=znver4 -C strip=symbols')"
assert_equal \
  '-C target-cpu=x86-64' \
  "$(sanitize_rust_flags '-C target-cpu=native')"
assert_equal \
  '-O2 -march=x86-64 -mtune=generic -pipe' \
  "$(sanitize_cpu_flags '-O2 -march=znver4 -mtune=znver4 -pipe')"
assert_equal \
  '-O2 -march=x86-64 -mtune=generic' \
  "$(sanitize_cpu_flags '-O2 -mcpu=skylake -mtune=native')"
assert_equal \
  '-O2 -mtune=generic' \
  "$(sanitize_cpu_flags '-O2 -mtune=znver4')"

printf 'Arch package CPU flag normalization passed.\n'
