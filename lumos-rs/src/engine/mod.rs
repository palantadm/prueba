//! The engine task: owns all show/runtime state, executes commands, renders DMX at `tick_hz`.

mod bindings;
mod effects;
mod morph;
mod patch;
mod playback;
mod scenes;
mod vconsole;

use crate::io::{IoEvent, IoHub, MtcDecoder};
use crate::library;
use crate::model::*;
use crate::persist::Store;
use bindings::*;
use patch::*;
use playback::*;
use scenes::ScenesState;
use vconsole::VcRt;

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, oneshot};

pub type Reply = oneshot::Sender<Result<(), String>>;

pub enum EngineIn {
    Cmd { cmd: Command, reply: Option<Reply> },
    /// Initial messages for a new client: Hello, Library, Show, Shows, Rt.
    Snapshot { reply: oneshot::Sender<Vec<String>> },
    GetShow { reply: oneshot::Sender<Show> },
    ImportShow { show: Box<Show>, reply: Reply },
    /// Profiles parsed from uploaded GDTF/QLC+/OFL files (assets already on disk).
    ImportProfiles { profiles: Vec<FixtureProfile>, reply: Reply },
    /// Dry-run of a fixture morph (REST `POST /api/morph/preview`).
    MorphPreview { ids: Vec<u32>, profile: String, mode: String, reply: oneshot::Sender<Result<MorphPreview, String>> },
}

#[derive(Clone)]
pub enum Out {
    Json(Arc<str>),
    Dmx(u16, Arc<[u8; 512]>),
}

#[derive(Clone)]
pub struct Handle {
    pub tx: mpsc::UnboundedSender<EngineIn>,
    pub out: broadcast::Sender<Out>,
    pub store: Store,
}

impl Handle {
    pub async fn command(&self, cmd: Command) -> Result<(), String> {
        let (r, rx) = oneshot::channel();
        self.tx.send(EngineIn::Cmd { cmd, reply: Some(r) }).map_err(|_| "Engine stopped".to_string())?;
        rx.await.map_err(|_| "Engine stopped".to_string())?
    }
    pub async fn snapshot(&self) -> Vec<String> {
        let (r, rx) = oneshot::channel();
        if self.tx.send(EngineIn::Snapshot { reply: r }).is_err() {
            return vec![];
        }
        rx.await.unwrap_or_default()
    }
    pub async fn get_show(&self) -> Option<Show> {
        let (r, rx) = oneshot::channel();
        self.tx.send(EngineIn::GetShow { reply: r }).ok()?;
        rx.await.ok()
    }
    pub async fn import_show(&self, show: Show) -> Result<(), String> {
        let (r, rx) = oneshot::channel();
        self.tx
            .send(EngineIn::ImportShow { show: Box::new(show), reply: r })
            .map_err(|_| "Engine stopped".to_string())?;
        rx.await.map_err(|_| "Engine stopped".to_string())?
    }
    pub async fn morph_preview(&self, ids: Vec<u32>, profile: String, mode: String) -> Result<MorphPreview, String> {
        let (r, rx) = oneshot::channel();
        self.tx
            .send(EngineIn::MorphPreview { ids, profile, mode, reply: r })
            .map_err(|_| "Engine stopped".to_string())?;
        rx.await.map_err(|_| "Engine stopped".to_string())?
    }
    pub async fn import_profiles(&self, profiles: Vec<FixtureProfile>) -> Result<(), String> {
        let (r, rx) = oneshot::channel();
        self.tx
            .send(EngineIn::ImportProfiles { profiles, reply: r })
            .map_err(|_| "Engine stopped".to_string())?;
        rx.await.map_err(|_| "Engine stopped".to_string())?
    }
}

pub fn spawn(store: Store) -> Handle {
    let (tx, rx) = mpsc::unbounded_channel();
    let (out, _) = broadcast::channel(512);
    let (io_tx, io_rx) = mpsc::unbounded_channel();
    let midi = crate::midi::MidiHub::start(store.dir.clone(), io_tx.clone());
    let io = IoHub::new(io_tx);
    let mut engine = Engine::new(store.clone(), out.clone(), io);
    engine.midi = midi;
    tokio::spawn(engine.run(rx, io_rx));
    Handle { tx, out, store }
}

#[derive(Clone, Copy)]
struct ExecState {
    level: f32,
    flash: bool,
}

struct Tc {
    running: bool,
    frames_f: f64,
    last_mtc: Option<f64>,
    dec: MtcDecoder,
    prev_disp: u32,
}

struct Engine {
    store: Store,
    t0: Instant,
    out: broadcast::Sender<Out>,
    io: IoHub,
    midi: crate::midi::MidiHub,

    show: Show,
    rev: u32,
    builtin: Vec<FixtureProfile>,
    profiles: HashMap<String, FixtureProfile>,
    patch: Patch,
    tc_cues: Vec<(u32, u32, f64)>,

    programmer: PerFixture,
    grand_master: f32,
    blackout: bool,
    dmx_override: BTreeMap<(u16, u16), u8>,
    runs: HashMap<u32, CueRun>,
    cl_levels: HashMap<u32, f32>,
    exec: HashMap<u32, ExecState>,
    effects_on: BTreeSet<u32>,
    effect_flash: BTreeSet<u32>,
    go_seq: u64,
    last_merged: PerFixture,

    frames: BTreeMap<u16, Box<[u8; 512]>>,
    bcast_pending: BTreeSet<u16>,
    bcast_at: HashMap<u16, Instant>,
    ext_dmx: HashMap<u16, (Instant, Vec<u8>)>,

    dirty: bool,
    patch_dirty: bool,
    io_dirty: bool,
    library_dirty: bool,
    shows_dirty: bool,
    show_pending: bool,
    show_sent: Instant,
    autosave_due: Option<Instant>,

    last_rt: String,
    last_rt_at: Instant,
    rt_check_at: Instant,
    fps: f32,
    frame_count: u32,
    fps_at: Instant,
    hz: u32,
    last_tick: f64,

    bind_last: HashMap<u32, f32>,
    learn: Option<u32>,
    recent: VecDeque<String>,
    tc: Tc,
    osc_fb: HashMap<String, f32>,
    vc: VcRt,
    scenes: ScenesState,
}

fn next_id(it: impl Iterator<Item = u32>) -> u32 {
    it.max().unwrap_or(0) + 1
}

fn clamp01(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn kind_matches(attr: &str, kind: PaletteKind) -> bool {
    let want = match kind {
        PaletteKind::All => return true,
        PaletteKind::Intensity => "intensity",
        PaletteKind::Position => "position",
        PaletteKind::Color => "color",
        PaletteKind::Beam => "beam",
    };
    ATTRS.iter().any(|(n, g, _)| *n == attr && *g == want)
}

impl Engine {
    fn new(store: Store, out: broadcast::Sender<Out>, io: IoHub) -> Self {
        let show = store.load_autosave().unwrap_or_default();
        let now = Instant::now();
        let mut e = Engine {
            store,
            t0: now,
            out,
            io,
            midi: crate::midi::MidiHub::disabled(),
            show,
            rev: 1,
            builtin: library::builtin(),
            profiles: HashMap::new(),
            patch: Patch::default(),
            tc_cues: vec![],
            programmer: PerFixture::new(),
            grand_master: 1.0,
            blackout: false,
            dmx_override: BTreeMap::new(),
            runs: HashMap::new(),
            cl_levels: HashMap::new(),
            exec: HashMap::new(),
            effects_on: BTreeSet::new(),
            effect_flash: BTreeSet::new(),
            go_seq: 0,
            last_merged: PerFixture::new(),
            frames: BTreeMap::new(),
            bcast_pending: BTreeSet::new(),
            bcast_at: HashMap::new(),
            ext_dmx: HashMap::new(),
            dirty: true,
            patch_dirty: true,
            io_dirty: true,
            library_dirty: true,
            shows_dirty: true,
            show_pending: true,
            show_sent: now - Duration::from_secs(1),
            autosave_due: None,
            last_rt: String::new(),
            last_rt_at: now - Duration::from_secs(2),
            rt_check_at: now,
            fps: 0.0,
            frame_count: 0,
            fps_at: now,
            hz: 44,
            last_tick: 0.0,
            bind_last: HashMap::new(),
            learn: None,
            recent: VecDeque::new(),
            tc: Tc { running: false, frames_f: 0.0, last_mtc: None, dec: MtcDecoder::default(), prev_disp: 0 },
            osc_fb: HashMap::new(),
            vc: VcRt::default(),
            scenes: ScenesState::default(),
        };
        e.hz = e.show.settings.tick_hz.clamp(10, 100);
        e.rebuild_derived();
        e
    }

    fn now(&self) -> f64 {
        self.t0.elapsed().as_secs_f64()
    }

    // ───────────── main loop ─────────────

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<EngineIn>, mut io_rx: mpsc::UnboundedReceiver<IoEvent>) {
        let mut hz = self.hz;
        let mut ticker = Self::make_ticker(hz);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    self.tick();
                    if self.hz != hz {
                        hz = self.hz;
                        ticker = Self::make_ticker(hz);
                    }
                }
                m = rx.recv() => match m {
                    Some(m) => self.on_in(m),
                    None => break,
                },
                Some(ev) = io_rx.recv() => self.on_io(ev),
            }
        }
        let _ = self.store.autosave(&self.show);
    }

    fn make_ticker(hz: u32) -> tokio::time::Interval {
        let mut t = tokio::time::interval(Duration::from_secs_f64(1.0 / hz.max(1) as f64));
        t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        t
    }

    fn on_in(&mut self, m: EngineIn) {
        match m {
            EngineIn::Cmd { cmd, reply } => {
                let r = self.handle(cmd);
                match reply {
                    Some(tx) => {
                        let _ = tx.send(r);
                    }
                    None => {
                        if let Err(e) = r {
                            tracing::debug!("command failed: {e}");
                        }
                    }
                }
            }
            EngineIn::Snapshot { reply } => {
                self.vc_request_refresh();
                let mut v = vec![
                    json(&ServerMsg::Hello { version: env!("CARGO_PKG_VERSION").into() }),
                    json(&ServerMsg::Library { profiles: self.library_list() }),
                    json(&ServerMsg::Show { rev: self.rev, show: self.show.clone() }),
                    json(&ServerMsg::Shows { shows: self.store.list() }),
                    self.midi_ports_json(),
                ];
                v.push(json(&ServerMsg::Rt { rt: self.build_rt() }));
                let _ = reply.send(v);
            }
            EngineIn::GetShow { reply } => {
                let _ = reply.send(self.show.clone());
            }
            EngineIn::ImportShow { show, reply } => {
                self.set_show(*show);
                let _ = reply.send(Ok(()));
            }
            EngineIn::MorphPreview { ids, profile, mode, reply } => {
                let _ = reply.send(self.morph_preview(&ids, &profile, &mode));
            }
            EngineIn::ImportProfiles { profiles, reply } => {
                for p in profiles {
                    match self.show.profiles.iter_mut().find(|x| x.id == p.id) {
                        Some(x) => *x = p,
                        None => self.show.profiles.push(p),
                    }
                }
                self.library_dirty = true;
                self.touch_patch();
                let _ = reply.send(Ok(()));
            }
        }
    }

    fn on_io(&mut self, ev: IoEvent) {
        match ev {
            IoEvent::Osc { addr, value } => self.handle_input(InEvent::Osc { addr: &addr, value }),
            IoEvent::Midi { port, bytes } => self.midi_in(&port, &bytes),
            IoEvent::MidiPorts { .. } => {
                let j = self.midi_ports_json();
                let _ = self.out.send(Out::Json(Arc::from(j)));
            }
            IoEvent::ExternalDmx { universe, data } => {
                self.ext_dmx.insert(universe, (Instant::now(), data));
                self.dirty = true;
            }
        }
    }

    // ───────────── helpers ─────────────

    fn midi_ports_json(&self) -> String {
        let v = self.midi.ports();
        json(&ServerMsg::MidiPorts { native: v.native, inputs: v.inputs, outputs: v.outputs })
    }

    fn emit(&self, msg: &ServerMsg) {
        let _ = self.out.send(Out::Json(Arc::from(json(msg))));
    }

    fn log(&self, level: &str, msg: String) {
        self.emit(&ServerMsg::Log { level: level.into(), msg });
    }

    fn library_list(&self) -> Vec<FixtureProfile> {
        let mut v: Vec<FixtureProfile> =
            self.builtin.iter().filter(|b| !self.show.profiles.iter().any(|p| p.id == b.id)).cloned().collect();
        v.extend(self.show.profiles.iter().cloned());
        v
    }

    /// Rebuild profile map, patch table, timecode cue table and DMX frame set.
    fn rebuild_derived(&mut self) {
        self.profiles = self.library_list().into_iter().map(|p| (p.id.clone(), p)).collect();
        self.patch = Patch::build(&self.show, &self.profiles);
        let fps = self.show.settings.timecode.fps;
        self.tc_cues = self
            .show
            .cuelists
            .iter()
            .flat_map(|cl| {
                cl.cues
                    .iter()
                    .filter_map(|c| Some((parse_tc(c.timecode.as_deref()?, fps)?, cl.id, c.number)))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.tc_cues.sort_by_key(|c| c.0);
        let mut unis: BTreeSet<u16> = self.show.universes.iter().map(|u| u.id).collect();
        unis.extend(self.patch.fixtures.iter().map(|f| f.universe));
        self.frames.retain(|u, _| unis.contains(u));
        for u in unis {
            self.frames.entry(u).or_insert_with(|| Box::new([0u8; 512]));
        }
        self.patch_dirty = false;
        self.dirty = true;
    }

    /// Mark the show document changed (broadcast + autosave).
    fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
        self.show_pending = true;
        self.dirty = true;
        if self.autosave_due.is_none() {
            self.autosave_due = Some(Instant::now() + Duration::from_secs(2));
        }
    }

    fn touch_patch(&mut self) {
        self.patch_dirty = true;
        self.touch();
    }

    fn set_show(&mut self, show: Show) {
        self.show = show;
        self.programmer.clear();
        self.runs.clear();
        self.cl_levels.clear();
        self.exec.clear();
        self.effects_on.clear();
        self.effect_flash.clear();
        self.dmx_override.clear();
        self.bind_last.clear();
        self.vc = VcRt::default();
        self.scenes = ScenesState::default();
        self.last_merged.clear();
        self.ext_dmx.clear();
        self.hz = self.show.settings.tick_hz.clamp(10, 100);
        self.library_dirty = true;
        self.io_dirty = true;
        self.touch_patch();
    }

    fn require_cuelist(&self, id: u32) -> Result<&Cuelist, String> {
        self.show.cuelists.iter().find(|c| c.id == id).ok_or_else(|| format!("Cuelist {id} does not exist"))
    }

    fn fixtures_exist(&self, ids: &[u32]) -> impl Iterator<Item = u32> + '_ {
        let ids = ids.to_vec();
        ids.into_iter().filter(|i| self.show.fixtures.iter().any(|f| f.id == *i))
    }

    // ───────────── commands ─────────────

    fn handle(&mut self, cmd: Command) -> Result<(), String> {
        use Command::*;
        match cmd {
            ProgSet { fixtures, attrs } => {
                let ids: Vec<u32> = self.fixtures_exist(&fixtures).collect();
                for f in ids {
                    let e = self.programmer.entry(f).or_default();
                    for (a, v) in &attrs {
                        if v.is_finite() {
                            e.insert(a.clone(), clamp01(*v));
                        }
                    }
                }
                self.dirty = true;
            }
            ProgRelease { fixtures, attrs } => {
                match (&fixtures, &attrs) {
                    (None, None) => self.programmer.clear(),
                    _ => {
                        let targets: Vec<u32> = fixtures.clone().unwrap_or_else(|| self.programmer.keys().copied().collect());
                        for f in targets {
                            match &attrs {
                                None => {
                                    self.programmer.remove(&f);
                                }
                                Some(list) => {
                                    if let Some(m) = self.programmer.get_mut(&f) {
                                        for a in list {
                                            m.remove(a);
                                        }
                                    }
                                }
                            }
                        }
                        self.programmer.retain(|_, m| !m.is_empty());
                    }
                }
                self.dirty = true;
            }
            ProgApplyPalette { fixtures, palette } => self.apply_palette(&fixtures, palette)?,
            ProgRecordPalette { id, fixtures, kind, name } => self.record_palette(id, &fixtures, kind, name)?,
            ProgRecordCue { cuelist, number, mode, label, release, refs: _ } => {
                self.record_cue(cuelist, number, mode, label, release)?
            }
            FixtureAdd { profile, mode, count, universe, address, name } => {
                self.fixture_add(profile, mode, count, universe, address, name)?
            }
            FixtureUpdate { fixture } => {
                if !(1..=512).contains(&fixture.address) {
                    return Err("Address must be 1..512".into());
                }
                if fixture.universe == 0 {
                    return Err("Universe must be ≥ 1".into());
                }
                let prof = self.profiles.get(&fixture.profile).ok_or("Unknown profile")?;
                if !prof.modes.iter().any(|m| m.name == fixture.mode) {
                    return Err(format!("Profile has no mode '{}'", fixture.mode));
                }
                let pos = self
                    .show
                    .fixtures
                    .iter()
                    .position(|f| f.id == fixture.id)
                    .ok_or_else(|| format!("Fixture {} does not exist", fixture.id))?;
                if !self.show.universes.iter().any(|u| u.id == fixture.universe) {
                    self.show.universes.push(UniverseCfg {
                        id: fixture.universe,
                        name: format!("Universe {}", fixture.universe),
                        outputs: vec![],
                    });
                    self.show.universes.sort_by_key(|u| u.id);
                    self.io_dirty = true;
                }
                self.show.fixtures[pos] = fixture;
                self.touch_patch();
            }
            FixtureRemove { ids } => {
                self.show.fixtures.retain(|f| !ids.contains(&f.id));
                for g in &mut self.show.groups {
                    g.fixtures.retain(|f| !ids.contains(f));
                }
                for e in &mut self.show.effects {
                    e.fixtures.retain(|f| !ids.contains(f));
                }
                for id in &ids {
                    self.programmer.remove(id);
                }
                self.touch_patch();
            }
            LibraryImportOfl { json } => {
                let p = library::import_ofl(&json)?;
                match self.show.profiles.iter_mut().find(|x| x.id == p.id) {
                    Some(x) => *x = p,
                    None => self.show.profiles.push(p),
                }
                self.library_dirty = true;
                self.touch_patch();
            }
            LibraryRemove { id } => {
                if self.show.fixtures.iter().any(|f| f.profile == id) {
                    return Err(format!("Profile '{id}' is used by patched fixtures"));
                }
                let before = self.show.profiles.len();
                self.show.profiles.retain(|p| p.id != id);
                if self.show.profiles.len() == before {
                    return Err("Only imported profiles can be removed (built-in ones are read-only)".into());
                }
                self.store.remove_library_dir(&crate::gdtf::asset_dir(&id));
                self.library_dirty = true;
                self.touch_patch();
            }
            GroupSave { mut group } => {
                if group.id == 0 {
                    group.id = next_id(self.show.groups.iter().map(|g| g.id));
                }
                match self.show.groups.iter_mut().find(|g| g.id == group.id) {
                    Some(g) => *g = group,
                    None => self.show.groups.push(group),
                }
                self.touch();
            }
            GroupRemove { id } => {
                self.show.groups.retain(|g| g.id != id);
                self.touch();
            }
            PaletteSave { mut palette } => {
                if palette.id == 0 {
                    palette.id = next_id(self.show.palettes.iter().map(|g| g.id));
                }
                for v in palette.values.values_mut() {
                    *v = clamp01(*v);
                }
                match self.show.palettes.iter_mut().find(|g| g.id == palette.id) {
                    Some(g) => *g = palette,
                    None => self.show.palettes.push(palette),
                }
                self.touch();
            }
            PaletteRemove { id } => {
                self.show.palettes.retain(|g| g.id != id);
                self.touch();
            }
            EffectSave { mut effect } => {
                if effect.id == 0 {
                    effect.id = next_id(self.show.effects.iter().map(|g| g.id));
                }
                match self.show.effects.iter_mut().find(|g| g.id == effect.id) {
                    Some(g) => *g = effect,
                    None => self.show.effects.push(effect),
                }
                self.touch();
            }
            EffectRemove { id } => {
                self.show.effects.retain(|g| g.id != id);
                self.effects_on.remove(&id);
                self.touch();
            }
            EffectToggle { id, on } => {
                if !self.show.effects.iter().any(|e| e.id == id) {
                    return Err(format!("Effect {id} does not exist"));
                }
                if on {
                    self.effects_on.insert(id);
                } else {
                    self.effects_on.remove(&id);
                }
                self.dirty = true;
            }
            CuelistSave { mut cuelist } => {
                if cuelist.id == 0 {
                    cuelist.id = next_id(self.show.cuelists.iter().map(|g| g.id));
                }
                sort_cues(&mut cuelist);
                for w in cuelist.cues.windows(2) {
                    if same_number(w[0].number, w[1].number) {
                        return Err(format!("Duplicate cue number {}", w[0].number));
                    }
                }
                let id = cuelist.id;
                match self.show.cuelists.iter_mut().find(|g| g.id == id) {
                    Some(g) => *g = cuelist,
                    None => self.show.cuelists.push(cuelist),
                }
                self.after_cuelist_edit(id);
            }
            CuelistRemove { id } => {
                self.show.cuelists.retain(|g| g.id != id);
                self.runs.remove(&id);
                self.after_cuelist_edit(id);
            }
            CueSave { cuelist, cue } => {
                if !cue.number.is_finite() || cue.number < 0.0 {
                    return Err("Invalid cue number".into());
                }
                let cl = self.show.cuelists.iter_mut().find(|c| c.id == cuelist).ok_or("Cuelist does not exist")?;
                match index_of(cl, cue.number) {
                    Some(i) => cl.cues[i] = cue,
                    None => cl.cues.push(cue),
                }
                sort_cues(cl);
                self.after_cuelist_edit(cuelist);
            }
            CueRemove { cuelist, number } => {
                let cl = self.show.cuelists.iter_mut().find(|c| c.id == cuelist).ok_or("Cuelist does not exist")?;
                cl.cues.retain(|c| !same_number(c.number, number));
                self.after_cuelist_edit(cuelist);
            }
            Go { cuelist } => self.go(cuelist)?,
            Back { cuelist } => self.back(cuelist)?,
            GoTo { cuelist, number } => self.go_to(cuelist, number)?,
            Pause { cuelist } => self.pause(cuelist)?,
            ReleaseCuelist { cuelist } => self.release_cuelist(cuelist),
            ExecSave { mut executor } => {
                if executor.id == 0 {
                    executor.id = next_id(self.show.executors.iter().map(|g| g.id));
                }
                let (id, page, slot) = (executor.id, executor.page, executor.slot);
                self.show.executors.retain(|e| e.id == id || !(e.page == page && e.slot == slot));
                match self.show.executors.iter_mut().find(|g| g.id == id) {
                    Some(g) => *g = executor,
                    None => self.show.executors.push(executor),
                }
                self.touch();
            }
            ExecRemove { id } => {
                self.show.executors.retain(|e| e.id != id);
                self.exec.remove(&id);
                self.touch();
            }
            ExecLevel { id, level } => self.set_exec_level(id, level)?,
            ExecGo { id } => self.exec_go(id)?,
            ExecBack { id } => self.exec_back(id)?,
            ExecFlash { id, down } => self.set_flash(id, down)?,
            GrandMaster { level } => {
                self.grand_master = clamp01(level);
                self.dirty = true;
            }
            Blackout { on } => {
                self.blackout = on;
                self.dirty = true;
            }
            DmxSet { universe, channel, value } => {
                if !(1..=512).contains(&channel) || universe == 0 {
                    return Err("Universe/channel out of range".into());
                }
                match value {
                    Some(v) => {
                        self.dmx_override.insert((universe, channel), v);
                    }
                    None => {
                        self.dmx_override.remove(&(universe, channel));
                    }
                }
                self.dirty = true;
            }
            UniverseSave { universe } => {
                if universe.id == 0 {
                    return Err("Universe id must be ≥ 1".into());
                }
                match self.show.universes.iter_mut().find(|u| u.id == universe.id) {
                    Some(u) => *u = universe,
                    None => self.show.universes.push(universe),
                }
                self.show.universes.sort_by_key(|u| u.id);
                self.io_dirty = true;
                self.touch_patch();
            }
            UniverseRemove { id } => {
                let n = self.show.fixtures.iter().filter(|f| f.universe == id).count();
                if n > 0 {
                    return Err(format!("Universe {id} still has {n} patched fixture(s)"));
                }
                self.show.universes.retain(|u| u.id != id);
                self.io_dirty = true;
                self.touch_patch();
            }
            SettingsSave { settings } => {
                if settings.timecode.fps <= 0.0 || !settings.timecode.fps.is_finite() {
                    return Err("Invalid frame rate".into());
                }
                self.hz = settings.tick_hz.clamp(10, 100);
                self.show.settings = settings;
                self.io_dirty = true;
                self.touch_patch();
            }
            BindingSave { mut binding } => {
                if binding.id == 0 {
                    binding.id = next_id(self.show.bindings.iter().map(|g| g.id));
                }
                match self.show.bindings.iter_mut().find(|g| g.id == binding.id) {
                    Some(g) => *g = binding,
                    None => self.show.bindings.push(binding),
                }
                self.touch();
            }
            BindingRemove { id } => {
                self.show.bindings.retain(|b| b.id != id);
                self.bind_last.remove(&id);
                self.touch();
            }
            LearnStart { binding } => self.learn = Some(binding.unwrap_or(0)),
            LearnStop => self.learn = None,
            MidiIn { port, bytes } => self.midi_in(&port, &bytes),
            TimecodeCtl { running, frames } => {
                self.tc.running = running;
                if let Some(f) = frames {
                    self.tc.frames_f = f as f64;
                }
            }
            StageSave { stage } => {
                self.show.stage = stage;
                self.touch();
            }
            ShowNew { name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err("Name required".into());
                }
                let mut s = Show::default();
                s.name = name.to_string();
                self.set_show(s);
            }
            ShowSave { name } => {
                let n = name.unwrap_or_else(|| self.show.name.clone());
                self.store.save(&n, &self.show)?;
                if self.show.name != n {
                    self.show.name = n;
                    self.touch();
                }
                let _ = self.store.autosave(&self.show);
                self.shows_dirty = true;
            }
            ShowLoad { name } => {
                let mut s = self.store.load(&name)?;
                s.name = name;
                self.set_show(s);
            }
            ShowRename { name } => {
                let n = name.trim();
                if n.is_empty() {
                    return Err("Name required".into());
                }
                self.show.name = n.to_string();
                self.touch();
            }
            SubscribeDmx { .. } => {}
            // ── wave-2 stubs, one group per owner (replace as features land) ──
            // be-prog
            Undo | Redo | BlindSet { .. } | CueLoad { .. } | CueUpdate { .. } | Unpark { .. } => {}
            // be-play
            ChaseSave { .. } | ChaseRemove { .. } | ChaseRun { .. } | ChaseBpm { .. } | ChaseTap { .. } | ChaseNudge { .. } => {}
            SpeedSave { .. } | SpeedRemove { .. } | SpeedSet { .. } | SpeedTap { .. } | CuelistManual { .. } => {}
            // be-auto
            CommandLine { .. } | MacroSave { .. } | MacroRemove { .. } | MacroRun { .. } | MacroStop { .. } => {}
            TimelineSave { .. } | TimelineRemove { .. } | TimelineRun { .. } | TimelineStop { .. } | TimelineSeek { .. } => {}
            // be-data
            BackupCreate { .. } | BackupRestore { .. } | BackupDelete { .. } | NodesScan | OutputTest { .. } | Identify { .. } => {}
            SceneRecord { id, name, from_output, fixtures, merge, refs: _ } => self.scene_record(id, name, from_output, fixtures, merge)?,
            SceneSave { scene } => self.scene_save(scene)?,
            SceneRemove { id } => self.scene_remove(id),
            SceneGo { id, on } => self.scene_go(id, on)?,
            SceneSolo { id } => {
                let now = self.now();
                self.scene_solo_at(id, now)?
            }
            SceneLevel { id, level } => self.scene_level(id, level)?,
            SceneMorph { a, b, t } => self.scene_morph(a, b, t)?,
            ScenesRelease => {
                let now = self.now();
                self.scenes_release_at(now)
            }
            FixtureMorph { ids, profile, mode, repack } => self.fixture_morph(ids, profile, mode, repack)?,
            MidiEnable { output, name, enabled } => self.midi.set_enabled(output, &name, enabled)?,
            VcSave { pages } => self.vc_save(pages)?,
            VcPress { widget, down } => self.vc_press(widget, down)?,
            VcValue { widget, value } => self.vc_set_value(widget, value)?,
            VcXy { widget, x, y } => self.vc_xy(widget, x, y)?,
        }
        Ok(())
    }

    fn after_cuelist_edit(&mut self, id: u32) {
        // Drop playback state that points at a cue that no longer exists.
        let valid = self
            .show
            .cuelists
            .iter()
            .find(|c| c.id == id)
            .map(|cl| self.runs.get(&id).map_or(true, |r| r.current.map_or(true, |n| index_of(cl, n).is_some())))
            .unwrap_or(false);
        if !valid {
            self.runs.remove(&id);
        }
        self.patch_dirty = true;
        self.touch();
    }

    fn fixture_add(
        &mut self,
        profile: String,
        mode: String,
        count: u32,
        universe: u16,
        address: Option<u16>,
        name: Option<String>,
    ) -> Result<(), String> {
        if count == 0 || count > 1024 {
            return Err("Count must be 1..1024".into());
        }
        if universe == 0 {
            return Err("Universe must be ≥ 1".into());
        }
        let prof = self.profiles.get(&profile).ok_or_else(|| format!("Unknown profile '{profile}'"))?.clone();
        let m = prof.modes.iter().find(|m| m.name == mode).ok_or_else(|| format!("Profile has no mode '{mode}'"))?;
        let fp = library::mode_footprint(m);
        if fp == 0 || fp > 512 {
            return Err("Mode has no channels".into());
        }
        if let Some(a) = address {
            if !(1..=512).contains(&a) {
                return Err("Address must be 1..512".into());
            }
        }
        let mut used = occupancy(&self.show, &self.profiles);
        let mut next = next_id(self.show.fixtures.iter().map(|f| f.id));
        let existing = self.show.fixtures.len();
        let mut cursor = address;
        let mut cur_u = universe;
        let mut added = Vec::new();
        for i in 0..count {
            let (u, a) = match cursor {
                Some(a) if a as usize + fp as usize - 1 <= 512 => {
                    let occ = used.entry(cur_u).or_insert([false; 512]);
                    for b in occ.iter_mut().skip(a as usize - 1).take(fp as usize) {
                        *b = true;
                    }
                    (cur_u, a)
                }
                Some(_) => allocate(&mut used, cur_u.saturating_add(1), 1, fp).ok_or("Out of DMX address space")?,
                None => allocate(&mut used, cur_u, 1, fp).ok_or("Out of DMX address space")?,
            };
            cur_u = u;
            cursor = Some(a + fp);
            if cursor.map_or(false, |c| c > 512) {
                cursor = Some(513);
            }
            let k = existing + i as usize;
            let fname = match (&name, count) {
                (Some(n), 1) => n.clone(),
                (Some(n), _) => format!("{n} {}", i + 1),
                (None, _) => format!("{} {}", prof.model, next),
            };
            added.push(Fixture {
                id: next,
                name: fname,
                profile: profile.clone(),
                mode: mode.clone(),
                universe: u,
                address: a,
                pos: [((k % 10) as f32 - 4.5) * 1.2, 5.0, (k / 10) as f32 * 1.5 - 1.0],
                rot: [0.0, 0.0, 0.0],
            });
            next += 1;
        }
        for f in &added {
            if !self.show.universes.iter().any(|u| u.id == f.universe) {
                self.show.universes.push(UniverseCfg { id: f.universe, name: format!("Universe {}", f.universe), outputs: vec![] });
                self.io_dirty = true;
            }
        }
        self.show.universes.sort_by_key(|u| u.id);
        self.show.fixtures.extend(added);
        self.touch_patch();
        Ok(())
    }

    // ───────────── programmer / palettes ─────────────

    fn apply_palette(&mut self, fixtures: &[u32], pal_id: u32) -> Result<(), String> {
        let pal = self
            .show
            .palettes
            .iter()
            .find(|p| p.id == pal_id)
            .ok_or_else(|| format!("Palette {pal_id} does not exist"))?
            .clone();
        for f in fixtures {
            let Some(pf) = self.patch.get(*f) else { continue };
            let e = self.programmer.entry(*f).or_default();
            for (a, v) in &pal.values {
                if pf.has_attr(a) {
                    e.insert(a.clone(), clamp01(*v));
                }
            }
            if let Some(o) = pal.per_fixture.get(f) {
                for (a, v) in o {
                    if pf.has_attr(a) {
                        e.insert(a.clone(), clamp01(*v));
                    }
                }
            }
        }
        self.programmer.retain(|_, m| !m.is_empty());
        self.dirty = true;
        Ok(())
    }

    fn record_palette(&mut self, id: Option<u32>, fixtures: &[u32], kind: PaletteKind, name: String) -> Result<(), String> {
        let mut gathered: Vec<(u32, Attrs)> = Vec::new();
        for f in fixtures {
            if let Some(m) = self.programmer.get(f) {
                let a: Attrs = m.iter().filter(|(k, _)| kind_matches(k, kind)).map(|(k, v)| (k.clone(), *v)).collect();
                if !a.is_empty() {
                    gathered.push((*f, a));
                }
            }
        }
        if gathered.is_empty() {
            return Err("Nothing in the programmer for that palette type".into());
        }
        let mut values = Attrs::new();
        let mut per_fixture = PerFixture::new();
        let all_attrs: BTreeSet<String> = gathered.iter().flat_map(|(_, a)| a.keys().cloned()).collect();
        for attr in all_attrs {
            let vals: Vec<(u32, f32)> = gathered.iter().filter_map(|(f, a)| a.get(&attr).map(|v| (*f, *v))).collect();
            let first = vals[0].1;
            if vals.iter().all(|(_, v)| (v - first).abs() < 1e-4) {
                values.insert(attr, first);
            } else {
                for (f, v) in vals {
                    per_fixture.entry(f).or_default().insert(attr.clone(), v);
                }
            }
        }
        let pid = id.unwrap_or_else(|| next_id(self.show.palettes.iter().map(|p| p.id)));
        let name = if name.trim().is_empty() { format!("Palette {pid}") } else { name };
        let pal = Palette { id: pid, name, kind, values, per_fixture };
        match self.show.palettes.iter_mut().find(|p| p.id == pid) {
            Some(p) => *p = pal,
            None => self.show.palettes.push(pal),
        }
        self.touch();
        Ok(())
    }

    fn record_cue(&mut self, cuelist: u32, number: Option<f64>, mode: RecordMode, label: String, release: bool) -> Result<(), String> {
        if self.programmer.is_empty() {
            return Err("Programmer is empty — nothing to record".into());
        }
        if !self.show.cuelists.iter().any(|c| c.id == cuelist) {
            self.show.cuelists.push(Cuelist {
                id: cuelist,
                name: format!("Cuelist {cuelist}"),
                cues: vec![],
                looped: false,
                tracking: true,
                priority: 0,
                auto_mark: false,
            });
        }
        let prog = self.programmer.clone();
        let cl = self.show.cuelists.iter_mut().find(|c| c.id == cuelist).ok_or("Cuelist does not exist")?;
        let num = number.unwrap_or_else(|| cl.cues.last().map_or(1.0, |c| c.number.floor() + 1.0));
        if !num.is_finite() || num < 0.0 {
            return Err("Invalid cue number".into());
        }
        match index_of(cl, num) {
            Some(i) => {
                let c = &mut cl.cues[i];
                match mode {
                    RecordMode::Replace => c.changes = prog,
                    RecordMode::Merge => {
                        for (f, attrs) in prog {
                            let e = c.changes.entry(f).or_default();
                            for (a, v) in attrs {
                                e.insert(a, v);
                            }
                        }
                    }
                }
                if !label.is_empty() {
                    c.label = label;
                }
            }
            None => {
                cl.cues.push(Cue {
                    number: num,
                    label,
                    fade_in: 2.0,
                    fade_out: 2.0,
                    delay: 0.0,
                    follow: None,
                    timecode: None,
                    changes: prog,
                    block: false,
                    refs: Default::default(),
                    timing: Default::default(),
                    curve: Default::default(),
                    stagger: 0.0,
                    notes: String::new(),
                });
                sort_cues(cl);
            }
        }
        if release {
            self.programmer.clear();
        }
        self.after_cuelist_edit(cuelist);
        Ok(())
    }

    // ───────────── playback ─────────────

    fn go(&mut self, id: u32) -> Result<(), String> {
        let now = self.now();
        let cl = self.require_cuelist(id)?;
        if cl.cues.is_empty() {
            return Err("Cuelist has no cues".into());
        }
        let len = cl.cues.len();
        let looped = cl.looped;
        let cur_idx = self.runs.get(&id).and_then(|r| r.current).and_then(|n| index_of(cl, n));
        if let Some(r) = self.runs.get_mut(&id) {
            if r.state == PlayState::Paused {
                if let Some(p) = r.paused_at.take() {
                    r.start += now - p;
                    if let Some(f) = r.follow_at.as_mut() {
                        *f += now - p;
                    }
                }
                r.state = PlayState::Running;
                return Ok(());
            }
        }
        let idx = match cur_idx {
            None => 0,
            Some(i) if i + 1 < len => i + 1,
            Some(_) if looped => 0,
            Some(_) => return Ok(()),
        };
        self.start_cue(id, idx);
        Ok(())
    }

    fn back(&mut self, id: u32) -> Result<(), String> {
        let cl = self.require_cuelist(id)?;
        let cur = self.runs.get(&id).and_then(|r| r.current).and_then(|n| index_of(cl, n));
        let last = cl.cues.len().saturating_sub(1);
        let looped = cl.looped;
        match cur {
            Some(i) if i > 0 => self.start_cue(id, i - 1),
            Some(_) if looped && last > 0 => self.start_cue(id, last),
            _ => {}
        }
        Ok(())
    }

    fn go_to(&mut self, id: u32, number: f64) -> Result<(), String> {
        let cl = self.require_cuelist(id)?;
        let idx = index_of(cl, number).ok_or_else(|| format!("Cue {number} does not exist"))?;
        self.start_cue(id, idx);
        Ok(())
    }

    fn pause(&mut self, id: u32) -> Result<(), String> {
        self.require_cuelist(id)?;
        let now = self.now();
        if let Some(r) = self.runs.get_mut(&id) {
            match r.state {
                PlayState::Running => {
                    r.paused_at = Some(now);
                    r.state = PlayState::Paused;
                }
                PlayState::Paused => {
                    if let Some(p) = r.paused_at.take() {
                        r.start += now - p;
                        if let Some(f) = r.follow_at.as_mut() {
                            *f += now - p;
                        }
                    }
                    r.state = PlayState::Running;
                }
                PlayState::Idle => {}
            }
        }
        Ok(())
    }

    fn release_cuelist(&mut self, id: u32) {
        let now = self.now();
        if let Some(r) = self.runs.get_mut(&id) {
            if r.values.is_empty() {
                self.runs.remove(&id);
                self.dirty = true;
                return;
            }
            r.from = r.values.clone();
            r.to = r.values.clone();
            r.drop.clear();
            for (f, attrs) in r.to.iter_mut() {
                for (a, v) in attrs.iter_mut() {
                    if a == "dimmer" {
                        *v = 0.0;
                    }
                    r.drop.push((*f, a.clone()));
                }
            }
            r.start = now;
            r.delay = 0.0;
            r.fade_in = 1.0;
            r.fade_out = 1.0;
            r.paused_at = None;
            r.follow_at = None;
            r.state = PlayState::Idle;
            r.current = None;
            r.settled = false;
            r.releasing = true;
            r.progress = 0.0;
        }
        self.dirty = true;
    }

    fn start_cue(&mut self, id: u32, idx: usize) {
        let now = self.now();
        let Some(cl) = self.show.cuelists.iter().find(|c| c.id == id) else { return };
        let Some(cue) = cl.cues.get(idx).cloned() else { return };
        let to = tracked_state(cl, idx);
        self.go_seq += 1;
        let seq = self.go_seq;
        let run = self.runs.entry(id).or_insert_with(|| CueRun::new(id));
        let mut from = run.values.clone();
        for (f, attrs) in &to {
            for a in attrs.keys() {
                if !from.get(f).map_or(false, |m| m.contains_key(a)) {
                    let fb = if a == "dimmer" {
                        0.0
                    } else {
                        self.last_merged.get(f).and_then(|m| m.get(a)).copied().unwrap_or_else(|| attr_default(a))
                    };
                    from.entry(*f).or_default().insert(a.clone(), fb);
                }
            }
        }
        let mut to_full = to.clone();
        let mut drop = Vec::new();
        for (f, attrs) in &from {
            for (a, v) in attrs {
                if !to.get(f).map_or(false, |m| m.contains_key(a)) {
                    to_full.entry(*f).or_default().insert(a.clone(), if a == "dimmer" { 0.0 } else { *v });
                    drop.push((*f, a.clone()));
                }
            }
        }
        run.state = PlayState::Running;
        run.current = Some(cue.number);
        run.from = from;
        run.to = to_full;
        run.drop = drop;
        run.start = now;
        run.delay = cue.delay.max(0.0);
        run.fade_in = cue.fade_in.max(0.0);
        run.fade_out = cue.fade_out.max(0.0);
        run.paused_at = None;
        run.go_seq = seq;
        run.follow_at = cue.follow.map(|s| now + s.max(0.0) as f64);
        run.settled = false;
        run.releasing = false;
        run.progress = 0.0;
        self.dirty = true;
    }

    fn update_playback(&mut self, now: f64) {
        let patch = &self.patch;
        let disc = |f: u32, a: &str| patch.get(f).map_or(false, |p| p.is_discrete(a));
        for r in self.runs.values_mut() {
            r.update(now, &disc);
        }
        self.scene_update(now);
        let due: Vec<u32> = self
            .runs
            .values()
            .filter(|r| r.state == PlayState::Running && r.follow_at.map_or(false, |t| now >= t))
            .map(|r| r.cuelist)
            .collect();
        for id in due {
            if let Some(r) = self.runs.get_mut(&id) {
                r.follow_at = None;
            }
            let _ = self.go(id);
        }
    }

    // ───────────── executors ─────────────

    fn exec_kind(&self, id: u32) -> Result<ExecKind, String> {
        self.show.executors.iter().find(|e| e.id == id).map(|e| e.kind.clone()).ok_or_else(|| format!("Executor {id} does not exist"))
    }

    fn set_exec_level(&mut self, id: u32, level: f32) -> Result<(), String> {
        let kind = self.exec_kind(id)?;
        let l = clamp01(level);
        self.exec.entry(id).or_insert(ExecState { level: 1.0, flash: false }).level = l;
        match kind {
            ExecKind::Cuelist { cuelist } => {
                self.cl_levels.insert(cuelist, l);
            }
            ExecKind::Master { master: MasterKind::Grand } => self.grand_master = l,
            ExecKind::Scene { scene } => self.scene_level(scene, l)?,
            _ => {}
        }
        self.dirty = true;
        Ok(())
    }

    fn exec_go(&mut self, id: u32) -> Result<(), String> {
        match self.exec_kind(id)? {
            ExecKind::Cuelist { cuelist } => self.go(cuelist),
            ExecKind::Effect { effect } => {
                if !self.effects_on.remove(&effect) {
                    self.effects_on.insert(effect);
                }
                self.dirty = true;
                Ok(())
            }
            ExecKind::Scene { scene } => self.scene_toggle(scene),
            _ => Ok(()),
        }
    }

    fn exec_back(&mut self, id: u32) -> Result<(), String> {
        match self.exec_kind(id)? {
            ExecKind::Cuelist { cuelist } => self.back(cuelist),
            ExecKind::Scene { scene } => self.scene_go(scene, false),
            _ => Ok(()),
        }
    }

    fn set_flash(&mut self, id: u32, down: bool) -> Result<(), String> {
        let kind = self.exec_kind(id)?;
        self.exec.entry(id).or_insert(ExecState { level: 1.0, flash: false }).flash = down;
        match kind {
            ExecKind::Effect { effect } => {
                if down {
                    self.effect_flash.insert(effect);
                } else {
                    self.effect_flash.remove(&effect);
                }
            }
            ExecKind::Scene { scene } => self.scene_set_flash(scene, down)?,
            _ => {}
        }
        self.dirty = true;
        Ok(())
    }

    fn exec_level(&self, id: u32) -> f32 {
        self.exec.get(&id).map_or(1.0, |e| e.level)
    }

    // ───────────── inputs ─────────────

    fn midi_in(&mut self, port: &str, bytes: &[u8]) {
        if self.show.settings.timecode.source == TcSource::Mtc {
            if let Some(fr) = self.tc.dec.feed(bytes, self.show.settings.timecode.fps) {
                self.tc.frames_f = fr as f64;
                self.tc.running = true;
                self.tc.last_mtc = Some(self.now());
            }
        }
        if let Some(msg) = parse_midi(bytes) {
            self.handle_input(InEvent::Midi { port, msg });
        }
    }

    fn handle_input(&mut self, ev: InEvent) {
        self.recent.push_front(ev.log_line());
        self.recent.truncate(8);
        if let Some(target) = self.learn {
            if ev.learnable() {
                self.learn = None;
                let _ = target;
                self.emit(&ServerMsg::Learned { source: ev.as_source() });
                return;
            }
        }
        self.vc_input(&ev);
        let value = ev.value();
        let matches: Vec<(u32, Action, f32)> = self
            .show
            .bindings
            .iter()
            .filter(|b| b.enabled && source_matches(&b.source, &ev))
            .map(|b| (b.id, b.action.clone(), b.out_min + value * (b.out_max - b.out_min)))
            .collect();
        for (bid, action, mapped) in matches {
            let prev = self.bind_last.insert(bid, mapped);
            let edge = rising_edge(prev, mapped);
            if let Err(e) = self.exec_action(action, mapped, edge) {
                tracing::debug!("binding {bid}: {e}");
            }
        }
    }

    fn exec_action(&mut self, action: Action, v: f32, edge: bool) -> Result<(), String> {
        match action {
            Action::ExecLevel { exec } => self.set_exec_level(exec, v)?,
            Action::ExecGo { exec } if edge => self.exec_go(exec)?,
            Action::ExecBack { exec } if edge => self.exec_back(exec)?,
            Action::ExecFlash { exec } => self.set_flash(exec, v >= 0.5)?,
            Action::Go { cuelist } if edge => self.go(cuelist)?,
            Action::Back { cuelist } if edge => self.back(cuelist)?,
            Action::GrandMaster => {
                self.grand_master = clamp01(v);
                self.dirty = true;
            }
            Action::BlackoutToggle if edge => {
                self.blackout = !self.blackout;
                self.dirty = true;
            }
            Action::Dmx { universe, channel } => {
                self.handle(Command::DmxSet { universe, channel, value: Some((clamp01(v) * 255.0).round() as u8) })?
            }
            Action::FixtureAttr { fixture, attr } => {
                let mut a = Attrs::new();
                a.insert(attr, clamp01(v));
                self.handle(Command::ProgSet { fixtures: vec![fixture], attrs: a })?
            }
            Action::GroupAttr { group, attr } => {
                let fx = self.show.groups.iter().find(|g| g.id == group).map(|g| g.fixtures.clone()).ok_or("No such group")?;
                let mut a = Attrs::new();
                a.insert(attr, clamp01(v));
                self.handle(Command::ProgSet { fixtures: fx, attrs: a })?
            }
            Action::EffectToggle { effect } if edge => {
                if !self.effects_on.remove(&effect) {
                    self.effects_on.insert(effect);
                }
                self.dirty = true;
            }
            Action::PaletteApply { palette, group } if edge => {
                let fx: Vec<u32> = match group {
                    Some(g) => self.show.groups.iter().find(|x| x.id == g).map(|x| x.fixtures.clone()).ok_or("No such group")?,
                    None => self.show.fixtures.iter().map(|f| f.id).collect(),
                };
                self.apply_palette(&fx, palette)?;
            }
            Action::ProgClear if edge => {
                self.programmer.clear();
                self.dirty = true;
            }
            _ => {}
        }
        Ok(())
    }

    // ───────────── timecode ─────────────

    fn tc_display(&self) -> u32 {
        let off = self.show.settings.timecode.offset_frames as i64;
        (self.tc.frames_f as i64 + off).max(0) as u32
    }

    fn update_timecode(&mut self, now: f64, dt: f64) {
        let tcfg = &self.show.settings.timecode;
        match tcfg.source {
            TcSource::Internal => {
                if self.tc.running {
                    self.tc.frames_f += dt * tcfg.fps as f64;
                }
            }
            TcSource::Mtc => {
                if let Some(t) = self.tc.last_mtc {
                    if now - t > 0.5 {
                        self.tc.running = false;
                    }
                }
            }
            TcSource::Off => {}
        }
        if !self.tc.running || tcfg.source == TcSource::Off {
            self.tc.prev_disp = self.tc_display();
            return;
        }
        let disp = self.tc_display();
        let prev = self.tc.prev_disp;
        if disp != prev {
            let max_jump = (self.show.settings.timecode.fps * 2.0) as u32;
            if disp > prev && disp - prev <= max_jump {
                let fire: Vec<(u32, f64)> =
                    self.tc_cues.iter().filter(|(f, _, _)| *f > prev && *f <= disp).map(|(_, cl, n)| (*cl, *n)).collect();
                for (cl, n) in fire {
                    let _ = self.go_to(cl, n);
                }
            }
            self.tc.prev_disp = disp;
        }
    }

    // ───────────── render ─────────────

    /// Steps 1–3 of the render: cuelists → scenes (+ scene morph) → programmer → effects.
    fn compute_look(&mut self, now: f64) -> PerFixture {
        // 1. cuelists
        let mut order: Vec<&CueRun> = self.runs.values().filter(|r| !r.values.is_empty()).collect();
        order.sort_by_key(|r| (self.show.cuelists.iter().find(|c| c.id == r.cuelist).map_or(0, |c| c.priority), r.go_seq));
        let mut merged = PerFixture::new();
        for r in order {
            let flash = self.show.executors.iter().any(|e| {
                matches!(e.kind, ExecKind::Cuelist { cuelist } if cuelist == r.cuelist) && self.exec.get(&e.id).map_or(false, |s| s.flash)
            });
            let level = if flash { 1.0 } else { self.cl_levels.get(&r.cuelist).copied().unwrap_or(1.0) };
            for (f, attrs) in &r.values {
                let e = merged.entry(*f).or_default();
                for (a, v) in attrs {
                    if a == "dimmer" {
                        let nv = v * level;
                        let cur = e.entry(a.clone()).or_insert(0.0);
                        if nv > *cur {
                            *cur = nv;
                        }
                    } else if level > 0.0 {
                        e.insert(a.clone(), *v);
                    }
                }
            }
        }
        // 1b. scenes and scene morph
        self.apply_scenes(&mut merged);
        self.last_merged = merged.clone();
        // 2. programmer
        for (f, attrs) in &self.programmer {
            let e = merged.entry(*f).or_default();
            for (a, v) in attrs {
                e.insert(a.clone(), *v);
            }
        }
        // 3. effects
        if !self.effects_on.is_empty() || !self.effect_flash.is_empty() {
            let on = &self.effects_on;
            let fl = &self.effect_flash;
            let execs = &self.show.executors;
            let exec = &self.exec;
            let overridden: Vec<Effect>;
            let effs: &[Effect] = if self.vc.effect_rate.is_empty() {
                &self.show.effects
            } else {
                overridden = self
                    .show
                    .effects
                    .iter()
                    .map(|e| {
                        let mut e = e.clone();
                        if let Some(r) = self.vc.effect_rate.get(&e.id) {
                            e.rate = *r;
                        }
                        e
                    })
                    .collect();
                &overridden
            };
            effects::apply(
                &mut merged,
                effs,
                &|id| on.contains(&id) || fl.contains(&id),
                &|id| {
                    execs
                        .iter()
                        .find(|e| matches!(e.kind, ExecKind::Effect { effect } if effect == id))
                        .map_or(1.0, |e| exec.get(&e.id).map_or(1.0, |s| s.level))
                },
                &self.patch,
                now,
            );
        }
        merged
    }

    fn render(&mut self, now: f64) {
        let mut merged = self.compute_look(now);
        // colour emulation: derive what each fixture really has from the colour attributes of the look
        for pf in &self.patch.fixtures {
            if let Some(a) = merged.get_mut(&pf.id) {
                morph::emulate(pf, a);
            }
        }
        // 4. masters
        let mut factor: HashMap<u32, f32> = HashMap::new();
        for e in &self.show.executors {
            if let ExecKind::Group { group } = e.kind {
                let st = self.exec.get(&e.id).copied().unwrap_or(ExecState { level: 1.0, flash: false });
                if let Some(g) = self.show.groups.iter().find(|g| g.id == group) {
                    for f in &g.fixtures {
                        if st.flash {
                            merged.entry(*f).or_default().insert("dimmer".into(), 1.0);
                        }
                        if st.level < 1.0 {
                            *factor.entry(*f).or_insert(1.0) *= st.level;
                        }
                    }
                }
            }
        }
        let gm = if self.blackout { 0.0 } else { self.grand_master };
        for pf in &self.patch.fixtures {
            let mut fct = factor.get(&pf.id).copied().unwrap_or(1.0) * gm;
            // Virtual dimmer: fixtures without a dimmer channel (RGB pars…) are dimmed by scaling colour.
            if !pf.has_dimmer {
                if let Some(d) = merged.get_mut(&pf.id).and_then(|a| a.remove("dimmer")) {
                    fct *= d;
                }
            }
            if fct >= 1.0 {
                continue;
            }
            let attrs = merged.entry(pf.id).or_default();
            if pf.has_dimmer {
                let v = attrs.get("dimmer").copied().unwrap_or(0.0);
                attrs.insert("dimmer".into(), v * fct);
            } else {
                for a in COLOR_ATTRS {
                    if pf.has_attr(a) {
                        let v = attrs.get(*a).copied().unwrap_or(0.0);
                        attrs.insert((*a).into(), v * fct);
                    }
                }
            }
        }
        // 5. patch → DMX
        let mut frames: BTreeMap<u16, Box<[u8; 512]>> = self.frames.keys().map(|u| (*u, Box::new([0u8; 512]))).collect();
        let empty = Attrs::new();
        for pf in &self.patch.fixtures {
            if let Some(fr) = frames.get_mut(&pf.universe) {
                write_fixture(fr, pf.address, &pf.chans, merged.get(&pf.id).unwrap_or(&empty));
            }
        }
        // 6. external DMX (HTP) then raw overrides
        for (u, (_, data)) in &self.ext_dmx {
            if let Some(fr) = frames.get_mut(u) {
                for (i, b) in data.iter().take(512).enumerate() {
                    fr[i] = fr[i].max(*b);
                }
            }
        }
        for ((u, ch), v) in &self.dmx_override {
            if let Some(fr) = frames.get_mut(u) {
                fr[*ch as usize - 1] = *v;
            }
        }
        for (u, fr) in frames {
            let changed = self.frames.get(&u).map_or(true, |old| **old != *fr);
            if changed {
                self.bcast_pending.insert(u);
            }
            self.frames.insert(u, fr);
        }
    }

    fn continuous_active(&self) -> bool {
        !self.effects_on.is_empty()
            || !self.effect_flash.is_empty()
            || self.runs.values().any(|r| !r.settled)
            || self.scenes.ramping()
    }

    // ───────────── tick ─────────────

    fn tick(&mut self) {
        let now = self.now();
        let dt = (now - self.last_tick).max(0.0);
        self.last_tick = now;
        self.frame_count += 1;
        if self.fps_at.elapsed() >= Duration::from_secs(1) {
            self.fps = ((self.frame_count as f64 / self.fps_at.elapsed().as_secs_f64()) * 10.0).round() as f32 / 10.0;
            self.frame_count = 0;
            self.fps_at = Instant::now();
        }

        self.update_timecode(now, dt);
        self.update_playback(now);

        let before = self.ext_dmx.len();
        self.ext_dmx.retain(|_, (t, _)| t.elapsed() < Duration::from_secs(3));
        if self.ext_dmx.len() != before {
            self.dirty = true;
        }

        if self.patch_dirty {
            self.rebuild_derived();
        }
        if self.io_dirty {
            self.io.configure(&self.show.universes, &self.show.settings);
            self.io_dirty = false;
        }
        if self.dirty || self.continuous_active() {
            self.dirty = false;
            self.render(now);
        }

        // outputs: every tick, every configured universe
        for u in &self.show.universes {
            if let Some(fr) = self.frames.get(&u.id) {
                self.io.send(u.id, fr);
            }
        }
        self.broadcast_dmx();
        self.broadcast_state();
    }

    fn broadcast_dmx(&mut self) {
        if self.out.receiver_count() == 0 {
            self.bcast_pending.clear();
            return;
        }
        let pending: Vec<u16> = self.bcast_pending.iter().copied().collect();
        for u in pending {
            let ready = self.bcast_at.get(&u).map_or(true, |t| t.elapsed() >= Duration::from_millis(33));
            if ready {
                self.send_dmx(u);
                self.bcast_pending.remove(&u);
            }
        }
        let stale: Vec<u16> = self
            .frames
            .keys()
            .copied()
            .filter(|u| self.bcast_at.get(u).map_or(true, |t| t.elapsed() >= Duration::from_secs(1)))
            .collect();
        for u in stale {
            self.send_dmx(u);
        }
    }

    fn send_dmx(&mut self, u: u16) {
        if let Some(fr) = self.frames.get(&u) {
            let _ = self.out.send(Out::Dmx(u, Arc::new(**fr)));
            self.bcast_at.insert(u, Instant::now());
        }
    }

    fn broadcast_state(&mut self) {
        if self.library_dirty {
            self.library_dirty = false;
            self.emit(&ServerMsg::Library { profiles: self.library_list() });
        }
        if self.shows_dirty {
            self.shows_dirty = false;
            self.emit(&ServerMsg::Shows { shows: self.store.list() });
        }
        if self.show_pending && self.show_sent.elapsed() >= Duration::from_millis(100) {
            self.show_pending = false;
            self.show_sent = Instant::now();
            self.emit(&ServerMsg::Show { rev: self.rev, show: self.show.clone() });
        }
        if let Some(t) = self.autosave_due {
            if Instant::now() >= t {
                self.autosave_due = None;
                if let Err(e) = self.store.autosave(&self.show) {
                    tracing::warn!("autosave failed: {e}");
                }
            }
        }
        if self.rt_check_at.elapsed() >= Duration::from_millis(50) {
            self.rt_check_at = Instant::now();
            let rt = self.build_rt();
            let s = json(&ServerMsg::Rt { rt: rt.clone() });
            if s != self.last_rt || self.last_rt_at.elapsed() >= Duration::from_secs(1) {
                self.last_rt = s.clone();
                self.last_rt_at = Instant::now();
                let _ = self.out.send(Out::Json(Arc::from(s)));
            }
            self.osc_feedback(&rt);
            self.vc_feedback(&rt.vc);
        }
    }

    fn osc_feedback(&mut self, rt: &Runtime) {
        if !self.show.settings.osc_feedback.enabled {
            return;
        }
        let mut items: Vec<(String, f32)> = vec![
            ("/lumos/master/grand".into(), rt.grand_master),
            ("/lumos/blackout".into(), if rt.blackout { 1.0 } else { 0.0 }),
        ];
        for e in &rt.executors {
            items.push((format!("/lumos/exec/{}/level", e.id), e.level));
            items.push((format!("/lumos/exec/{}/active", e.id), if e.active { 1.0 } else { 0.0 }));
        }
        for sc in &rt.scenes {
            items.push((format!("/lumos/scene/{}/level", sc.id), sc.level));
            items.push((format!("/lumos/scene/{}/active", sc.id), if sc.active { 1.0 } else { 0.0 }));
        }
        for c in &rt.cuelists {
            items.push((format!("/lumos/cuelist/{}/current", c.id), c.current.unwrap_or(0.0) as f32));
        }
        for (addr, v) in items {
            if self.osc_fb.get(&addr).map_or(true, |p| (p - v).abs() > 1e-3) {
                self.io.osc_feedback(&addr, v);
                self.osc_fb.insert(addr, v);
            }
        }
    }

    fn build_rt(&self) -> Runtime {
        let (outputs, mut inputs) = self.io.status();
        inputs.recent = self.recent.iter().cloned().collect();
        let fps = self.show.settings.timecode.fps;
        let executors = self
            .show
            .executors
            .iter()
            .map(|e| {
                let st = self.exec.get(&e.id).copied().unwrap_or(ExecState { level: 1.0, flash: false });
                let active = match &e.kind {
                    ExecKind::Cuelist { cuelist } => self.runs.get(cuelist).map_or(false, |r| r.state != PlayState::Idle),
                    ExecKind::Effect { effect } => self.effects_on.contains(effect) || self.effect_flash.contains(effect),
                    ExecKind::Group { .. } => st.level > 0.0,
                    ExecKind::Master { .. } => true,
                    ExecKind::Scene { scene } => self.scenes.is_active(*scene),
                    ExecKind::Chase { .. } | ExecKind::Speed { .. } | ExecKind::XFade { .. } => false, // be-play
                    ExecKind::Empty => false,
                };
                let level = match &e.kind {
                    ExecKind::Master { .. } => self.grand_master,
                    ExecKind::Scene { scene } => self.scenes.effective(*scene),
                    _ => st.level,
                };
                ExecRt { id: e.id, level, active, flash: st.flash }
            })
            .collect();
        let cuelists = self
            .show
            .cuelists
            .iter()
            .map(|cl| {
                let run = self.runs.get(&cl.id);
                let cur_idx = run.and_then(|r| r.current).and_then(|n| index_of(cl, n));
                let next = match cur_idx {
                    Some(i) if i + 1 < cl.cues.len() => Some(cl.cues[i + 1].number),
                    Some(_) if cl.looped => cl.cues.first().map(|c| c.number),
                    Some(_) => None,
                    None => cl.cues.first().map(|c| c.number),
                };
                CuelistRt {
                    id: cl.id,
                    state: run.map_or(PlayState::Idle, |r| r.state),
                    current: run.and_then(|r| r.current),
                    next,
                    progress: run.map_or(0.0, |r| r.progress),
                    level: self.cl_levels.get(&cl.id).copied().unwrap_or(1.0),
                    manual: false,
                }
            })
            .collect();
        let mut effects_running: Vec<u32> = self.effects_on.union(&self.effect_flash).copied().collect();
        effects_running.sort_unstable();
        let frames = self.tc_display();
        Runtime {
            grand_master: self.grand_master,
            blackout: self.blackout,
            programmer: self.programmer.clone(),
            executors,
            cuelists,
            effects_running,
            outputs,
            inputs,
            scenes: self.scene_rt(),
            morph: self.scenes.morph,
            timecode: TimecodeRt { running: self.tc.running, frames, text: fmt_tc(frames, fps) },
            learn: self.learn,
            fps: self.fps,
            blind: false,
            undo: 0,
            redo: 0,
            parked: vec![],
            cmd_log: vec![],
            chases: vec![],
            speed: vec![],
            timelines: vec![],
            macros_running: vec![],
            testing: false,
            identifying: vec![],
            vc: self.vc_states(),
        }
    }
}

fn json(m: &ServerMsg) -> String {
    serde_json::to_string(m).unwrap_or_else(|e| format!("{{\"t\":\"log\",\"level\":\"error\",\"msg\":\"serialise: {e}\"}}"))
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub fn engine() -> Engine {
        let dir = std::env::temp_dir().join(format!("lumos-eng-{}-{}", std::process::id(), rand_suffix()));
        let store = Store::new(dir).unwrap();
        let (out, _) = broadcast::channel(16);
        let (tx, _rx) = mpsc::unbounded_channel();
        let mut e = Engine::new(store, out, IoHub::new(tx));
        e.show = Show::default();
        e.patch_dirty = true;
        e
    }

    fn rand_suffix() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        N.fetch_add(1, Ordering::SeqCst)
    }

    pub fn add(e: &mut Engine, profile: &str, mode: &str, n: u32) {
        e.handle(Command::FixtureAdd { profile: profile.into(), mode: mode.into(), count: n, universe: 1, address: None, name: None }).unwrap();
        e.rebuild_derived();
    }

    pub fn set(e: &mut Engine, f: u32, a: &str, v: f32) {
        let mut m = Attrs::new();
        m.insert(a.into(), v);
        e.handle(Command::ProgSet { fixtures: vec![f], attrs: m }).unwrap();
    }

    pub fn frame(e: &mut Engine) -> [u8; 512] {
        e.render(0.0);
        **e.frames.get(&1).unwrap()
    }

    /// Collect `MidiOut` messages currently queued on a broadcast receiver.
    pub fn drain_midi(rx: &mut broadcast::Receiver<Out>) -> Vec<(Option<String>, Vec<u8>)> {
        let mut v = Vec::new();
        while let Ok(m) = rx.try_recv() {
            if let Out::Json(j) = m {
                if let Ok(ServerMsg::MidiOut { port, bytes }) = serde_json::from_str::<ServerMsg>(&j) {
                    v.push((port, bytes));
                }
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;

    #[test]
    fn auto_address_and_map() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "4ch", 3);
        let a: Vec<u16> = e.show.fixtures.iter().map(|f| f.address).collect();
        assert_eq!(a, vec![1, 5, 9]);
        set(&mut e, 2, "dimmer", 1.0);
        set(&mut e, 2, "red", 0.5);
        let f = frame(&mut e);
        assert_eq!(&f[0..4], &[0, 0, 0, 0]);
        assert_eq!(&f[4..8], &[255, 128, 0, 0]);
    }

    #[test]
    fn mover_16bit_defaults() {
        let mut e = engine();
        add(&mut e, "generic/mh-spot", "16ch", 1);
        let f = frame(&mut e);
        assert_eq!(f[0], 128); // pan centre
        assert_eq!(f[2], 128);
        set(&mut e, 1, "pan", 1.0);
        let f = frame(&mut e);
        assert_eq!((f[0], f[1]), (255, 255));
    }

    #[test]
    fn grand_master_and_blackout_virtual_dimmer() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        set(&mut e, 1, "red", 1.0);
        e.handle(Command::GrandMaster { level: 0.5 }).unwrap();
        assert_eq!(frame(&mut e)[0], 128);
        e.handle(Command::Blackout { on: true }).unwrap();
        assert_eq!(frame(&mut e)[0], 0);
    }

    #[test]
    fn dimmer_attr_is_virtual_dimmer_on_rgb_only_fixture() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        set(&mut e, 1, "red", 1.0);
        set(&mut e, 1, "dimmer", 0.5);
        assert_eq!(frame(&mut e)[0], 128);
        set(&mut e, 1, "dimmer", 0.0);
        assert_eq!(frame(&mut e)[0], 0);
        e.handle(Command::GrandMaster { level: 0.5 }).unwrap();
        set(&mut e, 1, "dimmer", 1.0);
        assert_eq!(frame(&mut e)[0], 128);
    }

    #[test]
    fn cue_record_and_go_with_htp_between_lists() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 0.8);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: "A".into(), release: true, refs: true }).unwrap();
        set(&mut e, 1, "dimmer", 0.4);
        e.handle(Command::ProgRecordCue { cuelist: 2, number: None, mode: RecordMode::Merge, label: "B".into(), release: true, refs: true }).unwrap();
        assert!(e.programmer.is_empty());
        // instant cues
        for cl in e.show.cuelists.iter_mut() {
            cl.cues[0].fade_in = 0.0;
            cl.cues[0].fade_out = 0.0;
        }
        e.handle(Command::Go { cuelist: 2 }).unwrap();
        e.update_playback(0.1);
        assert_eq!(frame(&mut e)[0], 102);
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        e.update_playback(0.1);
        assert_eq!(frame(&mut e)[0], 204); // HTP: max(0.8, 0.4)
        // level master scales dimmer
        e.cl_levels.insert(1, 0.25);
        assert_eq!(frame(&mut e)[0], 102); // max(0.2, 0.4)
    }

    #[test]
    fn programmer_overrides_cue_ltp() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "4ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        set(&mut e, 1, "red", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Replace, label: String::new(), release: true, refs: true }).unwrap();
        e.show.cuelists[0].cues[0].fade_in = 0.0;
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        e.update_playback(0.1);
        assert_eq!(frame(&mut e)[1], 255);
        set(&mut e, 1, "red", 0.0);
        assert_eq!(frame(&mut e)[1], 0);
    }

    #[test]
    fn crossfade_progress() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: String::new(), release: true, refs: true }).unwrap();
        e.show.cuelists[0].cues[0].fade_in = 2.0;
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        let start = e.runs[&1].start;
        e.update_playback(start + 1.0);
        e.render(start + 1.0);
        let v = e.frames.get(&1).unwrap()[0];
        assert!((v as i32 - 128).abs() <= 1, "{v}");
    }

    #[test]
    fn go_walks_cues_and_loops() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        for v in [0.2, 0.5, 0.9] {
            set(&mut e, 1, "dimmer", v);
            e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: String::new(), release: true, refs: true }).unwrap();
        }
        assert_eq!(e.show.cuelists[0].cues.iter().map(|c| c.number).collect::<Vec<_>>(), vec![1.0, 2.0, 3.0]);
        for _ in 0..3 {
            e.handle(Command::Go { cuelist: 1 }).unwrap();
        }
        assert_eq!(e.runs[&1].current, Some(3.0));
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        assert_eq!(e.runs[&1].current, Some(3.0)); // end, not looped
        e.show.cuelists[0].looped = true;
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        assert_eq!(e.runs[&1].current, Some(1.0));
        e.handle(Command::Back { cuelist: 1 }).unwrap();
        assert_eq!(e.runs[&1].current, Some(3.0));
        e.handle(Command::GoTo { cuelist: 1, number: 2.0 }).unwrap();
        assert_eq!(e.runs[&1].current, Some(2.0));
        assert!(e.handle(Command::GoTo { cuelist: 1, number: 9.0 }).is_err());
        e.handle(Command::ReleaseCuelist { cuelist: 1 }).unwrap();
        assert_eq!(e.runs.get(&1).and_then(|r| r.current), None);
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        let mut e = engine();
        assert!(e.handle(Command::Go { cuelist: 99 }).is_err());
        assert!(e.handle(Command::FixtureAdd { profile: "nope".into(), mode: "x".into(), count: 1, universe: 1, address: None, name: None }).is_err());
        assert!(e.handle(Command::FixtureAdd { profile: "generic/dimmer".into(), mode: "1ch".into(), count: 0, universe: 1, address: None, name: None }).is_err());
        assert!(e.handle(Command::DmxSet { universe: 1, channel: 600, value: Some(1) }).is_err());
        assert!(e.handle(Command::ExecLevel { id: 5, level: 1.0 }).is_err());
        assert!(e.handle(Command::ShowLoad { name: "../x".into() }).is_err());
        assert!(e.handle(Command::LibraryImportOfl { json: "{".into() }).is_err());
        assert!(e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: String::new(), release: false, refs: true }).is_err());
    }

    #[test]
    fn native_midi_events_follow_the_midi_in_path() {
        let mut e = engine();
        e.handle(Command::BindingSave {
            binding: Binding {
                id: 0,
                label: String::new(),
                enabled: true,
                source: Source::Midi { port: None, kind: MidiKind::Cc, channel: Some(1), number: 7 },
                action: Action::GrandMaster,
                out_min: 0.0,
                out_max: 1.0,
            },
        })
        .unwrap();
        // bytes from a native input arrive as IoEvent::Midi — same effect as Command::MidiIn
        e.on_io(IoEvent::Midi { port: "Native Pad".into(), bytes: vec![0xB0, 7, 64] });
        assert!((e.grand_master - 64.0 / 127.0).abs() < 1e-4);
        // learn mode sees native input too
        let mut rx = e.out.subscribe();
        e.handle(Command::LearnStart { binding: None }).unwrap();
        e.on_io(IoEvent::Midi { port: "Native Pad".into(), bytes: vec![0x90, 40, 100] });
        let mut learned = false;
        while let Ok(m) = rx.try_recv() {
            if let Out::Json(j) = m {
                learned |= matches!(serde_json::from_str::<ServerMsg>(&j), Ok(ServerMsg::Learned { .. }));
            }
        }
        assert!(learned);
    }

    #[test]
    fn midi_ports_message_and_enable_without_native_support() {
        let mut e = engine();
        // port-list change events are re-broadcast as ServerMsg::MidiPorts
        let mut rx = e.out.subscribe();
        e.on_io(IoEvent::MidiPorts { inputs: vec![], outputs: vec![] });
        let mut got = None;
        while let Ok(m) = rx.try_recv() {
            if let Out::Json(j) = m {
                if let Ok(ServerMsg::MidiPorts { native, .. }) = serde_json::from_str::<ServerMsg>(&j) {
                    got = Some(native);
                }
            }
        }
        assert_eq!(got, Some(false)); // test engines use the inert hub
        // the initial snapshot carries the port message too
        let (r, mut rrx) = oneshot::channel();
        e.on_in(EngineIn::Snapshot { reply: r });
        let snap = rrx.try_recv().unwrap();
        assert!(snap.iter().any(|j| j.contains("\"midi_ports\"")), "{snap:?}");
        // enabling a port without native MIDI is a clean error, not a panic
        assert!(e.handle(Command::MidiEnable { output: false, name: "x".into(), enabled: true }).is_err());
    }

    #[test]
    fn binding_edge_trigger_and_levels() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: String::new(), release: true, refs: true }).unwrap();
        e.handle(Command::ExecSave { executor: Executor { id: 0, page: 0, slot: 0, label: "A".into(), kind: ExecKind::Cuelist { cuelist: 1 } } }).unwrap();
        e.handle(Command::BindingSave {
            binding: Binding {
                id: 0,
                label: String::new(),
                enabled: true,
                source: Source::Midi { port: None, kind: MidiKind::Note, channel: None, number: 36 },
                action: Action::BlackoutToggle,
                out_min: 0.0,
                out_max: 1.0,
            },
        })
        .unwrap();
        e.handle(Command::BindingSave {
            binding: Binding {
                id: 0,
                label: String::new(),
                enabled: true,
                source: Source::Midi { port: None, kind: MidiKind::Cc, channel: Some(1), number: 7 },
                action: Action::GrandMaster,
                out_min: 0.0,
                out_max: 1.0,
            },
        })
        .unwrap();
        // note on toggles once; held repeats do not retrigger; note off re-arms
        e.midi_in("p", &[0x90, 36, 100]);
        assert!(e.blackout);
        e.midi_in("p", &[0x90, 36, 110]);
        assert!(e.blackout);
        e.midi_in("p", &[0x80, 36, 0]);
        e.midi_in("p", &[0x90, 36, 100]);
        assert!(!e.blackout);
        e.midi_in("p", &[0xB0, 7, 64]);
        assert!((e.grand_master - 64.0 / 127.0).abs() < 1e-4);
        // wrong channel ignored
        e.midi_in("p", &[0xB1, 7, 0]);
        assert!(e.grand_master > 0.4);
    }

    #[test]
    fn learn_captures_next_input() {
        let mut e = engine();
        let mut rx = e.out.subscribe();
        e.handle(Command::LearnStart { binding: None }).unwrap();
        e.midi_in("pad", &[0x90, 40, 0]); // note-off ignored
        assert!(e.learn.is_some());
        e.midi_in("pad", &[0xB2, 21, 33]);
        assert!(e.learn.is_none());
        match rx.try_recv() {
            Ok(Out::Json(s)) => assert!(s.contains("\"learned\"") && s.contains("\"number\":21"), "{s}"),
            _ => panic!("no learned message"),
        }
    }

    #[test]
    fn effects_modulate_dimmer() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 2);
        for f in [1, 2] {
            set(&mut e, f, "dimmer", 1.0);
        }
        e.handle(Command::EffectSave {
            effect: Effect { attr2: None, speed_master: None, bpm: 0.0, rotation: 0.0, id: 0, name: "pulse".into(), attr: "dimmer".into(), shape: Shape::Square, rate: 1.0, depth: 1.0, phase_spread: 360.0, fixtures: vec![1, 2] },
        })
        .unwrap();
        e.handle(Command::EffectToggle { id: 1, on: true }).unwrap();
        e.render(0.1); // phase 0.1 → square +1 for fixture 1; fixture 2 offset half cycle → −1
        let f = **e.frames.get(&1).unwrap();
        assert_eq!((f[0], f[1]), (255, 0));
    }

    #[test]
    fn palette_record_and_apply() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "4ch", 2);
        for f in [1, 2] {
            set(&mut e, f, "red", 1.0);
        }
        set(&mut e, 1, "green", 0.2);
        set(&mut e, 2, "green", 0.6);
        e.handle(Command::ProgRecordPalette { id: None, fixtures: vec![1, 2], kind: PaletteKind::Color, name: "Warm".into() }).unwrap();
        let p = &e.show.palettes[0];
        assert_eq!(p.values["red"], 1.0);
        assert!(!p.values.contains_key("green"));
        assert_eq!(p.per_fixture[&2]["green"], 0.6);
        e.handle(Command::ProgRelease { fixtures: None, attrs: None }).unwrap();
        e.handle(Command::ProgApplyPalette { fixtures: vec![1, 2], palette: 1 }).unwrap();
        assert_eq!(e.programmer[&1]["green"], 0.2);
        assert_eq!(e.programmer[&2]["red"], 1.0);
    }

    #[test]
    fn show_save_load_roundtrip() {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "4ch", 2);
        e.handle(Command::ShowSave { name: Some("Test A".into()) }).unwrap();
        e.handle(Command::ShowNew { name: "Blank".into() }).unwrap();
        assert!(e.show.fixtures.is_empty());
        e.handle(Command::ShowLoad { name: "Test A".into() }).unwrap();
        assert_eq!(e.show.fixtures.len(), 2);
    }

    #[test]
    fn timecode_cue_trigger() {
        let mut e = engine();
        add(&mut e, "generic/dimmer", "1ch", 1);
        set(&mut e, 1, "dimmer", 1.0);
        e.handle(Command::ProgRecordCue { cuelist: 1, number: None, mode: RecordMode::Merge, label: String::new(), release: true, refs: true }).unwrap();
        e.show.cuelists[0].cues[0].timecode = Some("00:00:01:00".into());
        e.show.settings.timecode = TimecodeCfg { source: TcSource::Internal, fps: 25.0, offset_frames: 0 };
        e.rebuild_derived();
        e.handle(Command::TimecodeCtl { running: true, frames: Some(0) }).unwrap();
        e.update_timecode(0.5, 0.5);
        assert!(!e.runs.contains_key(&1));
        e.update_timecode(1.5, 1.0);
        assert_eq!(e.runs[&1].current, Some(1.0));
    }
}
