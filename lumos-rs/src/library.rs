//! Built-in fixture library + Open Fixture Library (OFL) JSON import.

use crate::model::*;
use serde_json::Value;

// ───────────────────────────── builders ─────────────────────────────

fn c(attr: &str, offset: u16) -> ChannelDef {
    ChannelDef {
        attr: attr.into(),
        offset,
        fine: None,
        default: attr_default(attr),
        invert: false,
        capabilities: vec![],
    }
}

fn c16(attr: &str, offset: u16, fine: u16) -> ChannelDef {
    ChannelDef { fine: Some(fine), ..c(attr, offset) }
}

fn cd(mut ch: ChannelDef, default: f32) -> ChannelDef {
    ch.default = default;
    ch
}

fn cc(mut ch: ChannelDef, caps: &[(u8, u8, &str, Option<&str>)]) -> ChannelDef {
    ch.capabilities = caps
        .iter()
        .map(|(min, max, label, color)| Capability {
            min: *min,
            max: *max,
            label: (*label).into(),
            color: color.map(|s| s.to_string()),
        })
        .collect();
    ch
}

fn mode(name: &str, channels: Vec<ChannelDef>) -> Mode {
    Mode { name: name.into(), channels }
}

#[allow(clippy::too_many_arguments)]
fn prof(
    id: &str,
    model: &str,
    category: &str,
    modes: Vec<Mode>,
    beam: [f32; 2],
    pan: f32,
    tilt: f32,
    power: f32,
) -> FixtureProfile {
    FixtureProfile {
        id: id.into(),
        manufacturer: "Generic".into(),
        model: model.into(),
        category: category.into(),
        modes,
        beam_angle: beam,
        pan_range: pan,
        tilt_range: tilt,
        power_w: power,
    }
}

/// Number of DMX channels a mode occupies.
pub fn mode_footprint(m: &Mode) -> u16 {
    m.channels
        .iter()
        .map(|c| c.offset.max(c.fine.unwrap_or(0)) + 1)
        .max()
        .unwrap_or(0)
}

fn shutter_caps() -> ChannelDef {
    cd(
        cc(
            c("shutter", 0),
            &[
                (0, 3, "Closed", None),
                (4, 7, "Open", None),
                (8, 215, "Strobe slow→fast", None),
                (216, 255, "Open", None),
            ],
        ),
        0.025,
    )
}

fn at(mut ch: ChannelDef, offset: u16) -> ChannelDef {
    ch.offset = offset;
    ch
}

fn color_wheel(offset: u16) -> ChannelDef {
    at(
        cc(
            c("colorwheel", 0),
            &[
                (0, 9, "White", Some("#ffffff")),
                (10, 19, "Red", Some("#ff0000")),
                (20, 29, "Orange", Some("#ff8000")),
                (30, 39, "Yellow", Some("#ffe000")),
                (40, 49, "Green", Some("#00c000")),
                (50, 59, "Cyan", Some("#00d0ff")),
                (60, 69, "Blue", Some("#0030ff")),
                (70, 79, "Magenta", Some("#ff00c0")),
                (80, 89, "CTO", Some("#ffb060")),
                (90, 255, "Rotation", None),
            ],
        ),
        offset,
    )
}

fn gobo_wheel(offset: u16) -> ChannelDef {
    at(
        cc(
            c("gobo", 0),
            &[
                (0, 9, "Open", None),
                (10, 19, "Gobo 1", None),
                (20, 29, "Gobo 2", None),
                (30, 39, "Gobo 3", None),
                (40, 49, "Gobo 4", None),
                (50, 59, "Gobo 5", None),
                (60, 69, "Gobo 6", None),
                (70, 79, "Gobo 7", None),
                (80, 255, "Shake / scroll", None),
            ],
        ),
        offset,
    )
}

/// All built-in profiles (≥ 20).
pub fn builtin() -> Vec<FixtureProfile> {
    let mut v = Vec::new();

    v.push(prof(
        "generic/dimmer",
        "Dimmer",
        "dimmer",
        vec![mode("1ch", vec![c("dimmer", 0)])],
        [40.0, 40.0],
        0.0,
        0.0,
        1000.0,
    ));

    v.push(prof(
        "generic/rgb-par",
        "RGB Par",
        "par",
        vec![
            mode("3ch", vec![c("red", 0), c("green", 1), c("blue", 2)]),
            mode("4ch", vec![c("dimmer", 0), c("red", 1), c("green", 2), c("blue", 3)]),
            mode(
                "7ch",
                vec![
                    c("dimmer", 0),
                    c("red", 1),
                    c("green", 2),
                    c("blue", 3),
                    at(shutter_caps(), 4),
                    c("control", 5),
                    c("speed", 6),
                ],
            ),
        ],
        [25.0, 25.0],
        0.0,
        0.0,
        54.0,
    ));

    v.push(prof(
        "generic/rgbw-par",
        "RGBW Par",
        "par",
        vec![
            mode("4ch", vec![c("red", 0), c("green", 1), c("blue", 2), c("white", 3)]),
            mode("5ch", vec![c("dimmer", 0), c("red", 1), c("green", 2), c("blue", 3), c("white", 4)]),
            mode(
                "8ch",
                vec![
                    c("dimmer", 0),
                    c("red", 1),
                    c("green", 2),
                    c("blue", 3),
                    c("white", 4),
                    at(shutter_caps(), 5),
                    c("control", 6),
                    c("speed", 7),
                ],
            ),
        ],
        [25.0, 25.0],
        0.0,
        0.0,
        72.0,
    ));

    v.push(prof(
        "generic/rgba-par",
        "RGBA Par",
        "par",
        vec![
            mode("4ch", vec![c("red", 0), c("green", 1), c("blue", 2), c("amber", 3)]),
            mode("5ch", vec![c("dimmer", 0), c("red", 1), c("green", 2), c("blue", 3), c("amber", 4)]),
        ],
        [25.0, 25.0],
        0.0,
        0.0,
        72.0,
    ));

    v.push(prof(
        "generic/rgbawuv-par",
        "RGBAW+UV Par",
        "par",
        vec![
            mode(
                "6ch",
                vec![c("red", 0), c("green", 1), c("blue", 2), c("amber", 3), c("white", 4), c("uv", 5)],
            ),
            mode(
                "7ch",
                vec![
                    c("dimmer", 0),
                    c("red", 1),
                    c("green", 2),
                    c("blue", 3),
                    c("amber", 4),
                    c("white", 5),
                    c("uv", 6),
                ],
            ),
            mode(
                "10ch",
                vec![
                    c("dimmer", 0),
                    c("red", 1),
                    c("green", 2),
                    c("blue", 3),
                    c("amber", 4),
                    c("white", 5),
                    c("uv", 6),
                    at(shutter_caps(), 7),
                    c("control", 8),
                    c("speed", 9),
                ],
            ),
        ],
        [25.0, 40.0],
        0.0,
        0.0,
        108.0,
    ));

    v.push(prof(
        "generic/rgb-par-dimstrobe",
        "RGB Par (Dimmer + Strobe)",
        "par",
        vec![mode(
            "6ch",
            vec![c("dimmer", 0), c("red", 1), c("green", 2), c("blue", 3), at(shutter_caps(), 4), c("control", 5)],
        )],
        [30.0, 30.0],
        0.0,
        0.0,
        60.0,
    ));

    v.push(prof(
        "generic/led-bar",
        "LED Bar RGB",
        "bar",
        vec![
            mode("RGB 3ch", vec![c("red", 0), c("green", 1), c("blue", 2)]),
            mode("Dimmer+RGB 4ch", vec![c("dimmer", 0), c("red", 1), c("green", 2), c("blue", 3)]),
            mode(
                "Dimmer+RGBW+Strobe 7ch",
                vec![
                    c("dimmer", 0),
                    c("red", 1),
                    c("green", 2),
                    c("blue", 3),
                    c("white", 4),
                    at(shutter_caps(), 5),
                    c("control", 6),
                ],
            ),
        ],
        [40.0, 60.0],
        0.0,
        0.0,
        80.0,
    ));

    v.push(prof(
        "generic/strobe",
        "Strobe",
        "strobe",
        vec![
            mode("1ch", vec![cd(c("shutter", 0), 0.0)]),
            mode("2ch", vec![c("dimmer", 0), cd(c("shutter", 1), 0.0)]),
        ],
        [90.0, 90.0],
        0.0,
        0.0,
        1500.0,
    ));

    v.push(prof(
        "generic/fog",
        "Fog Machine",
        "effect",
        vec![
            mode(
                "1ch",
                vec![cc(c("control", 0), &[(0, 9, "Off", None), (10, 255, "Fog output", None)])],
            ),
            mode(
                "2ch",
                vec![
                    cc(c("control", 0), &[(0, 9, "Off", None), (10, 255, "Fog output", None)]),
                    c("speed", 1),
                ],
            ),
        ],
        [0.0, 0.0],
        0.0,
        0.0,
        1200.0,
    ));

    v.push(prof(
        "generic/hazer",
        "Hazer",
        "effect",
        vec![mode(
            "2ch",
            vec![cc(c("control", 0), &[(0, 9, "Off", None), (10, 255, "Haze output", None)]), c("speed", 1)],
        )],
        [0.0, 0.0],
        0.0,
        0.0,
        800.0,
    ));

    v.push(prof(
        "generic/scanner",
        "Scanner",
        "spot",
        vec![mode(
            "8ch",
            vec![
                c("pan", 0),
                c("tilt", 1),
                color_wheel(2),
                gobo_wheel(3),
                at(shutter_caps(), 4),
                c("dimmer", 5),
                c("gobo_rot", 6),
                c("speed", 7),
            ],
        )],
        [12.0, 12.0],
        180.0,
        90.0,
        150.0,
    ));

    v.push(prof(
        "generic/mh-spot",
        "Moving Head Spot",
        "spot",
        vec![
            mode(
                "16ch",
                vec![
                    c16("pan", 0, 1),
                    c16("tilt", 2, 3),
                    c("speed", 4),
                    c("dimmer", 5),
                    at(shutter_caps(), 6),
                    color_wheel(7),
                    gobo_wheel(8),
                    c("gobo_rot", 9),
                    cc(c("prism", 10), &[(0, 9, "Open", None), (10, 255, "Prism", None)]),
                    c("prism_rot", 11),
                    c("focus", 12),
                    c("zoom", 13),
                    c("frost", 14),
                    c("control", 15),
                ],
            ),
            mode(
                "12ch",
                vec![
                    c("pan", 0),
                    c("tilt", 1),
                    c("speed", 2),
                    c("dimmer", 3),
                    at(shutter_caps(), 4),
                    color_wheel(5),
                    gobo_wheel(6),
                    c("gobo_rot", 7),
                    c("prism", 8),
                    c("focus", 9),
                    c("zoom", 10),
                    c("control", 11),
                ],
            ),
        ],
        [10.0, 30.0],
        540.0,
        270.0,
        250.0,
    ));

    v.push(prof(
        "generic/mh-wash",
        "Moving Head Wash RGBW",
        "wash",
        vec![
            mode(
                "14ch",
                vec![
                    c16("pan", 0, 1),
                    c16("tilt", 2, 3),
                    c("speed", 4),
                    c("dimmer", 5),
                    at(shutter_caps(), 6),
                    c("red", 7),
                    c("green", 8),
                    c("blue", 9),
                    c("white", 10),
                    c("zoom", 11),
                    c("effect", 12),
                    c("control", 13),
                ],
            ),
            mode(
                "10ch",
                vec![
                    c("pan", 0),
                    c("tilt", 1),
                    c("dimmer", 2),
                    at(shutter_caps(), 3),
                    c("red", 4),
                    c("green", 5),
                    c("blue", 6),
                    c("white", 7),
                    c("zoom", 8),
                    c("speed", 9),
                ],
            ),
        ],
        [12.0, 50.0],
        540.0,
        270.0,
        300.0,
    ));

    v.push(prof(
        "generic/mh-beam",
        "Moving Head Beam",
        "beam",
        vec![mode(
            "16ch",
            vec![
                c16("pan", 0, 1),
                c16("tilt", 2, 3),
                c("speed", 4),
                c("dimmer", 5),
                at(shutter_caps(), 6),
                color_wheel(7),
                gobo_wheel(8),
                c("gobo_rot", 9),
                cc(c("prism", 10), &[(0, 9, "Open", None), (10, 255, "Prism", None)]),
                c("prism_rot", 11),
                c("focus", 12),
                c("frost", 13),
                c("effect", 14),
                c("control", 15),
            ],
        )],
        [2.0, 2.0],
        540.0,
        270.0,
        230.0,
    ));

    v.push(prof(
        "generic/fresnel-cto",
        "Tungsten Fresnel (Dimmer + CTO)",
        "wash",
        vec![
            mode("1ch", vec![c("dimmer", 0)]),
            mode("2ch", vec![c("dimmer", 0), c("cto", 1)]),
        ],
        [10.0, 60.0],
        0.0,
        0.0,
        1000.0,
    ));

    v.push(prof(
        "generic/led-profile",
        "LED Profile Spot",
        "spot",
        vec![mode(
            "6ch",
            vec![c("dimmer", 0), at(shutter_caps(), 1), c("zoom", 2), c("focus", 3), c("iris", 4), c("cto", 5)],
        )],
        [15.0, 50.0],
        0.0,
        0.0,
        300.0,
    ));

    v.push(prof(
        "generic/uv-bar",
        "UV Bar",
        "bar",
        vec![mode("1ch", vec![c("uv", 0)]), mode("2ch", vec![c("dimmer", 0), c("uv", 1)])],
        [40.0, 40.0],
        0.0,
        0.0,
        60.0,
    ));

    v.push(prof(
        "generic/rgb-wash-dim16",
        "RGB Wash (16-bit dimmer)",
        "wash",
        vec![mode("5ch", vec![c16("dimmer", 0, 1), c("red", 2), c("green", 3), c("blue", 4)])],
        [30.0, 50.0],
        0.0,
        0.0,
        150.0,
    ));

    v.push(prof(
        "generic/tunable-white",
        "Tunable White Panel",
        "wash",
        vec![mode("2ch", vec![c("dimmer", 0), c("cto", 1)])],
        [60.0, 60.0],
        0.0,
        0.0,
        100.0,
    ));

    v.push(prof(
        "generic/led-strip-rgbw",
        "LED Strip RGBW",
        "bar",
        vec![mode("4ch", vec![c("red", 0), c("green", 1), c("blue", 2), c("white", 3)])],
        [120.0, 120.0],
        0.0,
        0.0,
        40.0,
    ));

    v.push(prof(
        "generic/blinder",
        "Blinder",
        "wash",
        vec![mode("1ch", vec![c("dimmer", 0)])],
        [60.0, 60.0],
        0.0,
        0.0,
        650.0,
    ));

    v
}

// ───────────────────────────── OFL import ─────────────────────────────

fn slug(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn parse_deg(v: &Value) -> Option<f32> {
    match v {
        Value::Number(n) => n.as_f64().map(|x| x as f32),
        Value::String(s) => s.trim_end_matches("deg").trim().parse::<f32>().ok(),
        _ => None,
    }
}

fn cap_list(ch: &Value) -> Vec<&Value> {
    if let Some(c) = ch.get("capability") {
        vec![c]
    } else if let Some(arr) = ch.get("capabilities").and_then(|c| c.as_array()) {
        arr.iter().collect()
    } else {
        vec![]
    }
}

fn attr_from_capability(cap: &Value, channel_name: &str) -> Option<&'static str> {
    let ty = cap.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let color = cap.get("color").and_then(|t| t.as_str()).unwrap_or("");
    let wheel = cap
        .get("wheel")
        .and_then(|t| t.as_str())
        .unwrap_or(channel_name)
        .to_lowercase();
    Some(match ty {
        "Intensity" => "dimmer",
        "ColorIntensity" => match color {
            "Red" => "red",
            "Green" => "green",
            "Blue" => "blue",
            "White" | "Warm White" | "Cold White" => "white",
            "Amber" => "amber",
            "UV" => "uv",
            "Lime" => "lime",
            _ => "control",
        },
        "Pan" => "pan",
        "Tilt" => "tilt",
        "ColorPreset" | "ColorWheelIndex" => "colorwheel",
        "WheelSlot" | "WheelShake" | "WheelSlotRotation" if wheel.contains("color") => "colorwheel",
        "WheelSlot" | "WheelShake" => "gobo",
        "WheelRotation" => {
            if wheel.contains("color") {
                "colorwheel"
            } else {
                "gobo_rot"
            }
        }
        "ShutterStrobe" | "StrobeSpeed" | "StrobeDuration" => "shutter",
        "Zoom" => "zoom",
        "Focus" => "focus",
        "Iris" | "IrisEffect" => "iris",
        "Frost" | "FrostEffect" => "frost",
        "Prism" => "prism",
        "PrismRotation" => "prism_rot",
        "ColorTemperature" => "cto",
        "Speed" | "PanTiltSpeed" | "PanContinuous" | "TiltContinuous" => "speed",
        "Effect" | "EffectSpeed" | "EffectDuration" | "EffectParameter" => "effect",
        "NoFunction" => return None,
        _ => "control",
    })
}

fn attr_from_channel(ch: &Value, name: &str) -> &'static str {
    for cap in cap_list(ch) {
        if let Some(a) = attr_from_capability(cap, name) {
            return a;
        }
    }
    "control"
}

fn capabilities_of(ch: &Value) -> Vec<Capability> {
    let Some(arr) = ch.get("capabilities").and_then(|c| c.as_array()) else {
        return vec![];
    };
    arr.iter()
        .filter_map(|cap| {
            let r = cap.get("dmxRange")?.as_array()?;
            let min = r.first()?.as_u64()?.min(255) as u8;
            let max = r.get(1)?.as_u64()?.min(255) as u8;
            let label = cap
                .get("comment")
                .and_then(|c| c.as_str())
                .or_else(|| cap.get("type").and_then(|c| c.as_str()))
                .unwrap_or("")
                .to_string();
            let color = cap
                .get("colors")
                .and_then(|c| c.get("hexColors"))
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            Some(Capability { min, max, label, color })
        })
        .collect()
}

fn default_value(ch: &Value) -> f32 {
    match ch.get("defaultValue") {
        Some(Value::Number(n)) => (n.as_f64().unwrap_or(0.0) as f32 / 255.0).clamp(0.0, 1.0),
        Some(Value::String(s)) => {
            if let Some(p) = s.strip_suffix('%') {
                p.trim().parse::<f32>().map(|x| x / 100.0).unwrap_or(0.0)
            } else {
                s.parse::<f32>().map(|x| x / 255.0).unwrap_or(0.0)
            }
        }
        _ => 0.0,
    }
}

/// Parse one OFL fixture JSON document into a profile.
/// Accepts an optional top-level `"manufacturer"` string (OFL files do not carry it).
pub fn import_ofl(json: &str) -> Result<FixtureProfile, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("Invalid JSON: {e}"))?;
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or("OFL: missing \"name\"")?
        .to_string();
    let manufacturer = v
        .get("manufacturer")
        .and_then(|m| m.as_str().map(|s| s.to_string()).or_else(|| m.get("name").and_then(|n| n.as_str()).map(|s| s.to_string())))
        .unwrap_or_else(|| "Imported".into());
    let avail = v
        .get("availableChannels")
        .and_then(|a| a.as_object())
        .ok_or("OFL: missing \"availableChannels\"")?;
    let modes_json = v
        .get("modes")
        .and_then(|m| m.as_array())
        .ok_or("OFL: missing \"modes\"")?;

    // fine-channel aliases: alias name → owner channel name
    let mut fine_of: std::collections::HashMap<String, String> = Default::default();
    for (cname, cv) in avail {
        if let Some(al) = cv.get("fineChannelAliases").and_then(|a| a.as_array()) {
            for a in al.iter().filter_map(|a| a.as_str()) {
                fine_of.insert(a.to_string(), cname.clone());
            }
        }
    }

    let mut modes = Vec::new();
    let mut has_pan = false;
    let mut has_rgb = false;
    let mut pan_range = 0.0f32;
    let mut tilt_range = 0.0f32;
    for m in modes_json {
        let mname = m.get("name").and_then(|n| n.as_str()).unwrap_or("Mode").to_string();
        let Some(chs) = m.get("channels").and_then(|c| c.as_array()) else { continue };
        let mut defs: Vec<ChannelDef> = Vec::new();
        let mut src_names: Vec<String> = Vec::new();
        let mut ok = true;
        for (off, entry) in chs.iter().enumerate() {
            let off = off as u16;
            match entry {
                Value::Null => {}
                Value::String(cn) => {
                    if let Some(owner) = fine_of.get(cn) {
                        if let Some(i) = src_names.iter().position(|n| n == owner) {
                            defs[i].fine = Some(off);
                        }
                        continue;
                    }
                    let Some(cv) = avail.get(cn) else {
                        ok = false;
                        break;
                    };
                    let attr = attr_from_channel(cv, cn);
                    let mut def = c(attr, off);
                    def.default = default_value(cv);
                    if attr == "pan" || attr == "tilt" {
                        if cv.get("defaultValue").is_none() {
                            def.default = 0.5;
                        }
                        for cap in cap_list(cv) {
                            if let (Some(a), Some(b)) = (
                                cap.get("angleStart").and_then(parse_deg),
                                cap.get("angleEnd").and_then(parse_deg),
                            ) {
                                let span = (b - a).abs();
                                if attr == "pan" {
                                    pan_range = pan_range.max(span);
                                } else {
                                    tilt_range = tilt_range.max(span);
                                }
                            }
                        }
                    }
                    if cv.get("capabilities").is_some() && matches!(attr, "colorwheel" | "gobo" | "shutter" | "prism" | "control") {
                        def.capabilities = capabilities_of(cv);
                    }
                    src_names.push(cn.clone());
                    defs.push(def);
                }
                _ => {
                    ok = false; // matrix / insert entries are not supported
                    break;
                }
            }
        }
        if ok && !defs.is_empty() {
            if defs.iter().any(|d| d.attr == "pan") {
                has_pan = true;
            }
            if defs.iter().any(|d| d.attr == "red") && defs.iter().any(|d| d.attr == "blue") {
                has_rgb = true;
            }
            modes.push(Mode { name: mname, channels: defs });
        }
    }
    if modes.is_empty() {
        return Err("OFL: no importable modes (matrix modes are not supported)".into());
    }

    let cat0 = v
        .get("categories")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let category = match cat0 {
        "Dimmer" => "dimmer",
        "Strobe" => "strobe",
        "Smoke" | "Hazer" | "Effect" | "Laser" => "effect",
        "Pixel Bar" | "Bar" | "Stand" => "bar",
        "Blinder" => "wash",
        "Moving Head" | "Scanner" => {
            if has_pan {
                "spot"
            } else {
                "wash"
            }
        }
        "Color Changer" => {
            if has_rgb {
                "par"
            } else {
                "wash"
            }
        }
        _ => {
            if has_pan {
                "spot"
            } else if has_rgb {
                "par"
            } else {
                "other"
            }
        }
    };
    let phys = v.get("physical");
    let beam = phys
        .and_then(|p| p.get("lens"))
        .and_then(|l| l.get("degreesMinMax"))
        .and_then(|d| d.as_array())
        .and_then(|d| Some([d.first()?.as_f64()? as f32, d.get(1)?.as_f64()? as f32]))
        .unwrap_or([25.0, 25.0]);
    let power = phys
        .and_then(|p| p.get("power"))
        .and_then(|p| p.as_f64())
        .unwrap_or(0.0) as f32;
    if has_pan && pan_range == 0.0 {
        pan_range = 540.0;
    }
    if has_pan && tilt_range == 0.0 {
        tilt_range = 270.0;
    }

    Ok(FixtureProfile {
        id: format!("import/{}-{}", slug(&manufacturer), slug(&name)),
        manufacturer,
        model: name,
        category: category.into(),
        modes,
        beam_angle: beam,
        pan_range,
        tilt_range,
        power_w: power,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_count_and_ids_unique() {
        let b = builtin();
        assert!(b.len() >= 20, "{}", b.len());
        let mut ids: Vec<_> = b.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), b.len());
        for p in &b {
            for m in &p.modes {
                // offsets must be unique
                let mut used = std::collections::HashSet::new();
                for ch in &m.channels {
                    assert!(used.insert(ch.offset), "{} {} dup offset {}", p.id, m.name, ch.offset);
                    if let Some(f) = ch.fine {
                        assert!(used.insert(f), "{} {} dup fine {}", p.id, m.name, f);
                    }
                }
            }
        }
    }

    #[test]
    fn mover_footprint() {
        let b = builtin();
        let p = b.iter().find(|p| p.id == "generic/mh-spot").unwrap();
        assert_eq!(mode_footprint(&p.modes[0]), 16);
    }

    #[test]
    fn ofl_import_basic() {
        let j = r#"{
          "name":"Test Wash","manufacturer":"Acme","categories":["Moving Head"],
          "physical":{"power":200,"lens":{"degreesMinMax":[10,40]}},
          "availableChannels":{
            "Pan":{"fineChannelAliases":["Pan fine"],"capability":{"type":"Pan","angleStart":"0deg","angleEnd":"540deg"}},
            "Dimmer":{"capability":{"type":"Intensity"}},
            "Red":{"capability":{"type":"ColorIntensity","color":"Red"}},
            "Strobe":{"capabilities":[{"dmxRange":[0,9],"type":"ShutterStrobe","shutterEffect":"Open"},{"dmxRange":[10,255],"type":"ShutterStrobe","shutterEffect":"Strobe"}]}
          },
          "modes":[{"name":"5ch","channels":["Pan","Pan fine","Dimmer","Red","Strobe"]}]}"#;
        let p = import_ofl(j).unwrap();
        assert_eq!(p.model, "Test Wash");
        assert_eq!(p.manufacturer, "Acme");
        let m = &p.modes[0];
        assert_eq!(m.channels.len(), 4);
        assert_eq!(m.channels[0].attr, "pan");
        assert_eq!(m.channels[0].fine, Some(1));
        assert_eq!(m.channels[1].attr, "dimmer");
        assert_eq!(m.channels[1].offset, 2);
        assert_eq!(m.channels[3].attr, "shutter");
        assert_eq!(m.channels[3].capabilities.len(), 2);
        assert_eq!(p.pan_range, 540.0);
        assert_eq!(p.category, "spot");
    }

    #[test]
    fn ofl_rejects_garbage() {
        assert!(import_ofl("nope").is_err());
        assert!(import_ofl("{}").is_err());
    }
}
