// builtin_pause.rhai — the engine's own Esc pause menu, as a scene.
//
// Step 9-1 (docs/ember2d-master-plan.md §5.8): this replaced the Rust-only
// `PauseMenuState`. Play mode pushes it as scene "pause" when Esc is
// pressed and no scene is open; a project overrides it by shipping its own
// `scenes/pause.rhai`. It pauses the world (the default for any scene).
//
// The highlighted row lives in this scene entity's own var "sel".

fn items(ctx) {
    let list = ["Resume"];
    if ctx.is_editor_preview() { list.push("Back to Editor"); }
    list.push("Quit Game");
    list
}

fn on_start(id, ctx) {
    ctx.set_var(id, "sel", 0);
}

fn on_input(id, ctx) {
    let list = items(ctx);
    let n = list.len();
    let sel = ctx.get_var(id, "sel");
    if sel == () { sel = 0; }
    if ctx.just_pressed("escape") { ctx.pop_scene(); return; }
    if ctx.just_pressed("up") || ctx.just_pressed("w") { sel = (sel + n - 1) % n; }
    if ctx.just_pressed("down") || ctx.just_pressed("s") { sel = (sel + 1) % n; }
    if ctx.just_pressed("enter") || ctx.just_pressed("space") {
        let choice = list[sel];
        if choice == "Resume" { ctx.pop_scene(); }
        if choice == "Back to Editor" { ctx.return_to_editor(); }
        if choice == "Quit Game" { ctx.quit_game(); }
    }
    ctx.set_var(id, "sel", sel);
}

fn on_update(id, ctx) {
    let list = items(ctx);
    let sel = ctx.get_var(id, "sel");
    if sel == () { sel = 0; }
    // Centred, clamped to the screen: never negative on a tiny window
    // (R86, carried over from the Rust menu this replaced).
    let w = 30;
    let h = list.len() + 4;
    let x = (ctx.get_viewport_width() - w) / 2;
    let y = (ctx.get_viewport_height() - h) / 2;
    if x < 0 { x = 0; }
    if y < 0 { y = 0; }
    ctx.draw_panel(x, y, w, h, " PAUSED ", "White", "DarkBlue");
    ctx.draw_menu(x + 2, y + 2, w - 4, list, sel, "Grey", "DarkBlue", "Yellow", "DarkGrey");
}
