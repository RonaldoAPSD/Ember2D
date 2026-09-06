# Cascadia Mono

`CascadiaMono.ttf` is Microsoft's Cascadia Code/Mono typeface
(https://github.com/microsoft/cascadia-code), copied here from this
machine's own Windows installation (`C:\Windows\Fonts\CascadiaMono.ttf` —
it ships with Windows Terminal / VS Code).

**License: SIL Open Font License, Version 1.1** — the OFL is specifically
designed to permit bundling a font with software. The full canonical
license text lives at https://scripts.sil.org/OFL and in the upstream
repo's `LICENSE` file; it is not reproduced here verbatim because it
wasn't available locally to copy byte-for-byte at the time this file was
added — fetch and include the exact upstream text before this project (or
this asset specifically) is ever redistributed outside local development.

Used as Phase 7 Part 2's (docs/ember2d-phase7-plan.md) test/placeholder
TTF for `TtfFont` — monospace, so its glyph metrics are easy to reason
about in tests. Not yet wired into any theme; Part 3 is expected to
formalize which font(s) ship with which theme.
