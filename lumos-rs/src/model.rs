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

/// fixture → attribute → palette id: the value is *linked* to a palette (edit the palette, every
/// cue/scene using it follows). The absolute value is also stored as a fallback.
pub type PerFixtureRefs = BTreeMap<u32, BTreeMap<String, u32>>;

pub fn de_per_fixture_refs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PerFixtureRefs, D::Error> {
    let raw = BTreeMap::<String, BTreeMap<String, u32>>::deserialize(d)?;
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
    /// Gobo / slot image URL (e.g. `/library/<profile>/wheels/gobo1.png`), when the source provides one.
    #[serde(default)]
    pub image: Option<String>,
    /// Wheel name and 1-based slot index this capability selects (GDTF), for the visualiser.
    #[serde(default)]
    pub wheel: Option<String>,
    #[serde(default)]
    pub slot: Option<u32>,
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
    /// Where the profile came from: `builtin` | `gdtf` | `ofl` | `qxf`.
    #[serde(default)]
    pub source: Option<String>,
    /// Real geometry (GDTF): 3D models, axes, beam optics, wheels — drives the realistic visualiser.
    #[serde(default)]
    pub model3d: Option<Model3d>,
}

/// Geometry + optics of a fixture, converted from GDTF. Units: metres, degrees.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Model3d {
    /// Base URL of extracted assets, e.g. `/library/<profile id>/` (ends with `/`).
    pub assets: String,
    pub root: GeoNode,
    pub wheels: Vec<Wheel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum BeamType {
    Wash,
    Spot,
    None,
    Rectangle,
    Pc,
    Fresnel,
    Glow,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BeamInfo {
    /// Beam angle (bright cone) and field angle (outer cone), degrees, at zoom min.
    pub beam_angle: f32,
    pub field_angle: f32,
    /// Angles at zoom max (equal to the above if the fixture has no zoom).
    pub beam_angle_max: f32,
    pub field_angle_max: f32,
    /// Lens radius in metres.
    pub radius: f32,
    pub beam_type: BeamType,
    pub lamp: String,
    pub power_w: f32,
    pub luminous_flux: f32,
    pub color_temp: f32,
}

/// One node of the GDTF geometry tree.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GeoNode {
    pub name: String,
    /// Attribute that rotates this node: `pan` | `tilt` (GDTF Axis geometries) or `None`.
    #[serde(default)]
    pub axis: Option<String>,
    /// Model file relative to `assets` (`.glb`/`.gltf`/`.3ds`), when the geometry has one.
    #[serde(default)]
    pub model: Option<String>,
    /// Fallback primitive when no model file: `Cube` | `Cylinder` | `Sphere` | `Conventional`, with size w,h,d metres.
    #[serde(default)]
    pub primitive: Option<String>,
    pub size: [f32; 3],
    /// Local transform relative to parent, column-major 4×4 (three.js `Matrix4.fromArray`), metres.
    pub matrix: [f32; 16],
    #[serde(default)]
    pub beam: Option<BeamInfo>,
    pub children: Vec<GeoNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WheelSlot {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    /// Gobo image, relative to `Model3d.assets`.
    #[serde(default)]
    pub image: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Wheel {
    pub name: String,
    pub slots: Vec<WheelSlot>,
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
    /// Palette links for values in `changes` (resolved at render; `changes` is the fallback).
    #[serde(default, deserialize_with = "de_per_fixture_refs")]
    pub refs: PerFixtureRefs,
    /// Per attribute-group timing overrides. Keys: `intensity` `position` `color` `beam` `control`.
    #[serde(default)]
    pub timing: BTreeMap<String, AttrTiming>,
    #[serde(default)]
    pub curve: FadeCurve,
    /// Seconds of extra delay added per fixture, in fixture-number order (0 = all together).
    #[serde(default)]
    pub stagger: f32,
    #[serde(default)]
    pub notes: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AttrTiming {
    pub delay: f32,
    pub fade: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum FadeCurve {
    #[default]
    Linear,
    SCurve,
    Square,
    InvSquare,
    Sine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateMode {
    /// Merge programmer values into the cue.
    Merge,
    /// Replace the cue's values with the programmer.
    Replace,
    /// Also push the changes forward through following cues until they re-state the attribute.
    TrackForward,
    /// Change only this cue (following cues keep their old resulting look).
    CueOnly,
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
    /// Auto-mark ("move on dark"): pre-position non-intensity attributes of fixtures that are dark
    /// in the current cue and lit in the next one, while the current cue plays.
    #[serde(default)]
    pub auto_mark: bool,
}

fn d_true() -> bool {
    true
}

/// A recorded look (snapshot of attribute values) that can be recalled with a fade,
/// layered with a level master, or morphed into another scene.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Scene {
    pub id: u32,
    pub name: String,
    #[serde(default, deserialize_with = "de_per_fixture")]
    pub values: PerFixture,
    /// Seconds to fade in when activated / out when released.
    #[serde(default = "d_scene_fade")]
    pub fade_in: f32,
    #[serde(default = "d_scene_fade")]
    pub fade_out: f32,
    /// Accent colour `#rrggbb` for pool buttons.
    #[serde(default)]
    pub color: String,
    #[serde(default, deserialize_with = "de_per_fixture_refs")]
    pub refs: PerFixtureRefs,
    /// Submaster behaviour.
    #[serde(default)]
    pub kind: SceneKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SceneKind {
    /// Full look; level blends all attributes.
    #[default]
    Look,
    /// Only `dimmer` is taken from the scene (level scales it); other attributes are ignored.
    Intensity,
    /// Level caps the dimmer of the scene's fixtures (inhibitive submaster); values ignored.
    Inhibitive,
}

fn d_scene_fade() -> f32 {
    1.0
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
    /// Fader = scene level, button toggles the scene with its fade times.
    Scene { scene: u32 },
    /// Button runs/stops the chase; fader = chase size/level.
    Chase { chase: u32 },
    /// Fader sets the speed master BPM (range 20..240); button taps tempo.
    Speed { master: u32 },
    /// Manual crossfade: fader drives the running cue's fade progress (0..1); at the ends it completes.
    XFade { cuelist: u32 },
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
    /// 2-D shapes drive `attr` (x) and `attr2` (y), e.g. pan/tilt.
    Circle,
    Eight,
    Ramp,
    Pulse,
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
    /// Second attribute for 2-D shapes (Circle/Eight), usually `tilt` with `attr = pan`.
    #[serde(default)]
    pub attr2: Option<String>,
    /// Follow a speed master (BPM) instead of `rate`.
    #[serde(default)]
    pub speed_master: Option<u32>,
    /// >0: rate in BPM (1 cycle per beat) overriding `rate`.
    #[serde(default)]
    pub bpm: f32,
    /// Rotation of 2-D shapes in degrees.
    #[serde(default)]
    pub rotation: f32,
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
    #[serde(default)]
    pub msc: MscCfg,
    /// Start with blackout engaged (safe default for rehearsal PCs on a live rig).
    #[serde(default)]
    pub start_blackout: bool,
}

/// MIDI Show Control (theatre standard): lighting commands received as SysEx.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MscCfg {
    pub enabled: bool,
    /// MSC device id 0..=111, 127 = all-call.
    pub device_id: u8,
    /// Maps the MSC cue-list id (as text, e.g. "1") to a Lumos cuelist.
    pub map: Vec<MscMap>,
}

impl Default for MscCfg {
    fn default() -> Self {
        MscCfg { enabled: false, device_id: 127, map: vec![] }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MscMap {
    pub msc_list: String,
    pub cuelist: u32,
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
            msc: MscCfg::default(),
            start_blackout: false,
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

// ───────────────────────────── Chases, speed masters, macros, timelines ─────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChaseDir {
    #[default]
    Forward,
    Backward,
    Bounce,
    Random,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChaseStep {
    #[serde(default, deserialize_with = "de_per_fixture")]
    pub values: PerFixture,
    #[serde(default, deserialize_with = "de_per_fixture_refs")]
    pub refs: PerFixtureRefs,
    /// Use a recorded scene as the step content (in addition to `values`).
    #[serde(default)]
    pub scene: Option<u32>,
    /// Beats this step lasts.
    #[serde(default = "d_one")]
    pub hold: f32,
    /// Crossfade as a fraction of the step (0 = snap, 1 = continuous).
    #[serde(default)]
    pub fade: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Chase {
    pub id: u32,
    pub name: String,
    pub steps: Vec<ChaseStep>,
    #[serde(default = "d_bpm")]
    pub bpm: f32,
    #[serde(default)]
    pub speed_master: Option<u32>,
    #[serde(default)]
    pub dir: ChaseDir,
}

fn d_bpm() -> f32 {
    120.0
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpeedMaster {
    pub id: u32,
    pub name: String,
    pub bpm: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MacroStep {
    /// A command-line statement (see the command-line language in the engine).
    pub text: String,
    /// Seconds to wait after this step.
    #[serde(default)]
    pub wait: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Macro {
    pub id: u32,
    pub name: String,
    pub steps: Vec<MacroStep>,
    #[serde(default)]
    pub color: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TlEvent {
    /// Seconds from the timeline start.
    pub at: f32,
    /// Command-line statement executed when the playhead passes `at`.
    pub text: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Timeline {
    pub id: u32,
    pub name: String,
    pub length: f32,
    pub events: Vec<TlEvent>,
    #[serde(default)]
    pub looped: bool,
    /// When set, the playhead follows show timecode: `t = timecode − tc_start`.
    #[serde(default)]
    pub tc_start: Option<String>,
    /// Optional audio file (`/media/<file>`) the editor shows as a waveform.
    #[serde(default)]
    pub audio: Option<String>,
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
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub chases: Vec<Chase>,
    #[serde(default)]
    pub speed_masters: Vec<SpeedMaster>,
    #[serde(default)]
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub timelines: Vec<Timeline>,
    pub cuelists: Vec<Cuelist>,
    pub executors: Vec<Executor>,
    pub effects: Vec<Effect>,
    pub bindings: Vec<Binding>,
    pub stage: Stage,
    pub settings: Settings,
    /// Profiles imported into / customised in this show (merged over the built-in library).
    #[serde(default)]
    pub profiles: Vec<FixtureProfile>,
    /// Virtual console pages (button/slider/knob/XY widgets with input mapping and feedback).
    #[serde(default)]
    pub vconsole: Vec<VcPage>,
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
            scenes: vec![],
            chases: vec![],
            speed_masters: vec![],
            macros: vec![],
            timelines: vec![],
            cuelists: vec![],
            executors: vec![],
            effects: vec![],
            bindings: vec![],
            stage: Stage::default(),
            settings: Settings::default(),
            profiles: vec![],
            vconsole: vec![],
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

// ───────────────────────────── Virtual console ─────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum VcKind {
    Button,
    Slider,
    Knob,
    /// Two-axis pad driving pan/tilt (or any two attributes) of a group.
    XyPad,
    /// GO / BACK / current cue display for one cuelist.
    CueList,
    /// Tap-tempo / rate dial for an effect.
    SpeedDial,
    Label,
    /// Visual container; widgets drawn over it move with it in the editor.
    Frame,
    Clock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum VcMode {
    /// Active only while pressed (flash).
    Momentary,
    /// Each press toggles.
    Toggle,
    /// Fires once on press.
    Trigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum VcOp {
    Go,
    Back,
    Flash,
    Toggle,
    Release,
    Level,
}

/// What a widget controls.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum VcAction {
    None,
    Exec { exec: u32, op: VcOp },
    Cuelist { cuelist: u32, op: VcOp },
    GrandMaster,
    Blackout,
    Dmx { universe: u16, channel: u16 },
    FixtureAttr { fixtures: Vec<u32>, attr: String },
    GroupAttr { group: u32, attr: String },
    EffectToggle { effect: u32 },
    /// Slider/dial scales an effect's `rate` (Hz) between `out_min..out_max` of the widget.
    EffectRate { effect: u32 },
    PaletteApply { palette: u32, group: Option<u32> },
    ProgClear,
    /// Jump to a virtual-console page.
    Page { page: u32 },
    /// Scene button (Go/Toggle/Flash/Release) or level slider (Level).
    Scene { scene: u32, op: VcOp },
    /// Slider crossfades (morphs) scene `a` → `b`.
    SceneMorph { a: u32, b: u32 },
    /// XY pad: `group` moves `x_attr`/`y_attr` (typically pan/tilt).
    Xy { group: u32, x_attr: String, y_attr: String },
}

/// One physical input mapped onto a widget.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VcInput {
    pub source: Source,
    #[serde(default)]
    pub invert: bool,
    /// Normalised sub-range of the source mapped onto 0..1 (default 0..1).
    #[serde(default)]
    pub in_min: f32,
    #[serde(default = "d_one")]
    pub in_max: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum FeedbackTarget {
    /// MIDI out (relayed through the browser's Web MIDI output). Buttons send `on`/`off`
    /// (use controller-specific velocity as LED colour); sliders/knobs send `0..127` scaled value when `scale`.
    Midi {
        port: Option<String>,
        kind: MidiKind,
        channel: u8,
        number: u8,
        on: u8,
        off: u8,
        scale: bool,
    },
    /// OSC out: buttons send `on`/`off` floats, sliders send the 0..1 value.
    Osc {
        host: String,
        port: u16,
        address: String,
        on: f32,
        off: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VcWidget {
    pub id: u32,
    pub kind: VcKind,
    pub label: String,
    /// Grid position and size in cells.
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    /// Accent colour `#rrggbb`.
    pub color: String,
    pub action: VcAction,
    pub mode: VcMode,
    /// `vertical` | `horizontal` (sliders).
    #[serde(default)]
    pub orientation: String,
    #[serde(default)]
    pub inputs: Vec<VcInput>,
    #[serde(default)]
    pub feedback: Vec<FeedbackTarget>,
    /// Output range for continuous widgets (normalised attribute / rate units).
    #[serde(default)]
    pub out_min: f32,
    #[serde(default = "d_one")]
    pub out_max: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VcPage {
    pub id: u32,
    pub name: String,
    pub cols: u16,
    pub rows: u16,
    pub widgets: Vec<VcWidget>,
}

/// Live state of a widget (what feedback reflects).
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VcState {
    pub id: u32,
    /// 0..1 for continuous widgets.
    pub value: f32,
    pub active: bool,
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
    /// Fade progress is driven by a manual crossfader.
    #[serde(default)]
    pub manual: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ParkedChannel {
    pub universe: u16,
    pub channel: u16,
    pub value: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CmdLogLine {
    pub text: String,
    pub ok: bool,
    pub msg: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChaseRt {
    pub id: u32,
    pub running: bool,
    pub step: u32,
    pub bpm: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpeedRt {
    pub id: u32,
    pub bpm: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimelineRt {
    pub id: u32,
    pub running: bool,
    pub t: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SceneRt {
    pub id: u32,
    /// Effective level 0..1 (follows the fade).
    pub level: f32,
    pub active: bool,
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
    /// Virtual console widget states.
    #[serde(default)]
    pub vc: Vec<VcState>,
    #[serde(default)]
    pub scenes: Vec<SceneRt>,
    /// Active scene morph (a, b, t) if any.
    #[serde(default)]
    pub morph: Option<(u32, u32, f32)>,
    /// Blind mode: the programmer is edited but not sent to the output.
    #[serde(default)]
    pub blind: bool,
    /// Undo / redo steps available.
    #[serde(default)]
    pub undo: u16,
    #[serde(default)]
    pub redo: u16,
    #[serde(default)]
    pub parked: Vec<ParkedChannel>,
    /// Last command-line results (newest last, ≤ 30).
    #[serde(default)]
    pub cmd_log: Vec<CmdLogLine>,
    #[serde(default)]
    pub chases: Vec<ChaseRt>,
    #[serde(default)]
    pub speed: Vec<SpeedRt>,
    #[serde(default)]
    pub timelines: Vec<TimelineRt>,
    #[serde(default)]
    pub macros_running: Vec<u32>,
    /// Output test pattern running.
    #[serde(default)]
    pub testing: bool,
    /// Fixtures currently being identified/located.
    #[serde(default)]
    pub identifying: Vec<u32>,
}

// ───────────────────────────── Fixture morph (exchange) ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MorphMap {
    pub from: String,
    pub to: String,
    /// `direct` | `emulated` (e.g. rgb→rgbw, rgb→cmy, rgb→colorwheel slot) | `approx`.
    pub kind: String,
    pub note: String,
}

/// Result of `POST /api/morph/preview` `{ids, profile, mode}`.
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MorphPreview {
    /// 0..1 share of the source's used attributes that survive in the target.
    pub coverage: f32,
    pub maps: Vec<MorphMap>,
    /// Source attributes that the target cannot reproduce (values will be ignored).
    pub dropped: Vec<String>,
    /// Target attributes nobody drives (they'll sit at their defaults).
    pub unset: Vec<String>,
    pub warnings: Vec<String>,
    pub footprint: u16,
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
        /// Store palette links (refs) for attributes applied from palettes.
        #[serde(default = "d_true")]
        refs: bool,
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
    // ── scenes ──
    /// Record a scene. `from_output`: snapshot what is currently rendered (cuelists + programmer + scenes);
    /// otherwise record the programmer. `fixtures: None` = every fixture that has values.
    /// `id: Some(n)` updates scene n (`merge`: keep untouched fixtures, otherwise replace).
    SceneRecord { id: Option<u32>, name: String, from_output: bool, fixtures: Option<Vec<u32>>, merge: bool, #[serde(default = "d_true")] refs: bool },
    SceneSave { scene: Scene },
    SceneRemove { id: u32 },
    /// Activate (`on`) with `fade_in` or release with `fade_out`.
    SceneGo { id: u32, on: bool },
    /// Activate this scene and release every other scene.
    SceneSolo { id: u32 },
    /// Manual level 0..1 (no timing); level > 0 activates, 0 releases.
    SceneLevel { id: u32, level: f32 },
    /// Crossfade A→B (`t` 0..1). `t: None` releases the morph.
    SceneMorph { a: u32, b: u32, t: Option<f32> },
    ScenesRelease,
    // ── fixture exchange (morph) ──
    /// Replace the profile/mode of `ids`, keeping fixture ids so cues/scenes/palettes/groups survive.
    /// `repack`: re-address the fixtures consecutively from the first one's address.
    FixtureMorph { ids: Vec<u32>, profile: String, mode: String, repack: bool },
    // ── programming (be-prog) ──
    Undo,
    Redo,
    BlindSet { on: bool },
    /// Load a cue's resulting look into the programmer (to edit it, then `CueUpdate`).
    CueLoad { cuelist: u32, number: f64 },
    /// Write the programmer into an existing cue. `refs`: keep palette links for attributes that came from palettes.
    CueUpdate { cuelist: u32, number: f64, mode: UpdateMode, refs: bool },
    /// Release parked channels (`None` = all). Parking is `DmxSet` with a value.
    Unpark { universe: Option<u16>, channel: Option<u16> },
    // ── playback (be-play) ──
    ChaseSave { chase: Chase },
    ChaseRemove { id: u32 },
    ChaseRun { id: u32, on: bool },
    ChaseBpm { id: u32, bpm: f32 },
    ChaseTap { id: u32 },
    /// Step the running chase manually (±n).
    ChaseNudge { id: u32, delta: i32 },
    SpeedSave { master: SpeedMaster },
    SpeedRemove { id: u32 },
    SpeedSet { id: u32, bpm: f32 },
    SpeedTap { id: u32 },
    /// Manual crossfade of the running fade: `Some(p)` drives progress 0..1, `None` returns to timed.
    CuelistManual { cuelist: u32, progress: Option<f32> },
    // ── automation (be-auto) ──
    /// Command-line statement (see engine cmdline docs). Result lands in `Runtime.cmd_log`.
    CommandLine { text: String },
    MacroSave { r#macro: Macro },
    MacroRemove { id: u32 },
    MacroRun { id: u32 },
    MacroStop { id: Option<u32> },
    TimelineSave { timeline: Timeline },
    TimelineRemove { id: u32 },
    TimelineRun { id: u32, from: Option<f32> },
    TimelineStop { id: u32 },
    TimelineSeek { id: u32, t: f32 },
    // ── data & network (be-data) ──
    BackupCreate { label: String },
    BackupRestore { id: String },
    BackupDelete { id: String },
    /// Re-send the Art-Net node list after an ArtPoll.
    NodesScan,
    OutputTest { mode: TestMode },
    /// Locate a fixture: dimmer/colour to full, pan/tilt home, shutter open.
    Identify { fixture: u32, on: bool },
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
    // ── virtual console ──
    /// Replace all virtual-console pages (editor saves the whole layout).
    VcSave { pages: Vec<VcPage> },
    /// Press/release of a button widget (from UI or touch).
    VcPress { widget: u32, down: bool },
    /// Continuous value 0..1 for slider/knob/dial widgets.
    VcValue { widget: u32, value: f32 },
    /// XY pad position, both 0..1.
    VcXy { widget: u32, x: f32, y: f32 },
    /// Remove an imported/custom profile from the show library.
    LibraryRemove { id: String },
    /// Enable/disable a native (server-side) MIDI port. Machine-level setting, persisted outside the show.
    MidiEnable { output: bool, name: String, enabled: bool },
    // ── streaming ──
    /// Subscribe to binary DMX frames (`None` = all universes, `Some([])` = none).
    SubscribeDmx { universes: Option<Vec<u16>> },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum TestMode {
    Off,
    /// Every channel of `universe` (`None` = all universes) at `level`.
    Level { universe: Option<u16>, level: u8 },
    /// A single channel walking through `universe` (`None` = all).
    Chase { universe: Option<u16> },
    Channel { universe: u16, channel: u16, level: u8 },
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackupInfo {
    pub id: String,
    pub label: String,
    /// Unix seconds.
    pub at: u32,
    /// `auto` | `manual`
    pub kind: String,
    pub size: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArtNode {
    pub ip: String,
    pub name: String,
    pub long_name: String,
    /// Output port-addresses the node reports.
    pub universes: Vec<u16>,
    pub last_seen_ms: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MidiPortInfo {
    pub name: String,
    pub enabled: bool,
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
    /// Show backups (auto + manual). Sent on connect and after changes.
    Backups { items: Vec<BackupInfo> },
    /// Art-Net nodes discovered via ArtPoll.
    Nodes { nodes: Vec<ArtNode> },
    /// Native MIDI availability and port lists (`native=false` → the UI falls back to Web MIDI).
    MidiPorts { native: bool, inputs: Vec<MidiPortInfo>, outputs: Vec<MidiPortInfo> },
    /// Feedback MIDI to relay to a hardware output (the browser forwards it through Web MIDI).
    MidiOut { port: Option<String>, bytes: Vec<u8> },
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
