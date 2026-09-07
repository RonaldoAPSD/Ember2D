# scripts/check.ps1 - the phase-gate lint/hygiene gate (created 7A-6,
# docs/ember2d-master-plan.md par.5.1; extended as later phases add rules -
# see par.6.5). Run from anywhere; resolves paths relative to the repo root.
#
# What this does NOT try to be: a full clippy-lint replacement. Several
# checks below are deliberately coarse greps, not real static analysis -
# see each section's own comment for its specific blind spots. 7.5-9 is
# where real disallowed_types/disallowed_methods clippy config takes over
# the fs/Instant/eprintln!/HashMap-iteration checks; until then this is
# what CLAUDE.md's Determinism rules actually get enforced by.
#
# NOTE ON ENCODING: this file is plain ASCII on purpose - Windows
# PowerShell 5.1 does not reliably auto-detect UTF-8 without a BOM, and a
# stray em-dash or curly quote turns into mojibake that breaks parsing.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$failures = @()

function Get-RsFiles($subpath) {
    Get-ChildItem -Path (Join-Path $root $subpath) -Recurse -Filter "*.rs"
}

# --- 1. File size: no .rs over 750 lines (CLAUDE.md) ---
$allRsFiles = @()
foreach ($crate in @("ember2d-sim", "ember2d", "ember2d-editor", "ember2d-app")) {
    $allRsFiles += Get-RsFiles $crate
}
foreach ($file in $allRsFiles) {
    $lineCount = (Get-Content $file.FullName | Measure-Object -Line).Lines
    if ($lineCount -gt 750) {
        $rel = $file.FullName.Substring($root.Length + 1)
        $failures += "$rel : $lineCount lines (limit 750)"
    }
}

# --- 2. ember2d-sim determinism rules (CLAUDE.md's Determinism section) ---
# Scoped to ember2d-sim/src only (not examples/) - bench_sim.rs is a dev
# benchmarking tool that legitimately uses Instant/eprintln!/fs to report
# perf numbers to stdout; it never ships as part of the simulation a game
# actually runs, so it isn't the "ambient I/O in game logic" hazard these
# rules exist for. Test files (name contains "test") are exempt for the
# same reason: fixtures writing/reading temp files, or a PartialOrd trait
# impl used only in test-adjacent code, aren't that hazard either.
$simRsFiles = (Get-RsFiles "ember2d-sim/src") | Where-Object { $_.Name -notmatch "test" }

# Matches CLAUDE.md's own convention: a comment discussing why a pattern
# is forbidden (illustrating it as prose, e.g. "Instant::now() would be a
# determinism violation") is not a real occurrence of that pattern. Only
# checks the START of the trimmed line - good enough for this codebase's
# own comment style (a `//`/`///` line, not code with a trailing comment),
# and simpler/more predictable than trying to strip trailing comments from
# a code line without a real Rust tokenizer.
function Test-IsCommentLine($line) {
    return $line.TrimStart().StartsWith("//")
}

# 2a. eprintln! - real call sites only (eprintln! followed by an open
# paren, not a doc comment discussing the rule). ALLOWLISTED: R41
# (world.rs:142, docs/ember2d-master-plan.md section 3.2) - already
# tracked, scheduled for 7.5-9; don't let this check regress to "always
# red" over a known, deferred issue.
$eprintlnAllowlist = @("world.rs")
foreach ($file in $simRsFiles) {
    if ($eprintlnAllowlist -contains $file.Name) { continue }
    $hits = Select-String -Path $file.FullName -Pattern "eprintln!\("
    foreach ($hit in $hits) {
        if (Test-IsCommentLine $hit.Line) { continue }
        $failures += "$($file.Name):$($hit.LineNumber): new eprintln! in ember2d-sim (forbidden - CLAUDE.md Determinism)"
    }
}

# 2b. Filesystem access - std::fs:: calls or Path::...exists() checks.
# ALLOWLISTED: R17 (simulation.rs's resolve_exit_path, simulation/spawn.rs's
# node-graph script combine, docs/ember2d-master-plan.md section 3.2) -
# already tracked, scheduled for 7.5-9. NOT checked at all: level.rs/save.rs's
# own LevelData::load/save and SaveState::load_from_file/save_to_file -
# those ARE the file format's real load/save entry points, callable only
# between simulation runs, not the "filesystem access reachable from a
# running step" hazard R17 is actually about. A blanket "no std::fs
# anywhere in ember2d-sim" reading of CLAUDE.md's rule would make loading a
# level at all forbidden, which was never the intent.
$fsAllowlist = @("simulation.rs", "spawn.rs")
$fsCheckFiles = $simRsFiles | Where-Object { $_.Name -ne "level.rs" -and $_.Name -ne "save.rs" }
foreach ($file in $fsCheckFiles) {
    if ($fsAllowlist -contains $file.Name) { continue }
    $hits = Select-String -Path $file.FullName -Pattern "std::fs::|\.exists\(\)"
    foreach ($hit in $hits) {
        if (Test-IsCommentLine $hit.Line) { continue }
        $failures += "$($file.Name):$($hit.LineNumber): new filesystem access in ember2d-sim (forbidden - CLAUDE.md Determinism)"
    }
}

# 2c. Instant::now() / SystemTime::now() - wall-clock time.
foreach ($file in $simRsFiles) {
    $hits = Select-String -Path $file.FullName -Pattern "Instant::now\(\)|SystemTime::now\(\)"
    foreach ($hit in $hits) {
        if (Test-IsCommentLine $hit.Line) { continue }
        $failures += "$($file.Name):$($hit.LineNumber): wall-clock time in ember2d-sim (forbidden - CLAUDE.md Determinism)"
    }
}

# 2d. partial_cmp(..).unwrap_or(..) sort anti-pattern - the specific
# pattern CLAUDE.md forbids, not every "fn partial_cmp" (a correct
# PartialOrd trait impl delegating to a real total order, like
# api_spatial.rs's own, is fine and common - flagging bare partial_cmp
# would false-positive on it).
foreach ($file in $simRsFiles) {
    $hits = Select-String -Path $file.FullName -Pattern "partial_cmp\([^)]*\)\s*\.unwrap_or"
    foreach ($hit in $hits) {
        if (Test-IsCommentLine $hit.Line) { continue }
        $failures += "$($file.Name):$($hit.LineNumber): partial_cmp(..).unwrap_or(..) sort in ember2d-sim - use f32::total_cmp instead"
    }
}

# --- 3. cargo tree -p ember2d-sim --depth 1: exactly serde/ron/rhai/rand ---
Push-Location $root
try {
    $treeOutput = & cargo tree -p ember2d-sim --depth 1 2>&1
    $depNames = @()
    foreach ($line in $treeOutput) {
        if ($line -match '([A-Za-z][A-Za-z0-9_-]*) v\d') {
            $depNames += $matches[1]
        }
    }
    $depNames = $depNames | Where-Object { $_ -ne "ember2d-sim" } | Select-Object -Unique
    $allowedDeps = @("serde", "ron", "rhai", "rand")
    foreach ($dep in $depNames) {
        if ($allowedDeps -notcontains $dep) {
            $failures += "cargo tree -p ember2d-sim --depth 1 shows unexpected dependency '$dep' (allowed: serde, ron, rhai, rand - master plan section 4.2)"
        }
    }
} finally {
    Pop-Location
}

# --- 4. Doc numbers (7A-6, docs/ember2d-master-plan.md par.5.1) ---
& (Join-Path $PSScriptRoot "doc-check.ps1")
if ($LASTEXITCODE -ne 0) {
    $failures += "scripts/doc-check.ps1 failed (see its own output above)"
}

# --- 5. cargo fmt --check ---
# Not yet applicable - 7A-9 (docs/ember2d-master-plan.md par.5.1) hasn't
# chosen Option A (adopt rustfmt) or Option B (formally opt out) yet. Add
# the cargo fmt --check call here once it has.

if ($failures.Count -gt 0) {
    Write-Host "check.ps1 FAILED:" -ForegroundColor Red
    foreach ($f in $failures) { Write-Host "  - $f" -ForegroundColor Red }
    exit 1
}

Write-Host "check.ps1: all checks passed." -ForegroundColor Green
exit 0
