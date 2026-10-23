//! Undo / redo of show edits, plus the `handle` wrapper that records history around the command match.
//!
//! * Only commands that change the *show document* are recorded (see [`history_key`]); runtime-only
//!   commands (GO, faders, programmer values, blind, …) never create undo steps.
//! * A snapshot of the whole `Show` is taken before the command runs and kept only if the command
//!   succeeded and actually changed the show (`rev` moved).
//! * Rapid repeated "save"-style edits of the same object (dragging, debounced editor saves) coalesce
//!   into one step when they arrive within [`COALESCE_MS`] of each other.

use super::*;

pub const MAX_STEPS: usize = 100;
pub const COALESCE_MS: u64 = 800;

#[derive(Default)]
pub struct History {
    undo: Vec<Show>,
    redo: Vec<Show>,
    last: Option<(String, Instant)>,
}

impl History {
    pub fn counts(&self) -> (u16, u16) {
        (self.undo.len() as u16, self.redo.len() as u16)
    }

    /// Record `before` as an undo step (unless coalesced with the previous edit of the same key).
    pub fn push(&mut self, before: Show, key: &str, coalesce: bool) {
        let now = Instant::now();
        let merged = coalesce
            && self
                .last
                .as_ref()
                .map_or(false, |(k, t)| k == key && now.duration_since(*t) < Duration::from_millis(COALESCE_MS))
            && !self.undo.is_empty();
        self.last = Some((key.to_string(), now));
        self.redo.clear();
        if merged {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > MAX_STEPS {
            self.undo.remove(0);
        }
    }
}

/// `(key, coalesce)` for commands that edit the show document; `None` for everything else.
pub fn history_key(cmd: &Command) -> Option<(String, bool)> {
    use Command::*;
    let (name, id, coalesce): (&str, u64, bool) = match cmd {
        FixtureAdd { .. } => ("fixture_add", 0, false),
        FixtureUpdate { fixture } => ("fixture_update", fixture.id as u64, true),
        FixtureRemove { .. } => ("fixture_remove", 0, false),
        FixtureMorph { .. } => ("fixture_morph", 0, false),
        LibraryImportOfl { .. } => ("library_import", 0, false),
        LibraryRemove { .. } => ("library_remove", 0, false),
        GroupSave { group } => ("group_save", group.id as u64, true),
        GroupRemove { .. } => ("group_remove", 0, false),
        PaletteSave { palette } => ("palette_save", palette.id as u64, true),
        PaletteRemove { .. } => ("palette_remove", 0, false),
        ProgRecordPalette { .. } => ("record_palette", 0, false),
        EffectSave { effect } => ("effect_save", effect.id as u64, true),
        EffectRemove { .. } => ("effect_remove", 0, false),
        CuelistSave { cuelist } => ("cuelist_save", cuelist.id as u64, true),
        CuelistRemove { .. } => ("cuelist_remove", 0, false),
        CueSave { cuelist, cue } => ("cue_save", (*cuelist as u64) << 20 ^ (cue.number * 1000.0) as u64, true),
        CueRemove { .. } => ("cue_remove", 0, false),
        CueUpdate { .. } => ("cue_update", 0, false),
        ProgRecordCue { .. } => ("record_cue", 0, false),
        ExecSave { executor } => ("exec_save", executor.id as u64, true),
        ExecRemove { .. } => ("exec_remove", 0, false),
        UniverseSave { universe } => ("universe_save", universe.id as u64, true),
        UniverseRemove { .. } => ("universe_remove", 0, false),
        SettingsSave { .. } => ("settings_save", 0, true),
        BindingSave { binding } => ("binding_save", binding.id as u64, true),
        BindingRemove { .. } => ("binding_remove", 0, false),
        StageSave { .. } => ("stage_save", 0, true),
        VcSave { .. } => ("vc_save", 0, true),
        SceneRecord { .. } => ("scene_record", 0, false),
        SceneSave { scene } => ("scene_save", scene.id as u64, true),
        SceneRemove { .. } => ("scene_remove", 0, false),
        ShowRename { .. } => ("show_rename", 0, true),
        ChaseSave { chase } => ("chase_save", chase.id as u64, true),
        ChaseRemove { .. } => ("chase_remove", 0, false),
        SpeedSave { master } => ("speed_save", master.id as u64, true),
        SpeedRemove { .. } => ("speed_remove", 0, false),
        MacroSave { r#macro } => ("macro_save", r#macro.id as u64, true),
        MacroRemove { .. } => ("macro_remove", 0, false),
        TimelineSave { timeline } => ("timeline_save", timeline.id as u64, true),
        TimelineRemove { .. } => ("timeline_remove", 0, false),
        _ => return None,
    };
    Some((format!("{name}:{id}"), coalesce))
}

impl Engine {
    /// Execute a command, recording an undo step for show edits. `Undo`/`Redo` bypass the recorder.
    pub(super) fn handle(&mut self, cmd: Command) -> Result<(), String> {
        // Commands whose bookkeeping needs state from *before* the command ran.
        if let Command::CuelistManual { cuelist, progress } = cmd {
            return self.cuelist_manual(cuelist, progress);
        }
        let pre = self.pre_command(&cmd);
        let Some((key, coalesce)) = history_key(&cmd) else {
            let r = self.handle_inner(cmd);
            if r.is_ok() {
                self.post_command(pre);
            }
            return r;
        };
        let snap = self.show.clone();
        let rev0 = self.rev;
        let r = self.handle_inner(cmd);
        if r.is_ok() {
            self.post_command(pre);
            if self.rev != rev0 {
                self.prog.hist.push(snap, &key, coalesce);
            }
        }
        r
    }

    pub(super) fn undo(&mut self) -> Result<(), String> {
        let prev = self.prog.hist.undo.pop().ok_or("Nothing to undo")?;
        let cur = std::mem::replace(&mut self.show, prev);
        self.prog.hist.redo.push(cur);
        self.prog.hist.last = None;
        self.after_history_move();
        Ok(())
    }

    pub(super) fn redo(&mut self) -> Result<(), String> {
        let next = self.prog.hist.redo.pop().ok_or("Nothing to redo")?;
        let cur = std::mem::replace(&mut self.show, next);
        self.prog.hist.undo.push(cur);
        self.prog.hist.last = None;
        self.after_history_move();
        Ok(())
    }

    /// Make runtime state consistent with a show that was swapped by undo/redo.
    fn after_history_move(&mut self) {
        self.hz = self.show.settings.tick_hz.clamp(10, 100);
        self.library_dirty = true;
        self.io_dirty = true;
        let show = &self.show;
        self.runs.retain(|id, r| {
            show.cuelists.iter().find(|c| c.id == *id).map_or(false, |cl| r.current.map_or(true, |n| index_of(cl, n).is_some()))
        });
        self.cl_levels.retain(|id, _| show.cuelists.iter().any(|c| c.id == *id));
        self.exec.retain(|id, _| show.executors.iter().any(|e| e.id == *id));
        self.effects_on.retain(|id| show.effects.iter().any(|e| e.id == *id));
        self.effect_flash.retain(|id| show.effects.iter().any(|e| e.id == *id));
        self.scenes.runs.retain(|id, _| show.scenes.iter().any(|s| s.id == *id));
        self.scenes.flash.retain(|id| show.scenes.iter().any(|s| s.id == *id));
        if let Some((a, b, _)) = self.scenes.morph {
            if !show.scenes.iter().any(|s| s.id == a) || !show.scenes.iter().any(|s| s.id == b) {
                self.scenes.morph = None;
            }
        }
        self.prog.links.retain(|f, _| show.fixtures.iter().any(|x| x.id == *f));
        self.refresh_runs();
        self.touch_patch();
    }
}
