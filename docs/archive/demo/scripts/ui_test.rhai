fn on_update(me, ctx) {
    ctx.draw_panel(10, 5, 40, 15, "UI DEMO", "White", "DarkBlue");
    
    let options = ["Action 1", "Action 2", "Action 3", "Exit Menu"];
    let selected = ctx.get_global("menu_sel");
    if selected == () { selected = 0; }
    
    ctx.draw_menu(12, 8, 36, options, selected, "White", "Reset", "Black", "Cyan");
    
    if ctx.just_pressed("up") {
        selected = (selected + options.len() - 1) % options.len();
        ctx.set_global("menu_sel", selected);
    }
    if ctx.just_pressed("down") {
        selected = (selected + 1) % options.len();
        ctx.set_global("menu_sel", selected);
    }
    
    if ctx.just_pressed("enter") {
        ctx.log("Selected: " + options[selected]);
    }
}
