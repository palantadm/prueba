//! Virtual console: widget execution (press / value / xy), physical input mapping, live state and
//! MIDI/OSC feedback. Every widget drives the same engine code paths as the equivalent command.

use super::bindings::*;
use super::{clamp01, Engine, ExecState};
use crate::model::*;

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Runtime-only virtual console state (never persisted).
pub struct VcRt {
    /// Last value seen per (widget, input index) — edge detection for button inputs.
    input_last: HashMap<(u32, usize), f32>,
    /// Last continuous value written through a widget (0..1), used for edge detection on
    /// slider-driven trigger actions and for widgets without a natural target state.
    values: HashMap<u32, f32>,
    /// Latched state for toggle widgets with no natural target state (None, Page, Palette…).
    toggled: HashMap<u32, bool>,
    xy: HashMap<u32, (f32, f32)>,
    /// Runtime override of effect rates in Hz (the show document is not modified).
    pub effect_rate: HashMap<u32, f32>,
    /// Saved cuelist level while a Cuelist/Flash widget is held.
    saved_level: HashMap<u32, f32>,
    /// Last feedback value sent per (widget, feedback index): `u32::MAX` = never.
    fb_last: HashMap<(u32, usize), u32>,
    fb_refresh_at: Instant,
    force_refresh: bool,
}

impl Default for VcRt {
    fn default() -> Self {
        VcRt {
            input_last: HashMap::new(),
            values: HashMap::new(),
            toggled: HashMap::new(),
            xy: HashMap::new(),
            effect_rate: HashMap::new(),
            saved_level: HashMap::new(),
            fb_last: HashMap::new(),
            fb_refresh_at: Instant::now(),
            force_refresh: true,
        }
    }
}

fn finite_or(v: f32, d: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        d
    }
}

fn is_continuous(kind: VcKind) -> bool {
    matches!(kind, VcKind::Slider | VcKind::Knob | VcKind::SpeedDial)
}

impl Engine {
    // ───────────── lookup / mapping ─────────────

    fn vc_widget(&self, id: u32) -> Result<VcWidget, String> {
        self.show
            .vconsole
            .iter()
            .flat_map(|p| p.widgets.iter())
            .find(|w| w.id == id)
            .cloned()
            .ok_or_else(|| format!("Widget {id} does not exist"))
    }

    fn vc_map_out(w: &VcWidget, v01: f32) -> f32 {
        w.out_min + clamp01(v01) * (w.out_max - w.out_min)
    }

    fn vc_norm_out(w: &VcWidget, mapped: f32) -> f32 {
        let span = w.out_max - w.out_min;
        if span.abs() < 1e-9 {
            0.0
        } else {
            clamp01((mapped - w.out_min) / span)
        }
    }

    fn exec_is_active(&self, id: u32) -> bool {
        let Some(e) = self.show.executors.iter().find(|e| e.id == id) else { return false };
        let st = self.exec.get(&id).copied().unwrap_or(ExecState { level: 1.0, flash: false });
        match &e.kind {
            ExecKind::Cuelist { cuelist } => self.runs.get(cuelist).map_or(false, |r| r.state != PlayState::Idle),
            ExecKind::Effect { effect } => self.effects_on.contains(effect) || self.effect_flash.contains(effect),
            ExecKind::Group { .. } => st.level > 0.0,
            ExecKind::Master { .. } => true,
            ExecKind::Scene { scene } => self.scenes.is_active(*scene),
            ExecKind::XFade { cuelist } => self.manual_active(*cuelist),
            ExecKind::Chase { .. } | ExecKind::Speed { .. } => false, // be-play
            ExecKind::Empty => false,
        }
    }

    fn cuelist_running(&self, id: u32) -> bool {
        self.runs.get(&id).map_or(false, |r| r.state != PlayState::Idle)
    }

    fn group_fixtures(&self, group: u32) -> Result<Vec<u32>, String> {
        self.show
            .groups
            .iter()
            .find(|g| g.id == group)
            .map(|g| g.fixtures.clone())
            .ok_or_else(|| format!("Group {group} does not exist"))
    }

    fn prog_set_attr(&mut self, fixtures: Vec<u32>, attr: &str, v: f32) -> Result<(), String> {
        let mut a = Attrs::new();
        a.insert(attr.to_string(), clamp01(v));
        self.handle(Command::ProgSet { fixtures, attrs: a })
    }

    fn prog_release_attr(&mut self, fixtures: Vec<u32>, attr: &str) -> Result<(), String> {
        self.handle(Command::ProgRelease { fixtures: Some(fixtures), attrs: Some(vec![attr.to_string()]) })
    }

    // ───────────── live state ─────────────

    /// Current (value 0..1, active) of one widget, read from the real target.
    fn vc_state_of(&self, w: &VcWidget) -> (f32, bool) {
        let cont = is_continuous(w.kind);
        let (mut value, active) = match &w.action {
            VcAction::Scene { scene, op } => match op {
                VcOp::Level => {
                    let l = self.scenes.effective(*scene);
                    (Self::vc_norm_out(w, l), self.scenes.is_active(*scene))
                }
                VcOp::Flash => {
                    let f = self.scenes.flash.contains(scene);
                    (f as u8 as f32, f)
                }
                _ => {
                    let a = self.scenes.is_active(*scene);
                    (a as u8 as f32, a)
                }
            },
            VcAction::SceneMorph { a, b } => match self.scenes.morph {
                Some((ma, mb, t)) if ma == *a && mb == *b => (Self::vc_norm_out(w, t), true),
                _ => (self.vc.values.get(&w.id).copied().unwrap_or(0.0), false),
            },
            VcAction::Exec { exec, op } => match op {
                VcOp::Level => {
                    let l = match self.show.executors.iter().find(|e| e.id == *exec).map(|e| &e.kind) {
                        Some(ExecKind::Master { .. }) => self.grand_master,
                        _ => self.exec_level(*exec),
                    };
                    (Self::vc_norm_out(w, l), l > 0.0)
                }
                VcOp::Flash => {
                    let f = self.exec.get(exec).map_or(false, |s| s.flash);
                    (f as u8 as f32, f)
                }
                _ => {
                    let a = self.exec_is_active(*exec);
                    (a as u8 as f32, a)
                }
            },
            VcAction::Cuelist { cuelist, op } => match op {
                VcOp::Level => {
                    let l = self.cl_levels.get(cuelist).copied().unwrap_or(1.0);
                    (Self::vc_norm_out(w, l), l > 0.0)
                }
                _ => {
                    let a = self.cuelist_running(*cuelist);
                    (a as u8 as f32, a)
                }
            },
            VcAction::GrandMaster => (Self::vc_norm_out(w, self.grand_master), self.grand_master > 0.0),
            VcAction::Blackout => (self.blackout as u8 as f32, self.blackout),
            VcAction::Dmx { universe, channel } => {
                let v = match self.dmx_override.get(&(*universe, *channel)) {
                    Some(b) => *b,
                    None => self.frames.get(universe).map_or(0, |f| f[(*channel as usize).clamp(1, 512) - 1]),
                };
                (Self::vc_norm_out(w, v as f32 / 255.0), self.dmx_override.contains_key(&(*universe, *channel)))
            }
            VcAction::FixtureAttr { fixtures, attr } => {
                let held = fixtures.iter().find_map(|f| self.programmer.get(f).and_then(|m| m.get(attr)).copied());
                let v = held
                    .or_else(|| {
                        fixtures.iter().find_map(|f| self.last_merged.get(f).and_then(|m| m.get(attr)).copied())
                    })
                    .unwrap_or_else(|| attr_default(attr));
                (Self::vc_norm_out(w, v), held.map_or(false, |h| h > 0.5))
            }
            VcAction::GroupAttr { group, attr } => {
                let fx = self.group_fixtures(*group).unwrap_or_default();
                let held = fx.iter().find_map(|f| self.programmer.get(f).and_then(|m| m.get(attr)).copied());
                let v = held
                    .or_else(|| fx.iter().find_map(|f| self.last_merged.get(f).and_then(|m| m.get(attr)).copied()))
                    .unwrap_or_else(|| attr_default(attr));
                (Self::vc_norm_out(w, v), held.map_or(false, |h| h > 0.5))
            }
            VcAction::EffectToggle { effect } => {
                let a = self.effects_on.contains(effect) || self.effect_flash.contains(effect);
                (a as u8 as f32, a)
            }
            VcAction::EffectRate { effect } => {
                let rate = self
                    .vc
                    .effect_rate
                    .get(effect)
                    .copied()
                    .or_else(|| self.show.effects.iter().find(|e| e.id == *effect).map(|e| e.rate))
                    .unwrap_or(0.0);
                (Self::vc_norm_out(w, rate), self.effects_on.contains(effect))
            }
            VcAction::PaletteApply { .. } | VcAction::Page { .. } | VcAction::None => {
                let t = self.vc.toggled.get(&w.id).copied().unwrap_or(false);
                (self.vc.values.get(&w.id).copied().unwrap_or(t as u8 as f32), t)
            }
            VcAction::ProgClear => {
                let a = !self.programmer.is_empty();
                (a as u8 as f32, a)
            }
            VcAction::Xy { .. } => {
                let (x, _) = self.vc.xy.get(&w.id).copied().unwrap_or((0.5, 0.5));
                (x, self.vc.xy.contains_key(&w.id))
            }
        };
        if !cont && !matches!(w.action, VcAction::Xy { .. }) {
            value = active as u8 as f32;
        }
        (clamp01(finite_or(value, 0.0)), active)
    }

    pub(super) fn vc_states(&self) -> Vec<VcState> {
        self.show
            .vconsole
            .iter()
            .flat_map(|p| p.widgets.iter())
            .filter(|w| !matches!(w.kind, VcKind::Label | VcKind::Frame | VcKind::Clock))
            .map(|w| {
                let (value, active) = self.vc_state_of(w);
                VcState { id: w.id, value, active }
            })
            .collect()
    }

    // ───────────── commands ─────────────

    pub(super) fn vc_save(&mut self, mut pages: Vec<VcPage>) -> Result<(), String> {
        let mut page_ids = std::collections::BTreeSet::new();
        let mut wid_seen = std::collections::BTreeSet::new();
        let max_w = pages.iter().flat_map(|p| p.widgets.iter().map(|w| w.id)).max().unwrap_or(0);
        let mut next_w = max_w + 1;
        let max_p = pages.iter().map(|p| p.id).max().unwrap_or(0);
        let mut next_p = max_p + 1;
        for p in &mut pages {
            if p.id == 0 || !page_ids.insert(p.id) {
                p.id = next_p;
                next_p += 1;
                page_ids.insert(p.id);
            }
            p.cols = p.cols.clamp(1, 64);
            p.rows = p.rows.clamp(1, 64);
            if p.name.trim().is_empty() {
                p.name = format!("Page {}", p.id);
            }
            for w in &mut p.widgets {
                if w.id == 0 || !wid_seen.insert(w.id) {
                    w.id = next_w;
                    next_w += 1;
                    wid_seen.insert(w.id);
                }
                w.x = w.x.min(p.cols - 1);
                w.y = w.y.min(p.rows - 1);
                w.w = w.w.clamp(1, p.cols - w.x);
                w.h = w.h.clamp(1, p.rows - w.y);
                w.out_min = finite_or(w.out_min, 0.0);
                w.out_max = finite_or(w.out_max, 1.0);
                if w.orientation.is_empty() {
                    w.orientation = "vertical".into();
                }
                for i in &mut w.inputs {
                    i.in_min = finite_or(i.in_min, 0.0).clamp(0.0, 1.0);
                    i.in_max = finite_or(i.in_max, 1.0).clamp(0.0, 1.0);
                }
            }
        }
        self.show.vconsole = pages;
        let alive: std::collections::BTreeSet<u32> = wid_seen;
        self.vc.input_last.retain(|(w, _), _| alive.contains(w));
        self.vc.values.retain(|w, _| alive.contains(w));
        self.vc.toggled.retain(|w, _| alive.contains(w));
        self.vc.xy.retain(|w, _| alive.contains(w));
        self.vc.fb_last.clear();
        self.vc.force_refresh = true;
        self.touch();
        Ok(())
    }

    /// Button-like interaction. `down=false` is the release.
    pub(super) fn vc_press(&mut self, id: u32, down: bool) -> Result<(), String> {
        let w = self.vc_widget(id)?;
        if matches!(w.kind, VcKind::Label | VcKind::Frame | VcKind::Clock) {
            return Ok(());
        }
        if matches!(w.action, VcAction::Xy { .. }) {
            return Ok(());
        }
        match w.mode {
            VcMode::Momentary => self.vc_apply_state(&w, down, true),
            VcMode::Toggle => {
                if !down {
                    return Ok(());
                }
                let (_, cur) = self.vc_state_of(&w);
                self.vc_apply_state(&w, !cur, false)
            }
            VcMode::Trigger => {
                if !down {
                    return Ok(());
                }
                self.vc_trigger(&w)
            }
        }
    }

    /// Set the widget "on"/"off" (momentary and toggle modes).
    fn vc_apply_state(&mut self, w: &VcWidget, on: bool, momentary: bool) -> Result<(), String> {
        self.dirty = true;
        match &w.action {
            VcAction::Scene { scene, op } => match op {
                VcOp::Flash => self.scene_set_flash(*scene, on),
                VcOp::Level => self.scene_level(*scene, if on { w.out_max } else { w.out_min }),
                _ => {
                    // momentary: held = on, release = off; toggle state: follow `on`
                    if momentary || !matches!(op, VcOp::Back | VcOp::Release) {
                        self.scene_go(*scene, on)
                    } else {
                        self.scene_go(*scene, false)
                    }
                }
            },
            VcAction::SceneMorph { a, b } => {
                let t = if on { w.out_max } else { w.out_min };
                self.scene_morph(*a, *b, Some(t))
            }
            VcAction::Exec { exec, op } => match op {
                VcOp::Flash => self.set_flash(*exec, on),
                VcOp::Level => {
                    let l = if on { w.out_max } else { w.out_min };
                    self.set_exec_level(*exec, l)
                }
                VcOp::Go | VcOp::Back | VcOp::Release | VcOp::Toggle => {
                    if momentary && !on {
                        return Ok(());
                    }
                    self.vc_trigger(w)
                }
            },
            VcAction::Cuelist { cuelist, op } => match op {
                VcOp::Flash => {
                    if on {
                        let prev = self.cl_levels.get(cuelist).copied().unwrap_or(1.0);
                        self.vc.saved_level.entry(*cuelist).or_insert(prev);
                        self.cl_levels.insert(*cuelist, clamp01(w.out_max));
                    } else if let Some(l) = self.vc.saved_level.remove(cuelist) {
                        self.cl_levels.insert(*cuelist, l);
                    }
                    Ok(())
                }
                VcOp::Level => {
                    self.cl_levels.insert(*cuelist, clamp01(if on { w.out_max } else { w.out_min }));
                    Ok(())
                }
                _ => {
                    if momentary && !on {
                        return Ok(());
                    }
                    self.vc_trigger(w)
                }
            },
            VcAction::GrandMaster => {
                self.grand_master = clamp01(if on { w.out_max } else { w.out_min });
                Ok(())
            }
            VcAction::Blackout => {
                self.blackout = on;
                Ok(())
            }
            VcAction::Dmx { universe, channel } => {
                let value = if on { Some((clamp01(w.out_max) * 255.0).round() as u8) } else { None };
                self.handle(Command::DmxSet { universe: *universe, channel: *channel, value })
            }
            VcAction::FixtureAttr { fixtures, attr } => {
                if on {
                    self.prog_set_attr(fixtures.clone(), attr, w.out_max)
                } else {
                    self.prog_release_attr(fixtures.clone(), attr)
                }
            }
            VcAction::GroupAttr { group, attr } => {
                let fx = self.group_fixtures(*group)?;
                if on {
                    self.prog_set_attr(fx, attr, w.out_max)
                } else {
                    self.prog_release_attr(fx, attr)
                }
            }
            VcAction::EffectToggle { effect } => {
                if !self.show.effects.iter().any(|e| e.id == *effect) {
                    return Err(format!("Effect {effect} does not exist"));
                }
                if on {
                    self.effects_on.insert(*effect);
                } else {
                    self.effects_on.remove(effect);
                }
                Ok(())
            }
            VcAction::EffectRate { effect } => {
                if !self.show.effects.iter().any(|e| e.id == *effect) {
                    return Err(format!("Effect {effect} does not exist"));
                }
                if on {
                    self.effects_on.insert(*effect);
                } else {
                    self.effects_on.remove(effect);
                }
                Ok(())
            }
            VcAction::PaletteApply { .. } | VcAction::ProgClear => {
                if on {
                    self.vc_trigger(w)?;
                }
                self.vc.toggled.insert(w.id, on);
                Ok(())
            }
            VcAction::Page { .. } | VcAction::None => {
                self.vc.toggled.insert(w.id, on);
                Ok(())
            }
            VcAction::Xy { .. } => Ok(()),
        }
    }

    /// One-shot firing of the widget's action (trigger mode, or the "fire" half of other modes).
    fn vc_trigger(&mut self, w: &VcWidget) -> Result<(), String> {
        self.dirty = true;
        match &w.action {
            VcAction::Scene { scene, op } => match op {
                VcOp::Go => self.scene_go(*scene, true),
                VcOp::Back | VcOp::Release => self.scene_go(*scene, false),
                VcOp::Toggle => self.scene_toggle(*scene),
                VcOp::Level => self.scene_level(*scene, w.out_max),
                VcOp::Flash => Ok(()),
            },
            VcAction::SceneMorph { a, b } => {
                let t = match self.scenes.morph {
                    Some((ma, mb, t)) if ma == *a && mb == *b && t >= 0.5 => w.out_min,
                    _ => w.out_max,
                };
                self.scene_morph(*a, *b, Some(t))
            }
            VcAction::Exec { exec, op } => match op {
                VcOp::Go => self.exec_go(*exec),
                VcOp::Back => self.exec_back(*exec),
                VcOp::Toggle => match self.exec_kind(*exec)? {
                    ExecKind::Cuelist { cuelist } => {
                        if self.cuelist_running(cuelist) {
                            self.release_cuelist(cuelist);
                            Ok(())
                        } else {
                            self.go(cuelist)
                        }
                    }
                    _ => self.exec_go(*exec),
                },
                VcOp::Release => {
                    if let ExecKind::Cuelist { cuelist } = self.exec_kind(*exec)? {
                        self.release_cuelist(cuelist);
                    }
                    Ok(())
                }
                VcOp::Level => self.set_exec_level(*exec, w.out_max),
                VcOp::Flash => {
                    self.set_flash(*exec, true)?;
                    self.set_flash(*exec, false)
                }
            },
            VcAction::Cuelist { cuelist, op } => match op {
                VcOp::Go => self.go(*cuelist),
                VcOp::Back => self.back(*cuelist),
                VcOp::Toggle => {
                    if self.cuelist_running(*cuelist) {
                        self.release_cuelist(*cuelist);
                        Ok(())
                    } else {
                        self.go(*cuelist)
                    }
                }
                VcOp::Release => {
                    self.release_cuelist(*cuelist);
                    Ok(())
                }
                VcOp::Level => {
                    self.cl_levels.insert(*cuelist, clamp01(w.out_max));
                    Ok(())
                }
                VcOp::Flash => Ok(()),
            },
            VcAction::GrandMaster => {
                self.grand_master = clamp01(w.out_max);
                Ok(())
            }
            VcAction::Blackout => {
                self.blackout = !self.blackout;
                Ok(())
            }
            VcAction::EffectToggle { effect } | VcAction::EffectRate { effect } => {
                if !self.show.effects.iter().any(|e| e.id == *effect) {
                    return Err(format!("Effect {effect} does not exist"));
                }
                if !self.effects_on.remove(effect) {
                    self.effects_on.insert(*effect);
                }
                Ok(())
            }
            VcAction::PaletteApply { palette, group } => {
                self.exec_action(Action::PaletteApply { palette: *palette, group: *group }, 1.0, true)
            }
            VcAction::ProgClear => self.exec_action(Action::ProgClear, 1.0, true),
            VcAction::Dmx { .. } | VcAction::FixtureAttr { .. } | VcAction::GroupAttr { .. } => {
                // A trigger on a value target "pulses" it to out_max and releases on the next press.
                let (_, cur) = self.vc_state_of(w);
                self.vc_apply_state(w, !cur, false)
            }
            VcAction::Page { .. } | VcAction::None => {
                let t = self.vc.toggled.get(&w.id).copied().unwrap_or(false);
                self.vc.toggled.insert(w.id, !t);
                Ok(())
            }
            VcAction::Xy { .. } => Ok(()),
        }
    }

    /// Continuous value 0..1 for slider/knob/dial widgets (or any widget driven by an analogue input).
    pub(super) fn vc_set_value(&mut self, id: u32, v01: f32) -> Result<(), String> {
        let w = self.vc_widget(id)?;
        let v01 = clamp01(finite_or(v01, 0.0));
        self.dirty = true;
        let prev = self.vc.values.insert(id, v01);
        let mapped = Self::vc_map_out(&w, v01);
        match &w.action {
            VcAction::Exec { exec, op: VcOp::Level } => self.set_exec_level(*exec, mapped),
            VcAction::Cuelist { cuelist, op: VcOp::Level } => {
                self.cl_levels.insert(*cuelist, clamp01(mapped));
                Ok(())
            }
            VcAction::GrandMaster => {
                self.grand_master = clamp01(mapped);
                Ok(())
            }
            VcAction::Dmx { universe, channel } => {
                let b = (clamp01(mapped) * 255.0).round() as u8;
                self.handle(Command::DmxSet { universe: *universe, channel: *channel, value: Some(b) })
            }
            VcAction::FixtureAttr { fixtures, attr } => self.prog_set_attr(fixtures.clone(), attr, mapped),
            VcAction::GroupAttr { group, attr } => {
                let fx = self.group_fixtures(*group)?;
                self.prog_set_attr(fx, attr, mapped)
            }
            VcAction::EffectRate { effect } => {
                if !self.show.effects.iter().any(|e| e.id == *effect) {
                    return Err(format!("Effect {effect} does not exist"));
                }
                self.vc.effect_rate.insert(*effect, mapped.max(0.0));
                Ok(())
            }
            VcAction::Scene { scene, op: VcOp::Level } => self.scene_level(*scene, clamp01(mapped)),
            VcAction::SceneMorph { a, b } => self.scene_morph(*a, *b, Some(mapped)),
            VcAction::Xy { .. } => Ok(()),
            _ => {
                // Trigger-type action driven by an analogue value: press on the rising edge through 0.5,
                // release on the falling edge.
                let was_on = prev.map_or(false, |p| p >= 0.5);
                let now_on = v01 >= 0.5;
                if now_on != was_on {
                    return self.vc_press(id, now_on);
                }
                Ok(())
            }
        }
    }

    pub(super) fn vc_xy(&mut self, id: u32, x: f32, y: f32) -> Result<(), String> {
        let w = self.vc_widget(id)?;
        let VcAction::Xy { group, x_attr, y_attr } = &w.action else {
            return Err(format!("Widget {id} is not an XY pad"));
        };
        let (x, y) = (clamp01(finite_or(x, 0.5)), clamp01(finite_or(y, 0.5)));
        self.vc.xy.insert(id, (x, y));
        let fx = self.group_fixtures(*group)?;
        let mut a = Attrs::new();
        a.insert(x_attr.clone(), clamp01(Self::vc_map_out(&w, x)));
        a.insert(y_attr.clone(), clamp01(Self::vc_map_out(&w, y)));
        self.dirty = true;
        self.handle(Command::ProgSet { fixtures: fx, attrs: a })
    }

    // ───────────── physical inputs ─────────────

    /// Route a MIDI/OSC event to every widget input that matches it.
    pub(super) fn vc_input(&mut self, ev: &InEvent) {
        let raw = ev.value();
        let is_program = matches!(ev, InEvent::Midi { msg, .. } if msg.kind == MidiKind::Program);
        let is_note = matches!(ev, InEvent::Midi { msg, .. } if msg.kind == MidiKind::Note);
        let mut hits: Vec<(u32, usize, VcKind, f32)> = Vec::new();
        for w in self.show.vconsole.iter().flat_map(|p| p.widgets.iter()) {
            for (i, inp) in w.inputs.iter().enumerate() {
                if source_matches(&inp.source, ev) {
                    let span = (inp.in_max - inp.in_min).abs().max(1e-6);
                    // Notes are binary for buttons: any velocity > 0 is a press (soft pad hits included).
                    let r = if is_note && matches!(w.kind, VcKind::Button | VcKind::CueList) {
                        if raw > 0.0 {
                            1.0
                        } else {
                            0.0
                        }
                    } else {
                        raw
                    };
                    let mut n = clamp01((r - inp.in_min.min(inp.in_max)) / span);
                    if inp.invert {
                        n = 1.0 - n;
                    }
                    hits.push((w.id, i, w.kind, n));
                }
            }
        }
        for (wid, idx, kind, n) in hits {
            let r = match kind {
                VcKind::Button | VcKind::CueList => {
                    if is_program {
                        self.vc_press(wid, true).and_then(|_| self.vc_press(wid, false))
                    } else {
                        let prev = self.vc.input_last.insert((wid, idx), n);
                        let rising = n >= 0.5 && (is_note || prev.map_or(true, |p| p < 0.5));
                        let falling = n < 0.5 && prev.map_or(false, |p| p >= 0.5);
                        if rising {
                            self.vc_press(wid, true)
                        } else if falling {
                            self.vc_press(wid, false)
                        } else {
                            Ok(())
                        }
                    }
                }
                VcKind::Slider | VcKind::Knob | VcKind::SpeedDial => {
                    self.vc_echo_guard(wid, n);
                    self.vc_set_value(wid, n)
                }
                VcKind::XyPad => {
                    let cur = self.vc.xy.get(&wid).copied().unwrap_or((0.5, 0.5));
                    match idx {
                        0 => self.vc_xy(wid, n, cur.1),
                        1 => self.vc_xy(wid, cur.0, n),
                        _ => Ok(()),
                    }
                }
                _ => Ok(()),
            };
            if let Err(e) = r {
                tracing::debug!("vc input widget {wid}: {e}");
            }
        }
    }

    /// A value that arrived from the controller itself must not be echoed back (motor-fader fight).
    fn vc_echo_guard(&mut self, wid: u32, n: f32) {
        let Ok(w) = self.vc_widget(wid) else { return };
        for (fi, fb) in w.feedback.iter().enumerate() {
            if let FeedbackTarget::Midi { scale: true, .. } = fb {
                self.vc.fb_last.insert((wid, fi), (n * 127.0).round() as u32);
            }
        }
    }

    // ───────────── feedback ─────────────

    pub(super) fn vc_request_refresh(&mut self) {
        self.vc.force_refresh = true;
    }

    /// Send MIDI/OSC feedback for widgets whose state changed (all of them on refresh).
    pub(super) fn vc_feedback(&mut self, states: &[VcState]) {
        let refresh = self.vc.force_refresh || self.vc.fb_refresh_at.elapsed() >= Duration::from_secs(1);
        if refresh {
            self.vc.fb_refresh_at = Instant::now();
            self.vc.force_refresh = false;
        }
        let by_id: HashMap<u32, &VcState> = states.iter().map(|s| (s.id, s)).collect();
        let mut midi: Vec<(Option<String>, Vec<u8>)> = Vec::new();
        let mut osc: Vec<(String, u16, String, f32)> = Vec::new();
        for w in self.show.vconsole.iter().flat_map(|p| p.widgets.iter()) {
            if w.feedback.is_empty() {
                continue;
            }
            let Some(st) = by_id.get(&w.id) else { continue };
            let cont = is_continuous(w.kind) || matches!(w.kind, VcKind::XyPad);
            for (fi, fb) in w.feedback.iter().enumerate() {
                let key = (w.id, fi);
                match fb {
                    FeedbackTarget::Midi { port, kind, channel, number, on, off, scale } => {
                        let byte = if cont && *scale {
                            (st.value * 127.0).round() as u8
                        } else if st.active {
                            *on
                        } else {
                            *off
                        }
                        .min(127);
                        let sig = byte as u32;
                        if !refresh && self.vc.fb_last.get(&key) == Some(&sig) {
                            continue;
                        }
                        // Program change carries no on/off: only fire when turning on.
                        if *kind == MidiKind::Program && !st.active {
                            self.vc.fb_last.insert(key, sig);
                            continue;
                        }
                        self.vc.fb_last.insert(key, sig);
                        let ch = channel.clamp(&1, &16) - 1;
                        let num = (*number).min(127);
                        let bytes = match kind {
                            MidiKind::Note => vec![0x90 | ch, num, byte],
                            MidiKind::Cc => vec![0xB0 | ch, num, byte],
                            MidiKind::Program => vec![0xC0 | ch, num],
                        };
                        midi.push((port.clone(), bytes));
                    }
                    FeedbackTarget::Osc { host, port, address, on, off } => {
                        let v = if cont { st.value } else if st.active { *on } else { *off };
                        let sig = v.to_bits();
                        if !refresh && self.vc.fb_last.get(&key) == Some(&sig) {
                            continue;
                        }
                        self.vc.fb_last.insert(key, sig);
                        osc.push((host.clone(), *port, address.clone(), v));
                    }
                }
            }
        }
        for (port, bytes) in midi {
            // Native outputs take it first; browsers only relay what no native port handled.
            if !self.midi.send(port.as_deref(), &bytes) {
                self.emit(&ServerMsg::MidiOut { port, bytes });
            }
        }
        for (host, port, addr, v) in osc {
            self.io.osc_send(&host, port, &addr, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;
    use tokio::sync::broadcast;

    fn btn(id: u32, action: VcAction, mode: VcMode) -> VcWidget {
        VcWidget {
            id,
            kind: VcKind::Button,
            label: format!("b{id}"),
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            color: "#fff".into(),
            action,
            mode,
            orientation: String::new(),
            inputs: vec![],
            feedback: vec![],
            out_min: 0.0,
            out_max: 1.0,
        }
    }

    fn slider(id: u32, action: VcAction) -> VcWidget {
        let mut w = btn(id, action, VcMode::Trigger);
        w.kind = VcKind::Slider;
        w
    }

    fn page(widgets: Vec<VcWidget>) -> VcPage {
        VcPage { id: 1, name: "P".into(), cols: 8, rows: 6, widgets }
    }

    fn midi_src(kind: MidiKind, ch: u8, n: u8) -> Source {
        Source::Midi { port: None, kind, channel: Some(ch), number: n }
    }

    #[test]
    fn save_assigns_ids_and_clamps() {
        let mut e = engine();
        let mut a = btn(0, VcAction::None, VcMode::Toggle);
        a.x = 99;
        a.w = 99;
        let b = btn(0, VcAction::None, VcMode::Toggle);
        let mut p = page(vec![a, b]);
        p.id = 0;
        e.handle(Command::VcSave { pages: vec![p] }).unwrap();
        let ws = &e.show.vconsole[0].widgets;
        assert_ne!(ws[0].id, 0);
        assert_ne!(ws[0].id, ws[1].id);
        assert!(ws[0].x + ws[0].w <= 8);
        assert_ne!(e.show.vconsole[0].id, 0);
    }

    #[test]
    fn button_modes_drive_blackout_and_dmx() {
        let mut e = engine();
        e.handle(Command::VcSave {
            pages: vec![page(vec![
                btn(1, VcAction::Blackout, VcMode::Toggle),
                btn(2, VcAction::Dmx { universe: 1, channel: 5 }, VcMode::Momentary),
            ])],
        })
        .unwrap();
        e.handle(Command::VcPress { widget: 1, down: true }).unwrap();
        e.handle(Command::VcPress { widget: 1, down: false }).unwrap();
        assert!(e.blackout);
        e.handle(Command::VcPress { widget: 1, down: true }).unwrap();
        assert!(!e.blackout);

        e.handle(Command::VcPress { widget: 2, down: true }).unwrap();
        assert_eq!(frame(&mut e)[4], 255);
        e.handle(Command::VcPress { widget: 2, down: false }).unwrap();
        assert_eq!(frame(&mut e)[4], 0);
    }

    #[test]
    fn slider_levels_and_attrs() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        let mut gm = slider(1, VcAction::GrandMaster);
        gm.out_min = 0.0;
        let dim = slider(2, VcAction::FixtureAttr { fixtures: vec![1], attr: "dimmer".into() });
        e.handle(Command::VcSave { pages: vec![page(vec![gm, dim])] }).unwrap();
        e.handle(Command::VcValue { widget: 2, value: 1.0 }).unwrap();
        assert_eq!(frame(&mut e)[0], 255);
        e.handle(Command::VcValue { widget: 1, value: 0.5 }).unwrap();
        assert_eq!(frame(&mut e)[0], 128);
        let st = e.vc_states();
        let s1 = st.iter().find(|s| s.id == 1).unwrap();
        assert!((s1.value - 0.5).abs() < 1e-3);
        // build_rt exposes widget states
        assert_eq!(e.build_rt().vc.len(), 2);
    }

    #[test]
    fn effect_rate_is_runtime_only() {
        let mut e = engine();
        e.handle(Command::EffectSave {
            effect: Effect { attr2: None, speed_master: None, bpm: 0.0, rotation: 0.0,
                id: 0,
                name: "e".into(),
                attr: "dimmer".into(),
                shape: Shape::Sine,
                rate: 1.0,
                depth: 1.0,
                phase_spread: 0.0,
                fixtures: vec![],
            },
        })
        .unwrap();
        let mut w = slider(1, VcAction::EffectRate { effect: 1 });
        w.out_min = 0.5;
        w.out_max = 4.5;
        e.handle(Command::VcSave { pages: vec![page(vec![w])] }).unwrap();
        e.handle(Command::VcValue { widget: 1, value: 0.5 }).unwrap();
        assert!((e.vc.effect_rate[&1] - 2.5).abs() < 1e-4);
        assert!((e.show.effects[0].rate - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cuelist_go_and_toggle_state() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: "A".into(), release: true, refs: true })
            .unwrap();
        let go = btn(1, VcAction::Cuelist { cuelist: 1, op: VcOp::Go }, VcMode::Trigger);
        let tg = btn(2, VcAction::Cuelist { cuelist: 1, op: VcOp::Toggle }, VcMode::Trigger);
        e.handle(Command::VcSave { pages: vec![page(vec![go, tg])] }).unwrap();
        e.handle(Command::VcPress { widget: 1, down: true }).unwrap();
        let st = e.vc_states();
        assert!(st.iter().find(|s| s.id == 2).unwrap().active);
        e.handle(Command::VcPress { widget: 2, down: true }).unwrap();
        assert!(!e.vc_states().iter().find(|s| s.id == 2).unwrap().active);
    }

    #[test]
    fn midi_in_drives_widgets_and_led_feedback_round_trip() {
        let mut e = engine();
        let (tx, mut rx) = broadcast::channel(64);
        e.out = tx;
        let mut b = btn(1, VcAction::Blackout, VcMode::Toggle);
        b.inputs = vec![VcInput { source: midi_src(MidiKind::Note, 1, 36), invert: false, in_min: 0.0, in_max: 1.0 }];
        b.feedback = vec![FeedbackTarget::Midi {
            port: Some("APC".into()),
            kind: MidiKind::Note,
            channel: 1,
            number: 36,
            on: 127,
            off: 0,
            scale: false,
        }];
        let mut s = slider(2, VcAction::GrandMaster);
        s.inputs = vec![VcInput { source: midi_src(MidiKind::Cc, 1, 7), invert: false, in_min: 0.0, in_max: 1.0 }];
        s.feedback = vec![FeedbackTarget::Midi {
            port: None,
            kind: MidiKind::Cc,
            channel: 1,
            number: 7,
            on: 127,
            off: 0,
            scale: true,
        }];
        e.handle(Command::VcSave { pages: vec![page(vec![b, s])] }).unwrap();
        drain_midi(&mut rx);

        // pad press (note on) toggles blackout, release (velocity 0) is ignored by toggle mode
        e.handle(Command::MidiIn { port: "APC".into(), bytes: vec![0x90, 36, 100] }).unwrap();
        e.handle(Command::MidiIn { port: "APC".into(), bytes: vec![0x90, 36, 0] }).unwrap();
        assert!(e.blackout);
        let st = e.vc_states();
        e.vc_feedback(&st);
        let out = drain_midi(&mut rx);
        assert!(out.contains(&(Some("APC".to_string()), vec![0x90, 36, 127])), "LED on: {out:?}");

        // soft hit without note-off still counts as a press (toggles back and forth)
        e.handle(Command::MidiIn { port: "APC".into(), bytes: vec![0x90, 36, 10] }).unwrap();
        assert!(!e.blackout);
        e.handle(Command::MidiIn { port: "APC".into(), bytes: vec![0x90, 36, 10] }).unwrap();
        assert!(e.blackout);
        let st = e.vc_states();
        e.vc_feedback(&st);
        drain_midi(&mut rx);

        // second feedback pass without changes sends nothing
        let st = e.vc_states();
        e.vc_feedback(&st);
        assert!(drain_midi(&mut rx).is_empty());

        // fader CC moves GM, and is not echoed back to the controller
        e.handle(Command::MidiIn { port: "APC".into(), bytes: vec![0xB0, 7, 64] }).unwrap();
        assert!((e.grand_master - 64.0 / 127.0).abs() < 1e-4);
        let st = e.vc_states();
        e.vc_feedback(&st);
        assert!(drain_midi(&mut rx).is_empty(), "no echo for controller-originated value");

        // GM changed from the UI → motor fader / LED ring feedback
        e.handle(Command::GrandMaster { level: 1.0 }).unwrap();
        let st = e.vc_states();
        e.vc_feedback(&st);
        let out = drain_midi(&mut rx);
        assert!(out.contains(&(None, vec![0xB0, 7, 127])), "{out:?}");

        // pad pressed again turns blackout and LED off
        e.handle(Command::MidiIn { port: "APC".into(), bytes: vec![0x90, 36, 100] }).unwrap();
        assert!(!e.blackout);
        let st = e.vc_states();
        e.vc_feedback(&st);
        let out = drain_midi(&mut rx);
        assert!(out.contains(&(Some("APC".to_string()), vec![0x90, 36, 0])), "{out:?}");
    }

    #[test]
    fn learn_does_not_trigger_widgets() {
        let mut e = engine();
        let mut b = btn(1, VcAction::Blackout, VcMode::Toggle);
        b.inputs = vec![VcInput { source: midi_src(MidiKind::Note, 1, 36), invert: false, in_min: 0.0, in_max: 1.0 }];
        e.handle(Command::VcSave { pages: vec![page(vec![b])] }).unwrap();
        e.handle(Command::LearnStart { binding: None }).unwrap();
        e.handle(Command::MidiIn { port: "x".into(), bytes: vec![0x90, 36, 100] }).unwrap();
        assert!(!e.blackout);
    }

    #[test]
    fn osc_input_drives_slider_and_xy() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 2);
        e.handle(Command::GroupSave { group: Group { id: 0, name: "g".into(), fixtures: vec![1, 2] } }).unwrap();
        let mut s = slider(1, VcAction::GrandMaster);
        s.inputs = vec![VcInput { source: Source::Osc { address: "/gm".into() }, invert: true, in_min: 0.0, in_max: 1.0 }];
        let mut xy = btn(2, VcAction::Xy { group: 1, x_attr: "pan".into(), y_attr: "tilt".into() }, VcMode::Trigger);
        xy.kind = VcKind::XyPad;
        xy.inputs = vec![
            VcInput { source: Source::Osc { address: "/x".into() }, invert: false, in_min: 0.0, in_max: 1.0 },
            VcInput { source: Source::Osc { address: "/y".into() }, invert: false, in_min: 0.0, in_max: 1.0 },
        ];
        e.handle(Command::VcSave { pages: vec![page(vec![s, xy])] }).unwrap();
        e.on_io(crate::io::IoEvent::Osc { addr: "/gm".into(), value: Some(0.25) });
        assert!((e.grand_master - 0.75).abs() < 1e-4);
        e.on_io(crate::io::IoEvent::Osc { addr: "/x".into(), value: Some(1.0) });
        e.on_io(crate::io::IoEvent::Osc { addr: "/y".into(), value: Some(0.0) });
        let f = frame(&mut e);
        assert_eq!(f[0], 255); // pan MSB fixture 1
        assert_eq!(f[2], 0); // tilt MSB
        // direct XY command too
        e.handle(Command::VcXy { widget: 2, x: 0.0, y: 1.0 }).unwrap();
        let f = frame(&mut e);
        assert_eq!((f[0], f[2]), (0, 255));
    }

    #[test]
    fn vc_save_json_numeric_keys_regression() {
        let j = r##"{"type":"vc_save","pages":[{"id":1,"name":"P","cols":8,"rows":6,"widgets":[
          {"id":7,"kind":"button","label":"GO","x":0,"y":0,"w":2,"h":2,"color":"#35c46a",
           "action":{"type":"cuelist","cuelist":1,"op":"go"},"mode":"trigger","orientation":"",
           "inputs":[{"source":{"type":"midi","port":null,"kind":"note","channel":1,"number":36},"invert":false,"in_min":0,"in_max":1}],
           "feedback":[{"type":"midi","port":null,"kind":"note","channel":1,"number":36,"on":127,"off":0,"scale":false},
                       {"type":"osc","host":"127.0.0.1","port":9000,"address":"/go","on":1,"off":0}],
           "out_min":0,"out_max":1}]}]}"##;
        let cmd: Command = serde_json::from_str(j).unwrap();
        assert!(matches!(cmd, Command::VcSave { .. }));
        let mut e = engine();
        e.handle(cmd).unwrap();
        assert_eq!(e.show.vconsole[0].widgets[0].id, 7);
    }
}
