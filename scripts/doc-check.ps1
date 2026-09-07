# scripts/doc-check.ps1 - asserts the format version, API_VERSION, and
# registered-function count quoted in CLAUDE.md match the tree (7A-6,
# docs/ember2d-master-plan.md par.5.1, R35/R36). Part of check.ps1 (par.6.5) -
# not meant to be run standalone in normal use, but safe to.
#
# Deliberately does NOT check a "test count": CLAUDE.md and
# docs/ember2d-regression-checklist.md stopped quoting one as part of this
# same 7A-6 pass - a raw test count goes stale the instant any later step
# adds a test, so both docs now point at `cargo test --workspace` instead
# of a number this script (and a human editing CLAUDE.md) would otherwise
# have to keep re-verifying in lockstep. See those docs' own edits for why.
#
# Also deliberately does NOT check docs/ember2d-master-plan.md section 2.3's
# "Baseline numbers" table, even though section 6.1 says check.ps1 covers
# "CLAUDE.md or 2.3" - that table is explicitly a FROZEN snapshot "at
# cf59f42" (section 2.1's own header: "Rewrite this section at every phase
# gate"), not a live-tracked figure. Checking it against the current tree
# between phase gates would fail by design, not by drift. Flagged here
# rather than silently narrowed: section 6.1's own wording and section 2.1's
# "frozen until phase gate" convention are in real tension, and resolving
# that is a plan-design question, not a script-implementation one.
#
# NOTE ON ENCODING: this file is plain ASCII on purpose - Windows
# PowerShell 5.1 does not reliably auto-detect UTF-8 without a BOM, and a
# stray em-dash or curly quote turns into mojibake that breaks parsing.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$claudeMdPath = Join-Path $root "CLAUDE.md"
$claudeMd = Get-Content $claudeMdPath -Raw

$failures = @()

# --- LEVEL_FORMAT_VERSION ---
$levelRsPath = Join-Path $root "ember2d-sim/src/level.rs"
$levelMatch = Select-String -Path $levelRsPath -Pattern "pub const LEVEL_FORMAT_VERSION: u32 = (\d+);"
if (-not $levelMatch) {
    $failures += "could not find LEVEL_FORMAT_VERSION's definition in $levelRsPath"
} else {
    $actualLevelVersion = $levelMatch.Matches[0].Groups[1].Value
    if ($claudeMd -notmatch "LEVEL_FORMAT_VERSION.{0,40}currently $actualLevelVersion\b") {
        $failures += "CLAUDE.md's quoted LEVEL_FORMAT_VERSION doesn't match level.rs's actual value ($actualLevelVersion)"
    }
}

# --- API_VERSION ---
$typesRsPath = Join-Path $root "ember2d-sim/src/scripting/types.rs"
$apiMatch = Select-String -Path $typesRsPath -Pattern "pub const API_VERSION: i64 = (\d+);"
if (-not $apiMatch) {
    $failures += "could not find API_VERSION's definition in $typesRsPath"
} else {
    $actualApiVersion = $apiMatch.Matches[0].Groups[1].Value
    if ($claudeMd -notmatch "API_VERSION.{0,10}$actualApiVersion\b") {
        $failures += "CLAUDE.md's quoted API_VERSION doesn't match types.rs's actual value ($actualApiVersion)"
    }
}

# --- Registered script function count ---
$engineRsPath = Join-Path $root "ember2d-sim/src/scripting/engine.rs"
$actualFnCount = (Select-String -Path $engineRsPath -Pattern "register_fn").Count
if ($claudeMd -notmatch "$actualFnCount registered functions") {
    $failures += "CLAUDE.md's quoted registered-function count doesn't match engine.rs's actual count (grep -c register_fn = $actualFnCount)"
}

if ($failures.Count -gt 0) {
    Write-Host "doc-check FAILED:" -ForegroundColor Red
    foreach ($f in $failures) { Write-Host "  - $f" -ForegroundColor Red }
    exit 1
}

Write-Host "doc-check: CLAUDE.md's quoted LEVEL_FORMAT_VERSION/API_VERSION/function count all match the tree."
exit 0
