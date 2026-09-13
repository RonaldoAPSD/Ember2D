// editor/impl_render/modes.rs — Full-screen mode rendering (graph editor,
// fullscreen script editor). Extracted from the single `impl_render.rs`
// (7D-3 checkpoint 5, docs/ember2d-master-plan.md §5.4) purely to keep
// `impl_render/mod.rs` under CLAUDE.md's 750-line hard limit — no
// behavioral change from the split.

use ember2d::renderer::color::Color;
use ember2d::renderer::DrawSurface;

use super::super::graph_ui;
use super::super::ui;
use super::super::EditorState;

// ── Graph editor rendering ────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn render_graph_mode(
        &mut self,
        renderer: &mut dyn DrawSurface,
        mouse: &ember2d::mouse::MouseState,
        gx: i32,
        gy: i32,
    ) {
        let sw = renderer.width();
        let sh = renderer.height();

        // Resolve graph reference
        let graph = match self.grid.get(gx, gy, self.active_layer).and_then(|t| t.graph.as_ref()) {
            Some(g) => g.clone(),
            None => {
                renderer.draw_str(0, 0, "No graph", Color::Red, Color::Black);
                return;
            }
        };

        graph_ui::draw_graph(
            renderer,
            self.font.as_mut(),
            &self.theme,
            &graph,
            self.graph_selected_node,
            self.graph_connecting,
            mouse.cell_x,
            mouse.cell_y,
            self.graph_view_ox,
            self.graph_view_oy,
            sw,
            sh,
        );

        // Title bar (row 0)
        let tag =
            self.grid.get(gx, gy, self.active_layer).map(|t| t.tag.clone()).unwrap_or_default();
        let title = format!(
            " GRAPH — {} ({},{})   Esc=back  F=layout  RClick=add  Del=remove",
            if tag.is_empty() { "(tile)" } else { &tag },
            gx,
            gy
        );
        let title: String = format!("{:<width$}", title, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, 0, &title, Color::White, Color::DarkBlue);

        // Status bar (row 1 — help / connection hint)
        let status = if self.graph_editing_param.is_some() {
            let buf = self.graph_editing_param.as_ref().map(|(_, b)| b.as_str()).unwrap_or("");
            format!(" Editing param: {}█", buf)
        } else if self.graph_connecting.is_some() {
            " Drawing wire — click an input port to connect, Esc to cancel".into()
        } else {
            " LClick=select/drag  RClick=add node  Click port>wire  F=auto-layout".into()
        };
        let status: String = format!("{:<width$}", status, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, 1, &status, Color::Black, Color::DarkGrey);

        // Palette overlay
        if let Some((px, py)) = self.graph_palette_open {
            graph_ui::draw_palette(
                renderer,
                &self.theme,
                self.graph_palette_scroll,
                self.graph_palette_cursor,
                px,
                py,
                sw,
                sh,
                &mut self.ui_frame,
            );
        }

        // Inline param edit overlay (show buffer in status bar — already done above)
    }
}

impl EditorState {
    /// Points-based, themed fullscreen chrome (7D-3 checkpoint 5, master
    /// plan §5.4) — was raw `renderer.draw_str` at literal `Color::Cyan`/
    /// `Color::DarkBlue`, entirely untethered from the active theme (the
    /// one surface in this editor that stayed that way through every
    /// earlier theming step). `bar_h` matches every other chrome bar
    /// (`metrics.bar_h`, itself `theme.metrics.row_h`), so the fullscreen
    /// script editor's own title/status bars are the same height as the
    /// docked editor's title/menu/status bars, not an independent size.
    pub(super) fn render_script_mode(&mut self, renderer: &mut dyn DrawSurface, metrics: &ui::ChromeMetrics) {
        use ember2d::theme::PaletteRole;
        use ember2d_sim::math::{Rect, Vec2};

        let sw = renderer.pixel_width() as f32;
        let sh = renderer.pixel_height() as f32;
        let bar_h = metrics.bar_h;
        let text_px = self.theme.font_sizes.body;
        let accent = self.theme.role_color(PaletteRole::Accent);
        let panel_bg = self.theme.role_color(PaletteRole::PanelBg);
        let text_fg = self.theme.role_color(PaletteRole::TextPrimary);

        // Title bar
        let title = match &self.script_path {
            Some(p) => format!(
                " SCRIPT EDITOR — {}{}   Esc=back  Ctrl+S=save",
                p,
                if self.script_unsaved { "*" } else { "" }
            ),
            None => " SCRIPT EDITOR — (no file) ".to_string(),
        };
        let title_rect = Rect::new(0.0, 0.0, sw, bar_h);
        renderer.fill_rect_px(title_rect, accent);
        renderer.set_scissor(Some(title_rect));
        let title_baseline = self.font.ascent(text_px);
        renderer.draw_text_px(self.font.as_mut(), &title, Vec2::new(0.0, title_baseline), text_px, Color::Black);
        renderer.set_scissor(None);

        // Editor area — `script_error`'s message is copied to an owned
        // `String` (not `self.script_error()`'s own borrowed `&str`) so it
        // doesn't keep `self` borrowed across the `self.code_font.as_mut()`
        // mutable borrow this same call also needs.
        let script_error = self.script_error().map(|(line, msg)| (line, msg.to_string()));
        let script_selection = self.script_selection();
        ui::draw_script_editor(
            renderer,
            self.code_font.as_mut(),
            &self.theme,
            self.script_path.as_deref(),
            &self.script_buffer,
            self.script_cursor,
            self.script_scroll,
            self.script_hscroll,
            self.script_unsaved,
            metrics.script_fullscreen_rect(sw, sh),
            script_error.as_ref().map(|(line, msg)| (*line, msg.as_str())),
            script_selection,
            self.script_find_active.then_some(self.script_find_query.as_str()),
        );

        // Status bar
        let status =
            format!(" Line: {:<4} Col: {:<4} ", self.script_cursor.1 + 1, self.script_cursor.0 + 1);
        let status_rect = Rect::new(0.0, sh - bar_h, sw, bar_h);
        renderer.fill_rect_px(status_rect, panel_bg);
        renderer.set_scissor(Some(status_rect));
        let status_baseline = status_rect.y + self.font.ascent(text_px);
        renderer.draw_text_px(self.font.as_mut(), &status, Vec2::new(0.0, status_baseline), text_px, text_fg);
        renderer.set_scissor(None);
    }
}
