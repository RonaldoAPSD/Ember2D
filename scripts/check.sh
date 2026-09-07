#!/usr/bin/env bash
# scripts/check.sh - bash twin of check.ps1 (created 7A-6,
# docs/ember2d-master-plan.md §5.1 / §6.5) for the Linux CI runner 7A-7
# adds. Keep both scripts' checks in sync by hand - see check.ps1's own
# header for what each check does and does not try to catch, and why.

set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
failures=()

# --- 1. File size: no .rs over 750 lines (CLAUDE.md) ---
while IFS= read -r -d '' file; do
    lines=$(wc -l < "$file")
    if [ "$lines" -gt 750 ]; then
        rel="${file#"$root"/}"
        failures+=("$rel: $lines lines (limit 750)")
    fi
done < <(find "$root/ember2d-sim" "$root/ember2d" "$root/ember2d-editor" "$root/ember2d-app" -name '*.rs' -print0)

# --- 2. ember2d-sim determinism rules (CLAUDE.md's Determinism section) ---
# Scoped to ember2d-sim/src only (not examples/) - bench_sim.rs is a dev
# benchmarking tool that legitimately uses Instant/eprintln!/fs to report
# perf numbers to stdout; it never ships as part of the simulation a game
# actually runs. Test files (name contains "test") are exempt for the
# same reason: fixtures writing/reading temp files, or a PartialOrd trait
# impl used only in test-adjacent code, aren't that hazard either.
mapfile -d '' sim_files < <(find "$root/ember2d-sim/src" -name '*.rs' ! -name '*test*' -print0)

is_comment_line() {
    # $1 = line text. True if the trimmed line starts with //.
    [[ "$(echo "$1" | sed 's/^[[:space:]]*//')" == //* ]]
}

check_pattern() {
    local pattern="$1" label="$2"; shift 2
    local -a allowlist=("$@")
    for file in "${sim_files[@]}"; do
        base="$(basename "$file")"
        skip=0
        for a in "${allowlist[@]:-}"; do
            [ "$base" = "$a" ] && skip=1 && break
        done
        [ "$skip" -eq 1 ] && continue
        while IFS=: read -r lineno line; do
            is_comment_line "$line" && continue
            failures+=("$base:$lineno: $label")
        done < <(grep -nE "$pattern" "$file" 2>/dev/null)
    done
}

# 2a. eprintln! - ALLOWLISTED: R41 (world.rs:142, already tracked, 7.5-9).
check_pattern 'eprintln!\(' "new eprintln! in ember2d-sim (forbidden - CLAUDE.md Determinism)" "world.rs"

# 2b. Filesystem access - ALLOWLISTED: R17 (simulation.rs, spawn.rs;
# already tracked, 7.5-9). level.rs/save.rs's own load/save entry points
# are the file format's real load/save, not the "reachable from a running
# step" hazard R17 is about - excluded from the scan entirely, below.
sim_files_no_io_entry=()
for f in "${sim_files[@]}"; do
    b="$(basename "$f")"
    [ "$b" = "level.rs" ] && continue
    [ "$b" = "save.rs" ] && continue
    sim_files_no_io_entry+=("$f")
done
sim_files_saved=("${sim_files[@]}")
sim_files=("${sim_files_no_io_entry[@]}")
check_pattern 'std::fs::|\.exists\(\)' "new filesystem access in ember2d-sim (forbidden - CLAUDE.md Determinism)" "simulation.rs" "spawn.rs"
sim_files=("${sim_files_saved[@]}")

# 2c. Instant::now() / SystemTime::now() - wall-clock time.
check_pattern 'Instant::now\(\)|SystemTime::now\(\)' "wall-clock time in ember2d-sim (forbidden - CLAUDE.md Determinism)"

# 2d. partial_cmp(..).unwrap_or(..) sort anti-pattern.
check_pattern 'partial_cmp\([^)]*\)[[:space:]]*\.unwrap_or' "partial_cmp(..).unwrap_or(..) sort in ember2d-sim - use f32::total_cmp instead"

# --- 3. cargo tree -p ember2d-sim --depth 1: exactly serde/ron/rhai/rand ---
cd "$root" || exit 1
tree_output="$(cargo tree -p ember2d-sim --depth 1 2>&1)"
while IFS= read -r line; do
    dep="$(echo "$line" | grep -oE '[A-Za-z][A-Za-z0-9_-]* v[0-9]' | sed -E 's/ v[0-9]$//')"
    [ -z "$dep" ] && continue
    [ "$dep" = "ember2d-sim" ] && continue
    case "$dep" in
        serde|ron|rhai|rand) ;;
        *) failures+=("cargo tree -p ember2d-sim --depth 1 shows unexpected dependency '$dep' (allowed: serde, ron, rhai, rand - master plan §4.2)") ;;
    esac
done <<< "$tree_output"

# --- 4. Doc numbers (7A-6, docs/ember2d-master-plan.md §5.1) ---
# No bash port of doc-check.ps1 yet - PowerShell-only for now (Windows is
# this project's primary dev platform; add a doc-check.sh alongside this
# file once CI actually needs one, 7A-7).

# --- 5. cargo fmt --check ---
# Not yet applicable - 7A-9 hasn't chosen Option A/B yet.

if [ "${#failures[@]}" -gt 0 ]; then
    echo "check.sh FAILED:"
    for f in "${failures[@]}"; do echo "  - $f"; done
    exit 1
fi

echo "check.sh: all checks passed (note: doc-number check is PowerShell-only for now, see section 4 above)."
exit 0
