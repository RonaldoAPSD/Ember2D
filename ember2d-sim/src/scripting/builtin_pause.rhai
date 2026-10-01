// builtin_pause.rhai — the engine's own Esc pause menu, as a scene.
//
// Step 9-1 (docs/ember2d-master-plan.md §5.8): this replaced the Rust-only
// `PauseMenuState`. Play mode pushes it as scene "pause" when Esc is
// pressed and no scene is open; a project overrides it by shipping its own
// `scenes/pause.rhai`. It pauses the world (the default for any scene).
//
// Step 9-3: the list is an engine menu (`menu_open`) — the engine moves the
// highlight, confirms on Enter/Space and cancels on Escape; this script only
// reacts once the menu closes. Its id lives in this scene entity's var
// "menu". Popping the scene closes the menu with it.

fn items(ctx) {
    let list = ["Resume"];
    if ctx.is_editor_preview() { list.push("Back to Editor"); }
    list.push("Quit Game");
    list
}

fn on_start(id, ctx) {
    let m = ctx.menu_open(items(ctx), #{ title: "PAUSED" });
    ctx.set_var(id, "menu", m);
}

fn on_update(id, ctx) {
    let m = ctx.get_var(id, "menu");
    if m == () || !ctx.menu_closed(m) { return; }
    let sel = ctx.menu_selection(m);
    if sel < 0 { ctx.pop_scene(); return; }
    let choice = items(ctx)[sel];
    if choice == "Resume" { ctx.pop_scene(); }
    if choice == "Back to Editor" { ctx.return_to_editor(); }
    if choice == "Quit Game" { ctx.quit_game(); }
}
