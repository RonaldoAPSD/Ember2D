// editor/project_settings.rs — File > Project Settings...: the project's
// own `project.ron`, editable in the editor.
//
// Step 9-6 (docs/ember2d-master-plan.md §5.8). Before it, `project.ron`
// was written once, by the New Project wizard, and every later change
// (turn-based or real time, the start level, the world cell 9-5 added)
// meant editing the file by hand. The dialog lists the play settings as
// rows; a choice row cycles on click, a value row opens a prompt, and every
// change is saved at once. The editor applies what it uses itself (the
// name in the title, the world cell for the canvas) immediately; play mode
// re-reads the file at every F5 (`ember2d-app`'s `run_editor_app`), so a
// change shows the next time the game runs.

use ember2d::project::{GameplayLoop, ProjectData};
use ember2d_sim::scheduler::TurnModel;
use ember2d_sim::scripting::LogEntry;

use super::{EditorMode, EditorState, TextInputPurpose};

/// One row of the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectField {
    Name,
    GameplayLoop,
    TurnModel,
    /// Step 9.5-3: `ai_turns_per_step`.
    AiTurns,
    StartLevel,
    WorldCell,
    PixelsPerUnit,
}

impl ProjectField {
    pub const ALL: [ProjectField; 7] = [
        ProjectField::Name,
        ProjectField::GameplayLoop,
        ProjectField::TurnModel,
        ProjectField::AiTurns,
        ProjectField::StartLevel,
        ProjectField::WorldCell,
        ProjectField::PixelsPerUnit,
    ];

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            ProjectField::Name => "Name",
            ProjectField::GameplayLoop => "Gameplay",
            ProjectField::TurnModel => "Turn order",
            ProjectField::AiTurns => "AI turns/step",
            ProjectField::StartLevel => "Start level",
            ProjectField::WorldCell => "World cell (px)",
            ProjectField::PixelsPerUnit => "Pixels per unit",
        }
    }

    /// True for a row that cycles through choices on click, rather than
    /// opening a prompt.
    pub fn cycles(self) -> bool {
        matches!(
            self,
            ProjectField::GameplayLoop | ProjectField::TurnModel | ProjectField::StartLevel
        )
    }

    /// What the prompt for a value row says.
    pub fn prompt(self) -> &'static str {
        match self {
            ProjectField::Name => "Project name",
            ProjectField::WorldCell => "World cell as width,height in pixels (8,16 or 16,16)",
            ProjectField::PixelsPerUnit => "Source pixels per world unit (e.g. 16)",
            ProjectField::AiTurns => "AI turns one step may resolve (1 = one actor per step; 256 = all at once)",
            _ => "Value",
        }
    }
}

/// The value a row shows.
pub fn project_field_value(p: &ProjectData, field: ProjectField) -> String {
    match field {
        ProjectField::Name => p.name.clone(),
        ProjectField::GameplayLoop => match p.gameplay_loop {
            GameplayLoop::RealTime => "Real time".into(),
            GameplayLoop::TurnBased => "Turn-based".into(),
        },
        ProjectField::TurnModel => format!("{:?}", p.turn_model),
        ProjectField::AiTurns => p.ai_turns_per_step.to_string(),
        ProjectField::StartLevel => p.start_level.clone().unwrap_or_else(|| "(none)".into()),
        ProjectField::WorldCell => format!("{} x {}", p.world_cell.0, p.world_cell.1),
        ProjectField::PixelsPerUnit => format!("{}", p.pixels_per_unit),
    }
}

/// The next choice for a cycling row. `levels` are the project's level
/// file names, for the start level.
pub fn cycle_project_field(p: &mut ProjectData, field: ProjectField, levels: &[String]) {
    match field {
        ProjectField::GameplayLoop => {
            p.gameplay_loop = match p.gameplay_loop {
                GameplayLoop::RealTime => GameplayLoop::TurnBased,
                GameplayLoop::TurnBased => GameplayLoop::RealTime,
            }
        }
        ProjectField::TurnModel => {
            p.turn_model = match p.turn_model {
                TurnModel::Alternating => TurnModel::Energy,
                TurnModel::Energy => TurnModel::ActionCost,
                TurnModel::ActionCost => TurnModel::Alternating,
            }
        }
        ProjectField::StartLevel if !levels.is_empty() => {
            let at = p.start_level.as_ref().and_then(|s| levels.iter().position(|l| l == s));
            let next = at.map(|i| (i + 1) % levels.len()).unwrap_or(0);
            p.start_level = Some(levels[next].clone());
        }
        _ => {}
    }
}

/// Applies a typed value; `Err` says what was wrong.
pub fn set_project_field(p: &mut ProjectData, field: ProjectField, text: &str) -> Result<(), String> {
    let t = text.trim();
    match field {
        ProjectField::Name if !t.is_empty() => p.name = t.to_string(),
        ProjectField::Name => return Err("a project needs a name".into()),
        ProjectField::WorldCell => {
            let parts: Vec<u32> = t
                .split([',', 'x', 'X', ' '])
                .filter(|s| !s.trim().is_empty())
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            match parts.as_slice() {
                [w, h] if (1..=1024).contains(w) && (1..=1024).contains(h) => {
                    p.world_cell = (*w, *h)
                }
                _ => return Err(format!("'{t}' isn't a size like 16,16")),
            }
        }
        ProjectField::AiTurns => match t.parse::<u32>() {
            Ok(n) if (1..=1024).contains(&n) => p.ai_turns_per_step = n,
            _ => return Err(format!("'{t}' isn't a whole number from 1 to 1024")),
        },
        ProjectField::PixelsPerUnit => match t.parse::<f32>() {
            Ok(v) if v.is_finite() && v > 0.0 => p.pixels_per_unit = v,
            _ => return Err(format!("'{t}' isn't a positive number")),
        },
        _ => {}
    }
    Ok(())
}

/// What File > New Scene writes into `scenes/<name>.rhai` — a scene that
/// runs as it is (a line of HUD, Escape closes it), commented with where
/// to go next.
pub fn scene_template(name: &str) -> String {
    format!(
        r#"// scenes/{name}.rhai — a scene. Open it from any script with
// ctx.push_scene("{name}"); while it's on top the level is paused, unless
// it was pushed with #{{ pauses_world: false }}. Close it with
// ctx.pop_scene(). See docs/ember2d-scripting-api.md, "Scenes".

fn on_start(id, ctx) {{
    // Runs once, the step after the scene is pushed — open a menu or a
    // dialogue here (ctx.menu_open / ctx.draw_dialogue).
}}

fn on_input(id, ctx) {{
    if ctx.just_pressed("escape") {{
        ctx.pop_scene();
    }}
}}

fn on_update(id, ctx) {{
    ctx.draw_hud(2, 2, "{name}: press Escape to close", "White", "Reset");
}}
"#
    )
}

impl EditorState {
    /// Step 9-6: creates `rel` (a path inside the project, folders
    /// included) holding `contents`, opens it in the script editor and
    /// refreshes Files — what New Script and New Scene share. Never
    /// overwrites; says why when it can't.
    pub(super) fn create_project_file(&mut self, rel: &str, contents: &str) {
        let Some(folder) = self.project_folder.clone() else {
            self.console_log.push(LogEntry::warn("Open a project folder first".to_string()));
            return;
        };
        let path = std::path::Path::new(&folder).join(rel);
        if path.exists() {
            self.console_log.push(LogEntry::error(format!("'{rel}' already exists")));
            return;
        }
        let made = path
            .parent()
            .map(std::fs::create_dir_all)
            .unwrap_or(Ok(()))
            .and_then(|_| std::fs::write(&path, contents));
        match made {
            Ok(()) => {
                self.load_script(rel);
                self.refresh_project_files();
            }
            Err(e) => self.console_log.push(LogEntry::error(format!("Can't create '{rel}': {e}"))),
        }
    }

    /// File > Project Settings...: load the project's `project.ron` (or
    /// start from defaults named after the folder) and show the dialog.
    pub(super) fn open_project_settings(&mut self) {
        let Some(folder) = self.project_folder.clone() else {
            self.console_log
                .push(LogEntry::warn("Project Settings: open a project folder first".to_string()));
            return;
        };
        let project = ProjectData::load(&folder).unwrap_or_else(|_| {
            ProjectData::new(ProjectData::name_for(&folder), GameplayLoop::RealTime)
        });
        self.project_settings = Some(project);
        self.mode = EditorMode::ProjectSettings;
    }

    /// Leaves the dialog (everything was saved as it changed).
    pub(super) fn close_project_settings(&mut self) {
        self.project_settings = None;
        self.mode = EditorMode::Paint(super::ui::ToolKind::Paint);
        self.ignore_drag = true;
    }

    /// A click on a row: cycle it, or open its prompt.
    pub(super) fn project_settings_click(&mut self, field: ProjectField) {
        let Some(project) = self.project_settings.as_mut() else { return };
        if field.cycles() {
            let folder = self.project_folder.clone().unwrap_or_default();
            let levels: Vec<String> = ProjectData::levels_in(&folder)
                .iter()
                .filter_map(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .collect();
            cycle_project_field(project, field, &levels);
            self.save_project_settings();
        } else {
            self.prompt_buffer = project_field_value(project, field);
            if field == ProjectField::WorldCell {
                self.prompt_buffer = format!("{},{}", project.world_cell.0, project.world_cell.1);
            }
            self.mode = EditorMode::Prompt(TextInputPurpose::ProjectSetting(field));
        }
    }

    /// A prompt's value, then back to the dialog.
    pub(super) fn commit_project_setting(&mut self, field: ProjectField, text: &str) {
        self.mode = EditorMode::ProjectSettings;
        let Some(project) = self.project_settings.as_mut() else { return };
        match set_project_field(project, field, text) {
            Ok(()) => self.save_project_settings(),
            Err(why) => self.console_log.push(LogEntry::warn(format!("Project Settings: {why}"))),
        }
    }

    /// Writes `project.ron` and applies what the editor itself uses.
    fn save_project_settings(&mut self) {
        let (Some(project), Some(folder)) = (&self.project_settings, &self.project_folder) else {
            return;
        };
        if let Err(e) = project.save(folder) {
            self.console_log.push(LogEntry::error(format!("Project Settings: save failed: {e}")));
            return;
        }
        self.project_name = Some(project.name.clone());
        self.world_cell = project.play_settings().world_cell_px();
        self.clamp_scroll();
        self.save_message = Some("Project settings saved".to_string());
        self.save_message_timer = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectData {
        ProjectData::new("Demo", GameplayLoop::RealTime)
    }

    #[test]
    fn choice_rows_cycle_and_wrap() {
        let mut p = project();
        cycle_project_field(&mut p, ProjectField::GameplayLoop, &[]);
        assert_eq!(p.gameplay_loop, GameplayLoop::TurnBased);
        for _ in 0..3 {
            cycle_project_field(&mut p, ProjectField::TurnModel, &[]);
        }
        assert_eq!(p.turn_model, TurnModel::Alternating, "three steps go all the way round");
        let levels = vec!["a.level".to_string(), "b.level".to_string()];
        p.start_level = Some("b.level".into());
        cycle_project_field(&mut p, ProjectField::StartLevel, &levels);
        assert_eq!(p.start_level.as_deref(), Some("a.level"));
    }

    #[test]
    fn typed_values_are_checked() {
        let mut p = project();
        set_project_field(&mut p, ProjectField::WorldCell, "16, 16").unwrap();
        assert_eq!(p.world_cell, (16, 16));
        assert!(set_project_field(&mut p, ProjectField::WorldCell, "0,16").is_err());
        set_project_field(&mut p, ProjectField::PixelsPerUnit, "16").unwrap();
        assert_eq!(p.pixels_per_unit, 16.0);
        assert!(set_project_field(&mut p, ProjectField::PixelsPerUnit, "-1").is_err());
        assert!(set_project_field(&mut p, ProjectField::Name, "  ").is_err());
        assert_eq!(project_field_value(&p, ProjectField::WorldCell), "16 x 16");
    }
}
