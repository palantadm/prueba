//! Shared data model — the contract between engine, I/O and the web UI.
//!
//! TypeScript types are generated from these definitions by ts-rs
//! (`cargo test export_bindings` → `ui/src/gen/*.ts`). Never hand-edit the
//! generated files; change this file and regenerate.
//!
//! Conventions
//! * Attribute values are normalised `f32` in `0.0..=1.0` (16-bit resolution is
//!   handled by the profile's `fine` channel). Attribute names are the strings in
//!   [`ATTRS`].
//! * `Id` collections are keyed by numeric ids chosen by the server (fixtures use
//!   the user-visible fixture number).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub type Attrs = BTreeMap<String, f32>;
pub type PerFixture = BTreeMap<u32, Attrs>;

/// JSON object keys are strings; serde's internally-tagged enums (`Command`) buffer input and
/// cannot coerce them to `u32`, so integer-keyed maps are parsed explicitly.
pub fn de_per_fixture<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PerFixture, D::Error> {
    let raw = BTreeMap::<String, Attrs>::deserialize(d)?;
    raw.into_iter()
        .map(|(k, v)| k.parse::<u32>().map(|k| (k, v)).map_err(|_| serde::de::Error::custom(format!("bad fixture id '{k}'"))))
        .collect()
}

/// Canonical attribute vocabulary: (name, group, label).
pub const ATTRS: &[(&str, &str, &str)] = &[
    ("dimmer", "intensity", "Dimmer"),
    ("pan", "position", "Pan"),
    ("tilt", "position", "Tilt"),
    ("red", "color", "Red"),
    ("green", "color", "Green"),
    ("blue", "color", "Blue"),
    ("white", "color", "White"),
    ("amber", "color", "Amber"),
    ("uv", "color", "UV"),
    ("lime", "color", "Lime"),
    ("cto", "color", "CTO"),
    ("colorwheel", "color", "Color wheel"),
    ("gobo", "beam", "Gobo"),
    ("gobo_rot", "beam", "Gobo rot"),
    ("prism", "beam", "Prism"),
    ("prism_rot", "beam", "Prism rot"),
    ("frost", "beam", "Frost"),
    ("zoom", "beam", "Zoom"),
    ("focus", "beam", "Focus"),
    ("iris", "beam", "Iris"),
    ("shutter", "beam", "Shutter/Strobe"),
    ("speed", "control", "Speed"),
    ("effect", "control", "Effect"),
    ("control", "control", "Control"),
];

/// Normalised default ("home") value for an attribute that a profile does not override.
pub fn attr_default(attr: &str) -> f32 {
    match attr {
        "pan" | "tilt" => 0.5,
        _ => 0.0,
    }
}

// ───────────────────────────── Fixture library ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Capability {
    pub min: u8,
    pub max: u8,
    pub label: String,
    /// `#rrggbb` swatch for colour-wheel slots.
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChannelDef {
    pub attr: String,
    /// 0-based DMX offset from the fixture start address (MSB / only channel).
    pub offset: u16,
    /// 0-based offset of the LSB channel for 16-bit attributes.
    #[serde(default)]
    pub fine: Option<u16>,
    /// Normalised home value used when nothing drives the attribute.
    #[serde(default)]
    pub default: f32,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Mode {
    pub name: String,
    pub channels: Vec<ChannelDef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FixtureProfile {
    /// Unique id, e.g. `generic/rgb-par`.
    pub id: String,
    pub manufacturer: String,
    pub model: String,
    /// dimmer | par | wash | spot | beam | bar | strobe | effect | other
    pub category: String,
    pub modes: Vec<Mode>,
    /// Beam angle in degrees (min, max) — drives the 3D visualiser.
    #[serde(default = "default_beam")]
    pub beam_angle: [f32; 2],
    /// Total pan travel in degrees (0 = fixed).
    #[serde(default)]
    pub pan_range: f32,
    #[serde(default)]
    pub tilt_range: f32,
    #[serde(default)]
    pub power_w: f32,
}

fn default_beam() -> [f32; 2] {
    [25.0, 25.0]
}

// ───────────────────────────── Show model ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Fixture {
    pub id: u32,
    pub name: String,
    pub profile: String,
    pub mode: String,
    /// Lumos universe id (see [`UniverseCfg`]).
    pub universe: u16,
    /// 1..=512
    pub address: u16,
    /// Position on stage in metres (x = stage right, y = up, z = downstage).
    #[serde(default)]
    pub pos: [f32; 3],
    /// Rotation in degrees (pitch about x, yaw about y, roll about z).
    #[serde(default)]
    pub rot: [f32; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Group {
    pub id: u32,
    pub name: String,
    pub fixtures: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PaletteKind {
    Intensity,
    Position,
    Color,
    Beam,
    All,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Palette {
    pub id: u32,
    pub name: String,
    pub kind: PaletteKind,
    /// Universal values applied to every fixture that has the attribute.
    #[serde(default)]
    pub values: Attrs,
    /// Optional per-fixture overrides (take precedence over `values`).
    #[serde(default, deserialize_with = "de_per_fixture")]
    pub per_fixture: PerFixture,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Cue {
    /// Cue number, e.g. 1, 2, 2.5. Unique within a cuelist, ordering key.
    pub number: f64,
    #[serde(default)]
    pub label: String,
    /// Seconds.
    #[serde(default = "d_fade")]
    pub fade_in: f32,
    #[serde(default = "d_fade")]
    pub fade_out: f32,
    #[serde(default)]
    pub delay: f32,
    /// Auto-follow: run the next cue this many seconds after this one starts.
    #[serde(default)]
    pub follow: Option<f32>,
    /// `HH:MM:SS:FF` — GO when show timecode reaches this value.
    #[serde(default)]
    pub timecode: Option<String>,
    /// Recorded values. In tracking lists only *changes* are stored.
    #[serde(default, deserialize_with = "de_per_fixture")]
    pub changes: PerFixture,
    /// Block: stop tracking from earlier cues (stores full state).
    #[serde(default)]
    pub block: bool,
}

fn d_fade() -> f32 {
    2.0
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Cuelist {
    pub id: u32,
    pub name: String,
    pub cues: Vec<Cue>,
    #[serde(default)]
    pub looped: bool,
    #[serde(default = "d_true")]
    pub tracking: bool,
    /// Higher priority wins on LTP conflicts between cuelists.
    #[serde(default)]
    pub priority: u8,
}

fn d_true() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MasterKind {
    Grand,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ExecKind {
    Empty,
    /// Fader = cuelist intensity master, GO/BACK/FLASH buttons.
    Cuelist { cuelist: u32 },
    /// Fader = dimmer master for the group's fixtures.
    Group { group: u32 },
    /// Button toggles the effect, fader = effect depth multiplier.
    Effect { effect: u32 },
    Master { master: MasterKind },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Executor {
    pub id: u32,
    pub page: u8,
    /// 0-based slot within the page.
    pub slot: u8,
    pub label: String,
    pub kind: ExecKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Shape {
    Sine,
    Triangle,
    Saw,
    Square,
    Random,
}

/// Parametric effect. `attr` is any attribute name, or `"hue"` (rainbow across RGB fixtures).
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Effect {
    pub id: u32,
    pub name: String,
    pub attr: String,
    pub shape: Shape,
    /// Cycles per second.
    pub rate: f32,
    /// Amplitude 0..1 (full range = 1).
    pub depth: f32,
    /// Phase offset in degrees spread evenly across `fixtures` (0 = in unison).
    pub phase_spread: f32,
    pub fixtures: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MidiKind {
    Note,
    Cc,
    Program,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Source {
    /// `channel` 1..=16, `None` = any. `port` `None` = any.
    Midi {
        port: Option<String>,
        kind: MidiKind,
        channel: Option<u8>,
        number: u8,
    },
    Osc { address: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Action {
    ExecLevel { exec: u32 },
    ExecGo { exec: u32 },
    ExecBack { exec: u32 },
    ExecFlash { exec: u32 },
    Go { cuelist: u32 },
    Back { cuelist: u32 },
    GrandMaster,
    BlackoutToggle,
    Dmx { universe: u16, channel: u16 },
    FixtureAttr { fixture: u32, attr: String },
    GroupAttr { group: u32, attr: String },
    EffectToggle { effect: u32 },
    PaletteApply { palette: u32, group: Option<u32> },
    ProgClear,
}

/// Input → action mapping. Continuous sources deliver a normalised value
/// (MIDI `/127`, OSC float clamped 0..1). Trigger actions fire on the rising
/// edge through 0.5.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Binding {
    pub id: u32,
    #[serde(default)]
    pub label: String,
    #[serde(default = "d_true")]
    pub enabled: bool,
    pub source: Source,
    pub action: Action,
    #[serde(default)]
    pub out_min: f32,
    #[serde(default = "d_one")]
    pub out_max: f32,
}

fn d_one() -> f32 {
    1.0
}

// ───────────────────────────── I/O configuration ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OscMode {
    PerChannel,
    Blob,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SerialDriver {
    /// FTDI "Open DMX" style: raw 250 kbaud 8N2 with break by baud trick.
    EnttecOpen,
    /// Enttec DMX USB Pro / DMXking ultraDMX Micro/Pro (label 6 widget protocol).
    EnttecPro,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum OutputKind {
    /// `host` empty = broadcast. Port-address = net(7) / subnet(4) / universe(4 bits).
    ArtNet {
        host: String,
        port: u16,
        net: u8,
        subnet: u8,
        universe: u8,
    },
    Sacn {
        universe: u16,
        priority: u8,
        /// `None` = multicast `239.255.hi.lo`.
        unicast: Option<String>,
    },
    Osc {
        host: String,
        port: u16,
        /// Placeholders: `{universe}` `{channel}`.
        pattern: String,
        mode: OscMode,
        only_nonzero: bool,
    },
    Serial { driver: SerialDriver, port: String },
    /// Raw USB (uDMX etc.) — only when built with the `usb` feature.
    Usb { driver: String, serial: Option<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputCfg {
    pub enabled: bool,
    pub kind: OutputKind,
}

/// A logical universe. One universe may drive several outputs at once.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UniverseCfg {
    pub id: u16,
    pub name: String,
    #[serde(default)]
    pub outputs: Vec<OutputCfg>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OscInCfg {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArtNetInMap {
    pub net: u8,
    pub subnet: u8,
    pub universe: u8,
    /// Lumos universe the data is merged (HTP) into.
    pub target: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArtNetInCfg {
    pub enabled: bool,
    pub map: Vec<ArtNetInMap>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TcSource {
    Off,
    /// MIDI Time Code (quarter-frame / full-frame) received via MIDI input.
    Mtc,
    /// Free-running internal clock started/stopped from the UI.
    Internal,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimecodeCfg {
    pub source: TcSource,
    /// 24 | 25 | 29.97 | 30
    pub fps: f32,
    pub offset_frames: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OscFeedbackCfg {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Settings {
    /// Node name (Art-Net poll reply, sACN source name).
    pub node_name: String,
    /// Engine output rate. 44 = DMX512 maximum with full frames.
    pub tick_hz: u32,
    pub osc_in: OscInCfg,
    pub artnet_in: ArtNetInCfg,
    pub timecode: TimecodeCfg,
    pub osc_feedback: OscFeedbackCfg,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            node_name: "Lumos".into(),
            tick_hz: 44,
            osc_in: OscInCfg { enabled: true, port: 7700 },
            artnet_in: ArtNetInCfg { enabled: false, map: vec![] },
            timecode: TimecodeCfg { source: TcSource::Off, fps: 25.0, offset_frames: 0 },
            osc_feedback: OscFeedbackCfg { enabled: false, host: "127.0.0.1".into(), port: 9000 },
        }
    }
}

// ───────────────────────────── Stage (3D) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StageKind {
    Truss,
    Platform,
    Wall,
    Column,
    Curtain,
    Tree,
    Box,
    /// Projection surface or LED panel — uses `media`.
    Screen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MediaKind {
    None,
    Color,
    Image,
    Video,
    /// MJPEG / HLS / any URL the browser can play.
    Url,
    Webcam,
    /// Browser screen-capture (e.g. an NDI Monitor / Resolume window).
    Screen,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MediaSource {
    pub kind: MediaKind,
    /// Colour `#rrggbb`, or URL / `/media/<file>` for image/video/url.
    #[serde(default)]
    pub src: String,
    #[serde(default = "d_true")]
    pub looped: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StageObject {
    pub id: u32,
    pub kind: StageKind,
    pub name: String,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    /// Width, height, depth in metres (truss: length in `size[0]`).
    pub size: [f32; 3],
    pub color: String,
    #[serde(default)]
    pub media: Option<MediaSource>,
    /// `projection` | `led` (screens only).
    #[serde(default)]
    pub screen_type: Option<String>,
    #[serde(default = "d_one")]
    pub brightness: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Stage {
    /// Floor width (x), depth (z) in metres, and grid height (y).
    pub size: [f32; 3],
    pub objects: Vec<StageObject>,
}

impl Default for Stage {
    fn default() -> Self {
        Stage { size: [12.0, 8.0, 6.0], objects: vec![] }
    }
}

// ───────────────────────────── Show ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Show {
    pub name: String,
    pub universes: Vec<UniverseCfg>,
    pub fixtures: Vec<Fixture>,
    pub groups: Vec<Group>,
    pub palettes: Vec<Palette>,
    pub cuelists: Vec<Cuelist>,
    pub executors: Vec<Executor>,
    pub effects: Vec<Effect>,
    pub bindings: Vec<Binding>,
    pub stage: Stage,
    pub settings: Settings,
    /// Profiles imported into / customised in this show (merged over the built-in library).
    #[serde(default)]
    pub profiles: Vec<FixtureProfile>,
}

impl Default for Show {
    fn default() -> Self {
        Show {
            name: "Untitled".into(),
            universes: (1..=4)
                .map(|id| UniverseCfg { id, name: format!("Universe {id}"), outputs: vec![] })
                .collect(),
            fixtures: vec![],
            groups: vec![],
            palettes: vec![],
            cuelists: vec![],
            executors: vec![],
            effects: vec![],
            bindings: vec![],
            stage: Stage::default(),
            settings: Settings::default(),
            profiles: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ShowInfo {
    pub name: String,
    /// Unix seconds.
    pub modified: u32,
}

// ───────────────────────────── Runtime (not persisted) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PlayState {
    Idle,
    Running,
    Paused,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CuelistRt {
    pub id: u32,
    pub state: PlayState,
    /// Currently active cue number.
    pub current: Option<f64>,
    /// Next cue number a GO would run.
    pub next: Option<f64>,
    /// 0..1 progress of the running fade.
    pub progress: f32,
    /// Cuelist intensity master 0..1.
    pub level: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ExecRt {
    pub id: u32,
    pub level: f32,
    pub active: bool,
    pub flash: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OutState {
    Off,
    Ok,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputStatus {
    pub universe: u16,
    /// Index into `UniverseCfg.outputs`.
    pub index: u8,
    pub state: OutState,
    pub message: String,
    pub frames: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InputStatus {
    pub osc_listening: bool,
    pub osc_port: u16,
    pub osc_error: String,
    pub artnet_listening: bool,
    pub artnet_error: String,
    /// Last few input events for the activity monitor (newest first).
    pub recent: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimecodeRt {
    pub running: bool,
    pub frames: u32,
    /// `HH:MM:SS:FF`
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Runtime {
    pub grand_master: f32,
    pub blackout: bool,
    /// Values currently held in the programmer (per fixture).
    pub programmer: PerFixture,
    pub executors: Vec<ExecRt>,
    pub cuelists: Vec<CuelistRt>,
    pub effects_running: Vec<u32>,
    pub outputs: Vec<OutputStatus>,
    pub inputs: InputStatus,
    pub timecode: TimecodeRt,
    /// Binding id currently in learn mode (`0` = new binding).
    pub learn: Option<u32>,
    /// Measured engine rate.
    pub fps: f32,
}

// ───────────────────────────── Commands (client → server) ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RecordMode {
    /// Merge programmer into the cue (overwrite touched attributes).
    Merge,
    /// Replace the cue contents with the programmer.
    Replace,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Command {
    // ── programmer ──
    ProgSet { fixtures: Vec<u32>, attrs: Attrs },
    /// `None` fixtures = all; `None` attrs = every attribute.
    ProgRelease { fixtures: Option<Vec<u32>>, attrs: Option<Vec<String>> },
    ProgApplyPalette { fixtures: Vec<u32>, palette: u32 },
    /// Create/update a palette from the programmer values of `fixtures`
    /// (only attributes belonging to `kind`). `id: None` creates.
    ProgRecordPalette { id: Option<u32>, fixtures: Vec<u32>, kind: PaletteKind, name: String },
    /// `number: None` appends a new cue after the last one.
    ProgRecordCue {
        cuelist: u32,
        number: Option<f64>,
        mode: RecordMode,
        label: String,
        release: bool,
    },
    // ── patch ──
    /// Adds `count` fixtures; `address: None` = first free address from universe start.
    FixtureAdd {
        profile: String,
        mode: String,
        count: u32,
        universe: u16,
        address: Option<u16>,
        name: Option<String>,
    },
    FixtureUpdate { fixture: Fixture },
    FixtureRemove { ids: Vec<u32> },
    // ── library ──
    /// Open Fixture Library (OFL) JSON text. Adds the profile to the show.
    LibraryImportOfl { json: String },
    // ── pools ──
    GroupSave { group: Group },
    GroupRemove { id: u32 },
    PaletteSave { palette: Palette },
    PaletteRemove { id: u32 },
    EffectSave { effect: Effect },
    EffectRemove { id: u32 },
    EffectToggle { id: u32, on: bool },
    // ── cuelists ──
    CuelistSave { cuelist: Cuelist },
    CuelistRemove { id: u32 },
    CueSave { cuelist: u32, cue: Cue },
    CueRemove { cuelist: u32, number: f64 },
    // ── playback ──
    Go { cuelist: u32 },
    Back { cuelist: u32 },
    GoTo { cuelist: u32, number: f64 },
    Pause { cuelist: u32 },
    ReleaseCuelist { cuelist: u32 },
    ExecSave { executor: Executor },
    ExecRemove { id: u32 },
    ExecLevel { id: u32, level: f32 },
    ExecGo { id: u32 },
    ExecBack { id: u32 },
    ExecFlash { id: u32, down: bool },
    // ── masters / direct ──
    GrandMaster { level: f32 },
    Blackout { on: bool },
    /// Raw channel override on top of everything (DESK faders). `value: None` releases.
    DmxSet { universe: u16, channel: u16, value: Option<u8> },
    // ── I/O ──
    UniverseSave { universe: UniverseCfg },
    UniverseRemove { id: u16 },
    SettingsSave { settings: Settings },
    BindingSave { binding: Binding },
    BindingRemove { id: u32 },
    /// Start learn mode; the next MIDI/OSC input is broadcast as `ServerMsg::Learned`.
    LearnStart { binding: Option<u32> },
    LearnStop,
    /// Raw MIDI message captured by the browser (Web MIDI).
    MidiIn { port: String, bytes: Vec<u8> },
    TimecodeCtl { running: bool, frames: Option<u32> },
    // ── stage ──
    StageSave { stage: Stage },
    // ── show management ──
    ShowNew { name: String },
    ShowSave { name: Option<String> },
    ShowLoad { name: String },
    ShowRename { name: String },
    // ── streaming ──
    /// Subscribe to binary DMX frames (`None` = all universes, `Some([])` = none).
    SubscribeDmx { universes: Option<Vec<u16>> },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClientMsg {
    /// Always `"cmd"`.
    pub t: String,
    /// Echoed in the `ack`.
    pub id: String,
    pub cmd: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "t", rename_all = "snake_case")]
#[ts(export)]
pub enum ServerMsg {
    Hello { version: String },
    /// Full show document. Sent on connect and (coalesced ≤10/s) after every model change.
    Show { rev: u32, show: Show },
    /// Built-in + show profiles. Sent on connect and when the library changes.
    Library { profiles: Vec<FixtureProfile> },
    /// Live state, ≤20/s while changing, plus 1/s keep-alive.
    Rt { rt: Runtime },
    Ack { id: String, ok: bool, err: Option<String> },
    Learned { source: Source },
    /// Saved shows list. Sent on connect and after save/delete.
    Shows { shows: Vec<ShowInfo> },
    Log { level: String, msg: String },
}

/// Binary frame layout (server → client), sent only to subscribers:
/// `[0x01][universe: u16 BE][512 × u8]` — final merged DMX of one universe, ≤30 Hz,
/// only when the universe changed (plus 1 Hz refresh).
pub const BIN_DMX: u8 = 0x01;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bindings() {
        // ts-rs exports on `cargo test`; this test also checks the round-trip of the default show.
        let s = Show::default();
        let j = serde_json::to_string(&s).unwrap();
        let back: Show = serde_json::from_str(&j).unwrap();
        assert_eq!(back.universes.len(), 4);
        let c: ClientMsg =
            serde_json::from_str(r#"{"t":"cmd","id":"1","cmd":{"type":"go","cuelist":1}}"#).unwrap();
        assert!(matches!(c.cmd, Command::Go { cuelist: 1 }));
        // integer-keyed maps inside internally-tagged enums
        let c: ClientMsg = serde_json::from_str(
            r#"{"t":"cmd","id":"2","cmd":{"type":"cue_save","cuelist":1,"cue":{"number":1,"changes":{"3":{"dimmer":1.0}}}}}"#,
        )
        .unwrap();
        match c.cmd {
            Command::CueSave { cue, .. } => assert_eq!(cue.changes[&3]["dimmer"], 1.0),
            _ => panic!(),
        }
        let c: ClientMsg = serde_json::from_str(
            r#"{"t":"cmd","id":"3","cmd":{"type":"cuelist_save","cuelist":{"id":1,"name":"L","cues":[{"number":1,"label":"","fade_in":2,"fade_out":2,"delay":0,"follow":null,"timecode":null,"block":false,"changes":{"1":{"dimmer":1},"12":{"red":0.5}}}],"looped":false,"tracking":true,"priority":0}}}"#,
        )
        .unwrap();
        match c.cmd {
            Command::CuelistSave { cuelist } => assert_eq!(cuelist.cues[0].changes[&12]["red"], 0.5),
            _ => panic!(),
        }
        let c: ClientMsg = serde_json::from_str(
            r#"{"t":"cmd","id":"4","cmd":{"type":"palette_save","palette":{"id":0,"name":"P","kind":"color","values":{"red":1},"per_fixture":{"2":{"green":0.6}}}}}"#,
        )
        .unwrap();
        match c.cmd {
            Command::PaletteSave { palette } => assert_eq!(palette.per_fixture[&2]["green"], 0.6),
            _ => panic!(),
        }
        assert!(serde_json::from_str::<ClientMsg>(
            r#"{"t":"cmd","id":"5","cmd":{"type":"palette_save","palette":{"id":0,"name":"P","kind":"color","per_fixture":{"x":{}}}}}"#
        )
        .is_err());
    }
}
