#!/usr/bin/env bash
# Testsuite unter macOS, Gegenstück zu scripts/test.ps1: TypeScript, Rust-Unit-Tests,
# UI-Tests und End-to-End-Tests mit echten Modellen.
#
#   scripts/test.sh             alles außer Hardware
#   scripts/test.sh --quick     ohne Modelle, in rund einer Minute
#   scripts/test.sh --hardware  zusätzlich Mikrofon und System-Audio (spielt 11 s Ton ab)
#
# Der erste Lauf lädt Testaudio und Sortformer (~180 MB) nach src-tauri/target/test-fixtures.
# Die Kennzahlen der End-to-End-Tests stehen danach in src-tauri/target/test-reports/e2e,
# die des vorigen Laufs in e2e-previous.
set -u

quick=0
hardware=0
for arg in "$@"; do
  case "$arg" in
    --quick) quick=1 ;;
    --hardware) hardware=1 ;;
    *) echo "Unbekannte Option: $arg" >&2; exit 2 ;;
  esac
done

root="$(cd "$(dirname "$0")/.." && pwd)"
tauri="$root/src-tauri"
reports="$tauri/target/test-reports"
results=()
failed=0

step() {
  local name=$1 dir=$2
  shift 2
  printf '\n=== %s ===\n' "$name"
  local start=$SECONDS
  if (cd "$dir" && "$@"); then
    results+=("OK      $name ($((SECONDS - start)) s)")
  else
    results+=("FEHLER  $name ($((SECONDS - start)) s)")
    failed=1
  fi
}

step "TypeScript" "$root" bun x tsc --noEmit
step "Rust-Unit-Tests" "$tauri" cargo test --lib
step "UI-Tests (Playwright)" "$root" bun run test

if [ "$quick" -eq 0 ]; then
  if [ -d "$reports/e2e" ]; then
    rm -rf "$reports/e2e-previous"
    mv "$reports/e2e" "$reports/e2e-previous"
  fi
  args=(test --lib e2e:: -- --ignored --test-threads=1)
  [ "$hardware" -eq 0 ] && args+=(--skip e2e::hardware)
  step "End-to-End (echte Modelle)" "$tauri" cargo "${args[@]}"
fi

printf '\n=== Ergebnis ===\n'
printf '%s\n' "${results[@]}"
exit "$failed"
