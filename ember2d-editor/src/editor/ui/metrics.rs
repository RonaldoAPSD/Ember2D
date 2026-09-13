// editor/ui/metrics.rs — ChromeMetrics: the theme-derived layout constants
// that replace every `CELL_W`/`CELL_H`-based chrome constant (7D-3,
// docs/ember2d-master-plan.md §5.4). Panels, bars, and dock tabs no longer
// size themselves off the engine's fixed 8×16 glyph cell — they size off
// the active theme's own `Metrics` (`row_h`/`border`/`padding`), so a
// theme with a taller row height gets a taller title/status/menu bar too.
//
// All values here are in POINTS (7D-3's own new coordinate space,
// `ember2d::renderer::UiSpace`) — during this step's own "S pinned to R"
// era (every checkpoint before the live UI-scale menu lands) a point is
// numerically identical to a logical pixel, so nothing here needs its own
// scale-conversion logic; `UiPainter`/`UiSpace` are what apply `ui_scale`
// when it's finally live.
//
// `hier_w`/`insp_w`/`pal_w`/`con_h`/`edit_h` are NOT theme-derived — they're
// starting sizes for a floating/newly-shown panel, same numeric values the
// old cell-based `HIER_W`/`INSP_W`/`PAL_W`/`CON_H`/`EDIT_H` constants
// produced (`N cells * CELL_W/H`), just expressed directly in points now.
// `min_w`/`min_h`/`undock_max_*`/`dock_threshold_*` are the same "carried
// from N cells" reasoning — a floor/ceiling on panel size, not a chrome row
// count, so they stay fixed point values rather than scaling with the
// theme's own row height.

use ember2d::theme::Theme;
use ember2d_sim::math::Rect;

pub struct ChromeMetrics {
    /// Title bar, menu bar, status bar, and dock-tab-strip height.
    pub bar_h: f32,
    /// Generic content row height (list rows, form fields).
    pub row_h: f32,
    /// 9-slice border width and content inset.
    pub border: f32,
    pub padding: f32,
    /// The title bar's own close button — square, `bar_h` on a side.
    pub close_w: f32,
    /// The resize grip — square, `2 * border` on a side (R74, §3 in the
    /// master plan: the old fixed 8px grip with a 6+6px border overlapped
    /// its own corners; sizing it to exactly twice the border means the
    /// grip is ALL corner, no stretched middle, and never overlaps).
    pub grip: f32,
    pub min_w: f32,
    pub min_h: f32,
    pub undock_max_w: f32,
    pub undock_max_h: f32,
    pub dock_threshold_x: f32,
    pub dock_threshold_y: f32,
    pub hier_w: f32,
    pub insp_w: f32,
    pub pal_w: f32,
    pub con_h: f32,
    pub edit_h: f32,
}

impl ChromeMetrics {
    pub fn from_theme(theme: &Theme) -> Self {
        let bar_h = theme.metrics.row_h;
        let border = theme.metrics.border;
        ChromeMetrics {
            bar_h,
            row_h: theme.metrics.row_h,
            border,
            padding: theme.metrics.padding,
            close_w: bar_h,
            grip: border * 2.0,
            min_w: 80.0,
            min_h: 64.0,
            undock_max_w: 320.0,
            undock_max_h: 320.0,
            dock_threshold_x: 24.0,
            dock_threshold_y: 48.0,
            hier_w: 112.0,
            insp_w: 240.0,
            pal_w: 192.0,
            con_h: 144.0,
            edit_h: 192.0,
        }
    }

    /// Where the canvas/docked-panel area starts, vertically — below the
    /// title bar and the menu bar (`2 * bar_h`, was `2 * CELL_H`).
    pub fn chrome_top(&self) -> f32 {
        2.0 * self.bar_h
    }

    /// Where the canvas/docked-panel area ends, vertically — above the
    /// status bar (`screen_h - bar_h`, was `screen_h - CELL_H`).
    pub fn chrome_bottom(&self, screen_h: f32) -> f32 {
        screen_h - self.bar_h
    }

    /// The fullscreen script editor's own content area — one `bar_h`-tall
    /// title bar reserved above it, one status bar below (`impl_render.rs`'s
    /// `render_script_mode`). Shared with `input/script_editor.rs`'s own
    /// fullscreen click/scroll handling (7D-3 checkpoint 5, §3 R67) so the
    /// two can never independently recompute — and disagree on — where the
    /// text area actually starts and ends.
    pub fn script_fullscreen_rect(&self, screen_w: f32, screen_h: f32) -> Rect {
        Rect::new(0.0, self.bar_h, screen_w, (screen_h - 2.0 * self.bar_h).max(0.0))
    }
}
