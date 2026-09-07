# Cascadia Mono

`CascadiaMono.ttf` is Microsoft's Cascadia Code/Mono typeface
(https://github.com/microsoft/cascadia-code), copied here from this
machine's own Windows installation (`C:\Windows\Fonts\CascadiaMono.ttf` —
it ships with Windows Terminal / VS Code).

**License: SIL Open Font License, Version 1.1** — the OFL is specifically
designed to permit bundling a font with software. The exact upstream text
(including the "Copyright (c) 2019 - Present, Microsoft Corporation, with
Reserved Font Name Cascadia Code" notice and Reserved Font Name clause) is
bundled verbatim at `ember2d/assets/fonts/OFL.txt` (7A-8,
docs/ember2d-master-plan.md §5.1, R39), copied byte-for-byte from
`github.com/microsoft/cascadia-code`'s own `LICENSE` file. The canonical
license text (with no font-specific copyright notice) also lives at
https://scripts.sil.org/OFL.

Used as Phase 7 Part 2's (docs/ember2d-phase7-plan.md) test/placeholder
TTF for `TtfFont` — monospace, so its glyph metrics are easy to reason
about in tests. Not yet wired into any theme; Part 3 is expected to
formalize which font(s) ship with which theme.
