// tests/replay.rs — Step 5h's replay test, the phase's own "done when"
// (docs/ember2d-phase5-plan.md): a recorded input sequence replayed against
// a fresh simulation with the same seed must produce byte-identical state.
//
// WHAT'S "RECORDED" HERE, AND WHY NOT LITERAL `Command` VALUES: the plan's
// own sketch records `(step_index, Vec<Command>)` and replays those
// directly, bypassing `on_input` entirely. Doing that for real needs a
// production API this phase doesn't have — a way to inject a `Command` for
// a step without going through `on_input`'s own key-to-command translation
// — and building one now would be new, untested surface area added purely
// to serve this test, not something Step 5h itself asks for. `on_input` is
// a pure function of that step's `InputSnapshot` (no wall-clock, no OS
// RNG), and `TurnHarness::frame`/`turn` are themselves fully deterministic
// (no real timing involved anywhere) — so replaying the exact same *key*
// sequence through a fresh `TurnHarness` already exercises the property
// this test needs to prove: given the same seed and the same externally-
// supplied inputs, the simulation (`TurnScheduler`'s ordering, `on_turn`,
// direct-write combat, deferred writes, particle/shake RNG) reproduces
// byte-identical state. A literal command-stream-level replay entry point
// is real future work — worth building alongside Phase 9's netcode, or
// Step 5i's workspace split, since that's when something outside a test
// would actually consume it — not a gap in what this test proves today.
//
// No CI exists to run this in yet — a manual gate. Run it 5x fresh
// (`cargo test --test replay`, independently, not `--test-threads=1` in one
// process) before trusting it, same discipline Phase 4 used for its own
// throwaway harness tests (see docs/HANDOFF.md) — this file's own
// `TurnHarness` instances are already two independent in-process
// PlayState/World/ScriptEngine stacks, but re-running the whole test
// binary catches anything that could only ever vary *between* processes
// (a stray `HashMap` somewhere, OS-entropy leaking in some other way).

mod common;
use common::TurnHarness;
use ember2d::prelude::*;

// `CARGO_MANIFEST_DIR`-relative, not CWD-relative — Step 5i's workspace
// split (docs/ember2d-phase5-plan.md) moved this crate into `ember2d/`,
// one level below `roguelike/`, and `cargo test` (unlike `cargo run`) runs
// each integration test binary with its CWD set to the *package's own*
// directory rather than wherever the test was invoked from. A bare
// `"roguelike/..."` literal would only resolve for `cargo run`; this
// resolves for both.
const FLOOR2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../roguelike/floor2.level");

/// Checkpoint interval, in scripted-session actions — not raw sim steps.
/// `TurnHarness::turn` itself already spans however many follow-up frames
/// an AI-heavy round takes (see that method's own doc comment, Step 5f) —
/// checkpointing per *action* is what a human replaying this by hand would
/// call "the same point in the session," and is what makes a divergence
/// report ("diverged by action N") actually easy to reproduce by hand.
const CHECKPOINT_EVERY: usize = 4;

/// A representative play session — movement in every direction, waiting,
/// quaffing, wall bumps, and (floor2 has rats near these paths) combat —
/// deliberately the same shape as `tests/roguelike_combat.rs`'s own
/// `identical_input_sequences_produce_identical_state_across_independent_instances`
/// so both tests exercise the same load-bearing path; this one additionally
/// proves full-state byte-identity (not just position/hp/gold) with
/// per-checkpoint divergence reporting.
fn scripted_session() -> Vec<Option<Key>> {
    use Key::*;
    vec![
        Some(D), Some(D), Some(D), Some(S), Some(S), Some(A), Some(W),
        Some(Space), Some(D), Some(S), Some(Space), Some(A), Some(A),
        Some(W), Some(W), None, None, Some(Q), Some(D), Some(D),
    ]
}

fn drive(h: &mut TurnHarness, key: Option<Key>) {
    match key {
        Some(k) => { h.turn(k); }
        None => { h.frame(None); }
    }
}

/// RON-serialize everything a replay must reproduce byte-for-byte —
/// `World` + `globals` + `persistent` + `clips`, exactly what the plan
/// asks for, and exactly what `SaveState` already bundles (Step 5c,
/// docs/ember2d-phase5-plan.md's D17 fix) — reusing it here means this
/// test rides the same tested serialization path `save_game`/`load_game`
/// do, rather than hand-rolling a second one that could quietly drift from
/// it. `level_path` is a constant placeholder: it's session metadata, not
/// simulation state, and would be identical between the two runs anyway.
fn snapshot(h: &TurnHarness) -> String {
    let save = SaveState::new(
        h.world.clone(),
        h.persistent.clone(),
        h.play.globals.clone(),
        h.play.clips.clone(),
        "replay-snapshot".to_string(),
    );
    save.to_ron().expect("state must RON-serialize for comparison")
}

#[test]
fn replay_produces_byte_identical_state_across_independent_instances() {
    let session = scripted_session();

    // "Record": drive one instance through the whole session, checkpointing
    // full state along the way.
    let mut h1 = TurnHarness::load(FLOOR2);
    let mut recorded_checkpoints = Vec::new();
    for (i, &key) in session.iter().enumerate() {
        drive(&mut h1, key);
        if (i + 1) % CHECKPOINT_EVERY == 0 { recorded_checkpoints.push(snapshot(&h1)); }
    }
    let recorded_final = snapshot(&h1);

    // "Replay": a completely independent PlayState/World/ScriptEngine
    // stack, loaded from the same level file (same pinned seed —
    // `tests/roguelike_level_integrity.rs` already asserts every roguelike
    // level has one), driven by the identical key sequence.
    let mut h2 = TurnHarness::load(FLOOR2);
    let mut next_checkpoint = 0;
    for (i, &key) in session.iter().enumerate() {
        drive(&mut h2, key);
        if (i + 1) % CHECKPOINT_EVERY == 0 {
            let got = snapshot(&h2);
            assert_eq!(
                got, recorded_checkpoints[next_checkpoint],
                "state diverged by action #{} (the first {} actions matched) — \
                 same seed, same input sequence, independent PlayState/World/ScriptEngine instances",
                i + 1, i + 1 - CHECKPOINT_EVERY
            );
            next_checkpoint += 1;
        }
    }

    assert_eq!(snapshot(&h2), recorded_final, "final state must be byte-identical to the recorded run");
}
