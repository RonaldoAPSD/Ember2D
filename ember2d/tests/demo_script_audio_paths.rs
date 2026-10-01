// tests/demo_script_audio_paths.rs — Regression test for a stale-path
// defect found live during the 7C gate's own demo smoke-launch (docs/
// ember2d-master-plan.md, R61): R58's demos/ move regenerated every
// shipped LEVEL's own `script`/`next_level` fields (already covered by
// `roguelike_level_integrity.rs`'s `every_script_and_next_level_path_...`
// test) but never touched the SCRIPT FILES' own content — several
// `demos/roguelike/scripts/*.rhai` files still called
// `ctx.play_sound("roguelike/audio/...")`/`play_music(...)` with the
// pre-move path, which `ember2d::audio::AudioEngine` resolves relative to
// the process CWD (repo root) exactly like level/script paths — so every
// affected call silently failed (`[audio] play_sound '...': The system
// cannot find the path specified`) the instant a player triggered it.
// Undetectable by `every_script_and_next_level_path_...` (that one only
// ever looks at level DATA, never at a script's own text) or by any
// gameplay test (none asserts on audio actually playing) — found only by
// literally launching a demo and reading its own stderr output.
//
// This scans every `.rhai` file under `demos/*/scripts/` for
// `play_sound`/`play_sound_at`/`play_music` calls with a string-literal
// first argument and checks that path exists on disk — a plain text scan,
// not a real Rhai parser (good enough for "does this literal path exist,"
// not general enough to resolve a call built from a variable/expression,
// which none of the shipped scripts do).

use std::path::Path;

fn audio_paths_referenced(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for call in ["play_sound_at(", "play_sound(", "play_music("] {
        let mut rest = source;
        while let Some(pos) = rest.find(call) {
            let after_call = &rest[pos + call.len()..];
            if let Some(quote_start) = after_call.find('"') {
                let after_quote = &after_call[quote_start + 1..];
                if let Some(quote_end) = after_quote.find('"') {
                    paths.push(after_quote[..quote_end].to_string());
                }
            }
            rest = after_call;
        }
    }
    paths
}

#[test]
fn every_audio_path_a_demo_script_references_exists_on_disk() {
    let _ = std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));

    let mut checked = 0;
    for demo_dir in ["demos/roguelike/scripts", "demos/shooter/scripts"] {
        let Ok(entries) = std::fs::read_dir(demo_dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rhai") {
                continue;
            }
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
            // Step 9-6: project-relative — resolved as play mode resolves
            // it (`play/outcome.rs::flush_audio`), against a level in the
            // demo's own folder (the scripts dir's parent).
            let project = Path::new(demo_dir).parent().unwrap_or(Path::new("."));
            let level = project.join("any.level").to_string_lossy().into_owned();
            for audio_path in audio_paths_referenced(&source) {
                checked += 1;
                let full = ember2d::play::resolve_exit_path(&audio_path, &level, &|q| {
                    Path::new(q).exists()
                });
                assert!(
                    Path::new(&full).exists(),
                    "{}: references audio path '{}', which does not exist",
                    path.display(),
                    audio_path
                );
            }
        }
    }
    assert!(
        checked > 0,
        "no play_sound/play_music call found in any demo script — this test would pass vacuously"
    );
}

#[test]
fn audio_paths_referenced_finds_every_call_shape() {
    let source = r#"
        ctx.play_sound("a/b.ogg");
        ctx.play_sound_at("c/d.ogg", 1.0, 2.0);
        ctx.play_music("e/f.ogg");
    "#;
    // Grouped by call type (the outer loop scans one call name across the
    // whole source before moving to the next), not by where each call
    // appears in the file — irrelevant to the real regression test above,
    // which only checks each found path exists, never their order.
    let mut paths = audio_paths_referenced(source);
    paths.sort();
    assert_eq!(paths, vec!["a/b.ogg", "c/d.ogg", "e/f.ogg"]);
}
