"""Built-in fixture library — 50+ generic and brand fixtures."""
from __future__ import annotations
from .model import FixtureDef, FixtureMode, ChannelDef, ChannelType, Capability, Physical


# ── Shared capability sets ────────────────────────────────────────────────────

def _shutter_caps():
    return [
        Capability(0,   31,  "Closed"),
        Capability(32,  63,  "Open"),
        Capability(64,  95,  "Strobe 1–25 Hz"),
        Capability(96,  127, "Strobe 26–50 Hz (fast)"),
        Capability(128, 159, "Pulse open slow–fast"),
        Capability(160, 191, "Pulse close slow–fast"),
        Capability(192, 223, "Random strobe"),
        Capability(224, 255, "Open"),
    ]

def _color_wheel_caps():
    return [
        Capability(0,   7,   "White",      color="#ffffff"),
        Capability(8,   15,  "Red",        color="#ff2020"),
        Capability(16,  23,  "Yellow",     color="#ffee00"),
        Capability(24,  31,  "Green",      color="#20ff20"),
        Capability(32,  39,  "Blue",       color="#2040ff"),
        Capability(40,  47,  "Magenta",    color="#ff20ff"),
        Capability(48,  55,  "Cyan",       color="#20ffff"),
        Capability(56,  63,  "Orange",     color="#ff8000"),
        Capability(64,  71,  "UV",         color="#9000ff"),
        Capability(72,  79,  "Light Blue", color="#80c0ff"),
        Capability(80,  167, "Rotation CCW fast–slow"),
        Capability(168, 175, "Stop"),
        Capability(176, 239, "Rotation CW slow–fast"),
        Capability(240, 255, "Random"),
    ]

def _gobo_wheel_caps():
    return [
        Capability(0,   7,   "Open",      gobo_index=0),
        Capability(8,   15,  "Gobo 1",    gobo_index=1),
        Capability(16,  23,  "Gobo 2",    gobo_index=2),
        Capability(24,  31,  "Gobo 3",    gobo_index=3),
        Capability(32,  39,  "Gobo 4",    gobo_index=4),
        Capability(40,  47,  "Gobo 5",    gobo_index=5),
        Capability(48,  55,  "Gobo 6",    gobo_index=6),
        Capability(56,  63,  "Gobo 7",    gobo_index=7),
        Capability(64,  127, "Gobo shake"),
        Capability(128, 191, "Rotation CW slow–fast"),
        Capability(192, 255, "Rotation CCW fast–slow"),
    ]

def _speed_caps():
    return [
        Capability(0,   9,   "Max speed"),
        Capability(10,  245, "Fast–slow"),
        Capability(246, 255, "Blackout on move"),
    ]

def _reset_caps():
    return [
        Capability(0,   79,  "Normal"),
        Capability(80,  84,  "Reset all"),
        Capability(85,  87,  "Reset Pan/Tilt"),
        Capability(88,  90,  "Reset Colors"),
        Capability(200, 209, "Reset (hold 3s)"),
    ]


# ── Channel factories ─────────────────────────────────────────────────────────

def _ch(name: str, ch_type: ChannelType, caps=None, default=0, highlight=255,
        is_fine=False, coarse="", snap=False) -> ChannelDef:
    return ChannelDef(
        name=name, type=ch_type,
        capabilities=caps or [],
        default_value=default,
        highlight_value=highlight,
        is_fine=is_fine,
        coarse_name=coarse,
        snap=snap,
    )

def _pan():   return _ch("Pan",  ChannelType.PAN,  [Capability(0,255,"Pan 0–540°")])
def _panf():  return _ch("Pan Fine",  ChannelType.PAN,  is_fine=True, coarse="Pan")
def _tilt():  return _ch("Tilt", ChannelType.TILT, [Capability(0,255,"Tilt 0–270°")])
def _tiltf(): return _ch("Tilt Fine", ChannelType.TILT, is_fine=True, coarse="Tilt")
def _dim():   return _ch("Dimmer", ChannelType.INTENSITY, [Capability(0,255,"Intensity 0–100%")])
def _dimf():  return _ch("Dimmer Fine", ChannelType.INTENSITY, is_fine=True, coarse="Dimmer")
def _red():   return _ch("Red",   ChannelType.RED,   [Capability(0,255,"Red 0–100%")])
def _redf():  return _ch("Red Fine", ChannelType.RED, is_fine=True, coarse="Red")
def _grn():   return _ch("Green", ChannelType.GREEN, [Capability(0,255,"Green 0–100%")])
def _grnf():  return _ch("Green Fine", ChannelType.GREEN, is_fine=True, coarse="Green")
def _blu():   return _ch("Blue",  ChannelType.BLUE,  [Capability(0,255,"Blue 0–100%")])
def _bluf():  return _ch("Blue Fine", ChannelType.BLUE, is_fine=True, coarse="Blue")
def _wht():   return _ch("White", ChannelType.WHITE, [Capability(0,255,"White 0–100%")])
def _amb():   return _ch("Amber", ChannelType.AMBER, [Capability(0,255,"Amber 0–100%")])
def _uv():    return _ch("UV",    ChannelType.UV,    [Capability(0,255,"UV 0–100%")])
def _lime():  return _ch("Lime",  ChannelType.LIME,  [Capability(0,255,"Lime 0–100%")])
def _zoom():  return _ch("Zoom",  ChannelType.ZOOM,  [Capability(0,127,"Narrow–Wide"),Capability(128,255,"Wide–Narrow")])
def _focus(): return _ch("Focus", ChannelType.FOCUS, [Capability(0,255,"Near–Far")])
def _iris():  return _ch("Iris",  ChannelType.IRIS,  [Capability(0,127,"Open–Closed"),Capability(128,191,"Pulse open"),Capability(192,255,"Pulse close")])
def _prism(): return _ch("Prism", ChannelType.PRISM, [Capability(0,0,"No prism"),Capability(1,127,"Prism CW"),Capability(128,255,"Prism CCW")], snap=True)
def _frost(): return _ch("Frost", ChannelType.FROST, [Capability(0,255,"No frost–Full frost")])
def _speed(): return _ch("Speed", ChannelType.SPEED, _speed_caps())
def _reset(): return _ch("Reset", ChannelType.RESET, _reset_caps(), snap=True)
def _shutter():     return _ch("Shutter", ChannelType.SHUTTER, _shutter_caps(), default=32, snap=True)
def _color_wheel(): return _ch("Color Wheel", ChannelType.COLOR_WHEEL, _color_wheel_caps(), snap=True)
def _gobo_wheel():  return _ch("Gobo Wheel",  ChannelType.GOBO_WHEEL,  _gobo_wheel_caps(), snap=True)
def _gobo_rot():    return _ch("Gobo Rotation", ChannelType.GOBO_ROT, [
    Capability(0,0,"Stop"), Capability(1,127,"CW fast–slow"),
    Capability(128,128,"Stop"), Capability(129,255,"CCW slow–fast"),
])
def _macro():       return _ch("Macro", ChannelType.MACRO, [Capability(0,255,"Macros")])
def _cto():         return _ch("CTO", ChannelType.CTO, [Capability(0,255,"No correction–Full CTO")])
def _ctb():         return _ch("CTB", ChannelType.CTB, [Capability(0,255,"No correction–Full CTB")])
def _strobe_rate(): return _ch("Strobe Rate", ChannelType.STROBE, [
    Capability(0,9,"No strobe"), Capability(10,245,"Slow–Fast 1–25 Hz"), Capability(246,255,"Fast (max)"),
])
def _effects():     return _ch("Effects", ChannelType.EFFECTS, [Capability(0,255,"Effect programs")])
def _cw():  return _ch("Cool White", ChannelType.CTB, [Capability(0,255,"Cool White 0–100%")])
def _ww():  return _ch("Warm White", ChannelType.CTO, [Capability(0,255,"Warm White 0–100%")])
def _fan(): return _ch("Fan Speed",  ChannelType.SPEED, [Capability(0,9,"Off"),Capability(10,255,"Slow–Fast")])
def _pump(): return _ch("Output",   ChannelType.INTENSITY, [Capability(0,9,"Off"),Capability(10,255,"Low–High output")])


# ── Physical presets ──────────────────────────────────────────────────────────

_PHYS_MH_SMALL  = Physical(weight_kg=6.5,  width_mm=250, height_mm=370, depth_mm=210, power_w=85,  lumens=8000,  pan_max=540, tilt_max=270, lens_min=12, lens_max=12)
_PHYS_MH_MED    = Physical(weight_kg=9.0,  width_mm=300, height_mm=470, depth_mm=260, power_w=150, lumens=14000, pan_max=540, tilt_max=270, lens_min=11, lens_max=11)
_PHYS_MH_LARGE  = Physical(weight_kg=14.5, width_mm=350, height_mm=550, depth_mm=310, power_w=280, lumens=22000, pan_max=540, tilt_max=270, lens_min=7,  lens_max=7)
_PHYS_PAR_SMALL = Physical(weight_kg=1.2,  width_mm=130, height_mm=200, depth_mm=130, power_w=25)
_PHYS_PAR_MED   = Physical(weight_kg=2.5,  width_mm=180, height_mm=260, depth_mm=180, power_w=60)
_PHYS_STROBE    = Physical(weight_kg=3.5,  width_mm=280, height_mm=160, depth_mm=120, power_w=130)
_PHYS_BEAM      = Physical(weight_kg=11.0, width_mm=290, height_mm=430, depth_mm=270, power_w=200, lumens=20000, pan_max=540, tilt_max=270, lens_min=2, lens_max=2)


# ── Fixture definitions ───────────────────────────────────────────────────────

def _build_fixtures() -> list[FixtureDef]:
    fxs = []

    # ── Generic / Dimmers ─────────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Dimmer 1ch", fixture_type="Dimmer",
        categories=["Dimmer"],
        channels={"Dimmer": _dim()},
        modes=[FixtureMode("1ch", ["Dimmer"])],
        physical=Physical(power_w=1000),
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Dimmer 2ch", fixture_type="Dimmer",
        categories=["Dimmer"],
        channels={"Dimmer": _dim(), "Strobe Rate": _strobe_rate()},
        modes=[FixtureMode("2ch", ["Dimmer", "Strobe Rate"])],
        physical=Physical(power_w=1000),
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Dimmer 4ch", fixture_type="Dimmer",
        categories=["Dimmer"],
        channels={
            "Dimmer 1": _ch("Dimmer 1", ChannelType.INTENSITY),
            "Dimmer 2": _ch("Dimmer 2", ChannelType.INTENSITY),
            "Dimmer 3": _ch("Dimmer 3", ChannelType.INTENSITY),
            "Dimmer 4": _ch("Dimmer 4", ChannelType.INTENSITY),
        },
        modes=[FixtureMode("4ch", ["Dimmer 1","Dimmer 2","Dimmer 3","Dimmer 4"])],
    ))

    # ── Generic / LED PARs ────────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="RGB PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={"Red": _red(), "Green": _grn(), "Blue": _blu()},
        modes=[FixtureMode("3ch", ["Red","Green","Blue"])],
        physical=_PHYS_PAR_SMALL,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="RGBA PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={"Red": _red(), "Green": _grn(), "Blue": _blu(), "Amber": _amb()},
        modes=[FixtureMode("4ch", ["Red","Green","Blue","Amber"])],
        physical=_PHYS_PAR_SMALL,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="RGBW PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={"Red": _red(), "Green": _grn(), "Blue": _blu(), "White": _wht()},
        modes=[FixtureMode("4ch", ["Red","Green","Blue","White"])],
        physical=_PHYS_PAR_SMALL,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="RGBWA PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={"Red": _red(), "Green": _grn(), "Blue": _blu(), "White": _wht(), "Amber": _amb()},
        modes=[
            FixtureMode("5ch", ["Red","Green","Blue","White","Amber"]),
            FixtureMode("6ch", ["Dimmer","Red","Green","Blue","White","Amber"], ),
        ],
        physical=_PHYS_PAR_MED,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="RGBWAU PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={
            "Red": _red(), "Green": _grn(), "Blue": _blu(),
            "White": _wht(), "Amber": _amb(), "UV": _uv(),
        },
        modes=[
            FixtureMode("6ch", ["Red","Green","Blue","White","Amber","UV"]),
            FixtureMode("7ch", ["Dimmer","Red","Green","Blue","White","Amber","UV"]),
        ],
        physical=_PHYS_PAR_MED,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="CW+WW PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={"Cool White": _cw(), "Warm White": _ww()},
        modes=[FixtureMode("2ch", ["Cool White","Warm White"])],
        physical=_PHYS_PAR_SMALL,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="CW+WW+A PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={"Cool White": _cw(), "Warm White": _ww(), "Amber": _amb()},
        modes=[FixtureMode("3ch", ["Cool White","Warm White","Amber"])],
        physical=_PHYS_PAR_SMALL,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="RGBWAU Master PAR", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={
            "Dimmer": _dim(),
            "Red": _red(), "Green": _grn(), "Blue": _blu(),
            "White": _wht(), "Amber": _amb(), "UV": _uv(),
            "Strobe Rate": _strobe_rate(),
        },
        modes=[
            FixtureMode("7ch",  ["Dimmer","Red","Green","Blue","White","Amber","UV"]),
            FixtureMode("8ch",  ["Dimmer","Red","Green","Blue","White","Amber","UV","Strobe Rate"]),
        ],
        physical=_PHYS_PAR_MED,
    ))

    # ── Generic / Moving Head Spot ────────────────────────────────────────────
    _spot_chs_base = {
        "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
        "Color Wheel": _color_wheel(), "Gobo Wheel": _gobo_wheel(),
        "Shutter": _shutter(), "Dimmer": _dim(),
    }
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Spot 8ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels=_spot_chs_base,
        modes=[FixtureMode("8ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer"])],
        physical=_PHYS_MH_SMALL,
    ))

    _spot_chs_12 = dict(_spot_chs_base)
    _spot_chs_12.update({"Speed": _speed(), "Focus": _focus(), "Iris": _iris(), "Reset": _reset()})
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Spot 12ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels=_spot_chs_12,
        modes=[
            FixtureMode("8ch",  ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer"]),
            FixtureMode("12ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer","Speed","Focus","Iris","Reset"]),
        ],
        physical=_PHYS_MH_MED,
    ))

    _spot_chs_16 = dict(_spot_chs_12)
    _spot_chs_16.update({
        "Prism": _prism(), "Frost": _frost(),
        "Color Fine": _ch("Color Fine", ChannelType.COLOR_WHEEL, is_fine=True, coarse="Color Wheel"),
        "Gobo Fine":  _ch("Gobo Fine",  ChannelType.GOBO_WHEEL,  is_fine=True, coarse="Gobo Wheel"),
    })
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Spot 16ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels=_spot_chs_16,
        modes=[
            FixtureMode("8ch",  ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer"]),
            FixtureMode("12ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer","Speed","Focus","Iris","Reset"]),
            FixtureMode("16ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer","Speed","Focus","Iris","Prism","Frost","Reset","Color Fine","Gobo Fine"]),
        ],
        physical=_PHYS_MH_LARGE,
    ))

    # ── Generic / Moving Head Wash ────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Wash 7ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Tilt": _tilt(),
            "Red": _red(), "Green": _grn(), "Blue": _blu(),
            "Dimmer": _dim(), "Speed": _speed(),
        },
        modes=[FixtureMode("7ch", ["Pan","Tilt","Red","Green","Blue","Dimmer","Speed"])],
        physical=_PHYS_MH_SMALL,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Wash 11ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
            "Red": _red(), "Green": _grn(), "Blue": _blu(), "White": _wht(),
            "Dimmer": _dim(), "Zoom": _zoom(), "Speed": _speed(),
        },
        modes=[
            FixtureMode("7ch",  ["Pan","Tilt","Red","Green","Blue","Dimmer","Speed"]),
            FixtureMode("11ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Red","Green","Blue","White","Dimmer","Zoom","Speed"]),
        ],
        physical=_PHYS_MH_MED,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Wash 14ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
            "Red": _red(), "Red Fine": _redf(), "Green": _grn(), "Green Fine": _grnf(),
            "Blue": _blu(), "Blue Fine": _bluf(), "White": _wht(),
            "Dimmer": _dim(), "Zoom": _zoom(), "Speed": _speed(),
        },
        modes=[
            FixtureMode("11ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Red","Green","Blue","White","Dimmer","Zoom","Speed"]),
            FixtureMode("14ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Red","Red Fine","Green","Green Fine","Blue","Blue Fine","White","Dimmer","Zoom","Speed"]),
        ],
        physical=_PHYS_MH_LARGE,
    ))

    # ── Generic / Moving Head Beam ────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Moving Head Beam 14ch", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
            "Color Wheel": _color_wheel(), "Gobo Wheel": _gobo_wheel(), "Gobo Rotation": _gobo_rot(),
            "Prism": _prism(), "Frost": _frost(),
            "Shutter": _shutter(), "Dimmer": _dim(), "Focus": _focus(),
            "Speed": _speed(), "Reset": _reset(),
        },
        modes=[FixtureMode("14ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Gobo Rotation","Prism","Frost","Shutter","Dimmer","Focus","Speed","Reset"])],
        physical=_PHYS_BEAM,
    ))

    # ── Generic / Strobes ─────────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Strobe 1ch", fixture_type="Strobe",
        categories=["Strobe"],
        channels={"Strobe Rate": _strobe_rate()},
        modes=[FixtureMode("1ch", ["Strobe Rate"])],
        physical=_PHYS_STROBE,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Strobe 2ch", fixture_type="Strobe",
        categories=["Strobe"],
        channels={"Dimmer": _dim(), "Strobe Rate": _strobe_rate()},
        modes=[FixtureMode("2ch", ["Dimmer","Strobe Rate"])],
        physical=_PHYS_STROBE,
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Strobe RGB 5ch", fixture_type="Strobe",
        categories=["Strobe"],
        channels={"Dimmer": _dim(), "Strobe Rate": _strobe_rate(), "Red": _red(), "Green": _grn(), "Blue": _blu()},
        modes=[FixtureMode("5ch", ["Dimmer","Strobe Rate","Red","Green","Blue"])],
        physical=_PHYS_STROBE,
    ))

    # ── Generic / LED Strips ──────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="LED Strip RGB", fixture_type="LED Strip",
        categories=["Color Changer"],
        channels={"Red": _red(), "Green": _grn(), "Blue": _blu()},
        modes=[FixtureMode("3ch", ["Red","Green","Blue"])],
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="LED Strip RGBW", fixture_type="LED Strip",
        categories=["Color Changer"],
        channels={"Red": _red(), "Green": _grn(), "Blue": _blu(), "White": _wht()},
        modes=[FixtureMode("4ch", ["Red","Green","Blue","White"])],
    ))
    # Pixel strip — 7ch master + 6 individual channels
    px_chs = {"Master": _dim(), "Red": _red(), "Green": _grn(), "Blue": _blu(), "White": _wht(), "Amber": _amb(), "UV": _uv()}
    fxs.append(FixtureDef(
        manufacturer="Generic", model="LED Strip Pixel 7ch", fixture_type="LED Strip",
        categories=["Color Changer"],
        channels=px_chs,
        modes=[FixtureMode("7ch", ["Master","Red","Green","Blue","White","Amber","UV"])],
    ))

    # ── Generic / LED Bar (4-pixel, 16ch) ─────────────────────────────────────
    bar_chs = {}
    bar_order = []
    for seg in range(1, 5):
        for c, fn in [("R", _red), ("G", _grn), ("B", _blu), ("W", _wht)]:
            name = f"Seg{seg} {c}"
            bar_chs[name] = fn()
            bar_chs[name].name = name
            bar_order.append(name)
    fxs.append(FixtureDef(
        manufacturer="Generic", model="LED Bar 4seg RGBW 16ch", fixture_type="LED Bar",
        categories=["Color Changer"],
        channels=bar_chs,
        modes=[FixtureMode("16ch", bar_order)],
    ))

    # ── Generic / Smoke & Haze ────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Smoke Machine", fixture_type="Fogger",
        categories=["Smoke"],
        channels={"Output": _pump(), "Fan Speed": _fan()},
        modes=[FixtureMode("2ch", ["Output","Fan Speed"])],
        physical=Physical(power_w=1200),
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Hazer", fixture_type="Hazer",
        categories=["Smoke"],
        channels={
            "Haze Output": _ch("Haze Output", ChannelType.INTENSITY, [Capability(0,9,"Off"),Capability(10,255,"Low–High")]),
            "Fan Speed": _fan(),
        },
        modes=[FixtureMode("2ch", ["Haze Output","Fan Speed"])],
        physical=Physical(power_w=650),
    ))

    # ── Generic / Laser ───────────────────────────────────────────────────────
    laser_on_caps = [Capability(0,9,"Off"),Capability(10,19,"Red"),Capability(20,29,"Green"),Capability(30,39,"Blue"),Capability(40,79,"R+G"),Capability(80,119,"R+B"),Capability(120,159,"G+B"),Capability(160,255,"R+G+B")]
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Laser RGB 6ch", fixture_type="Laser",
        categories=["Laser"],
        channels={
            "On/Color": _ch("On/Color", ChannelType.COLOR_WHEEL, laser_on_caps, snap=True),
            "Red":      _red(),
            "Green":    _grn(),
            "Blue":     _blu(),
            "Pattern":  _ch("Pattern", ChannelType.GOBO_WHEEL, [Capability(0,255,"Patterns 1–32")], snap=True),
            "Rotation": _ch("Rotation", ChannelType.GOBO_ROT, [Capability(0,127,"CW fast–slow"),Capability(128,128,"Stop"),Capability(129,255,"CCW slow–fast")]),
        },
        modes=[FixtureMode("6ch", ["On/Color","Red","Green","Blue","Pattern","Rotation"])],
        physical=Physical(power_w=60),
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Laser 3ch", fixture_type="Laser",
        categories=["Laser"],
        channels={
            "On/Color": _ch("On/Color", ChannelType.COLOR_WHEEL, laser_on_caps, snap=True),
            "Pattern":  _ch("Pattern", ChannelType.GOBO_WHEEL, [Capability(0,255,"Patterns")], snap=True),
            "Rotation": _ch("Rotation", ChannelType.GOBO_ROT),
        },
        modes=[FixtureMode("3ch", ["On/Color","Pattern","Rotation"])],
    ))

    # ── Generic / Scanner ─────────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Scanner 8ch", fixture_type="Scanner",
        categories=["Scanner"],
        channels={
            "Pan":         _ch("Pan",  ChannelType.PAN,  [Capability(0,255,"Pan 0–180°")]),
            "Tilt":        _ch("Tilt", ChannelType.TILT, [Capability(0,255,"Tilt 0–90°")]),
            "Color Wheel": _color_wheel(),
            "Gobo Wheel":  _gobo_wheel(),
            "Shutter":     _shutter(),
            "Dimmer":      _dim(),
            "Speed":       _speed(),
            "Effects":     _effects(),
        },
        modes=[FixtureMode("8ch", ["Pan","Tilt","Color Wheel","Gobo Wheel","Shutter","Dimmer","Speed","Effects"])],
        physical=Physical(weight_kg=4.5, power_w=70, pan_max=180, tilt_max=90),
    ))

    # ── Generic / Flower Effect ───────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Flower Effect 5ch", fixture_type="Effect",
        categories=["Effect"],
        channels={
            "Rotation": _ch("Rotation", ChannelType.GOBO_ROT, [Capability(0,127,"CW fast–slow"),Capability(128,128,"Stop"),Capability(129,255,"CCW slow–fast")]),
            "Red":    _red(), "Green": _grn(), "Blue": _blu(),
            "Dimmer": _dim(),
        },
        modes=[FixtureMode("5ch", ["Rotation","Red","Green","Blue","Dimmer"])],
    ))

    # ── Chauvet DJ / SlimPAR Pro H ────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Chauvet DJ", model="SlimPAR Pro H", fixture_type="LED PAR",
        categories=["Color Changer"],
        channels={
            "Red": _red(), "Green": _grn(), "Blue": _blu(),
            "White": _wht(), "Amber": _amb(), "UV": _uv(),
            "Master": _dim(),
            "Strobe Rate": _strobe_rate(),
            "Color Macro": _ch("Color Macro", ChannelType.MACRO, [Capability(0,9,"Off"),Capability(10,255,"Macros 1–25")]),
            "Color Speed": _ch("Color Speed", ChannelType.SPEED),
            "Dimmer Mode": _ch("Dimmer Mode", ChannelType.MAINTENANCE, snap=True),
            "Dimmer Curve": _ch("Dimmer Curve", ChannelType.MAINTENANCE, snap=True),
        },
        modes=[
            FixtureMode("3ch",  ["Red","Green","Blue"]),
            FixtureMode("6ch",  ["Red","Green","Blue","White","Amber","UV"]),
            FixtureMode("7ch",  ["Master","Red","Green","Blue","White","Amber","UV"]),
            FixtureMode("12ch", ["Master","Red","Green","Blue","White","Amber","UV","Strobe Rate","Color Macro","Color Speed","Dimmer Mode","Dimmer Curve"]),
        ],
        physical=Physical(weight_kg=1.6, width_mm=229, height_mm=73, depth_mm=229, power_w=72, lumens=0),
    ))

    # ── Chauvet DJ / Intimidator Spot 360 ────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Chauvet DJ", model="Intimidator Spot 360", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
            "Color Wheel": _ch("Color Wheel", ChannelType.COLOR_WHEEL, [
                Capability(0,10,"White",color="#ffffff"), Capability(11,21,"Red",color="#ff2020"),
                Capability(22,32,"Green",color="#20ff20"), Capability(33,43,"Blue",color="#2020ff"),
                Capability(44,54,"Yellow",color="#ffee00"), Capability(55,65,"Magenta",color="#ff20ff"),
                Capability(66,76,"Cyan",color="#20ffff"), Capability(77,87,"Orange",color="#ff8000"),
                Capability(88,98,"Split W/R",color="#ff8080"), Capability(99,109,"Split R/G",color="#80ff40"),
                Capability(110,120,"Split G/B",color="#40c0c0"), Capability(121,131,"Split B/Y",color="#8080ff"),
                Capability(132,142,"Split Y/M",color="#ff80ff"), Capability(143,153,"Split M/C",color="#80ffff"),
                Capability(154,255,"Rotation CW–CCW"),
            ], snap=True),
            "Gobo Wheel": _ch("Gobo Wheel", ChannelType.GOBO_WHEEL, [
                Capability(0,7,"Open"), Capability(8,15,"Gobo 1"), Capability(16,23,"Gobo 2"),
                Capability(24,31,"Gobo 3"), Capability(32,39,"Gobo 4"), Capability(40,47,"Gobo 5"),
                Capability(48,55,"Gobo 6"), Capability(56,63,"Gobo 7"),
                Capability(64,127,"Gobo shake"), Capability(128,255,"Rotation CW–CCW"),
            ], snap=True),
            "Shutter": _shutter(), "Dimmer": _dim(), "Dimmer Fine": _dimf(),
            "Focus": _focus(), "Speed": _speed(),
            "Reset": _reset(),
            "Effects": _ch("Effects", ChannelType.EFFECTS, [
                Capability(0,9,"Off"), Capability(10,59,"Effect 1"), Capability(60,109,"Effect 2"),
                Capability(110,159,"Effect 3"), Capability(160,209,"Effect 4"), Capability(210,255,"Effect 5"),
            ]),
        },
        modes=[
            FixtureMode("9ch",  ["Pan","Tilt","Color Wheel","Gobo Wheel","Shutter","Dimmer","Focus","Speed","Reset"]),
            FixtureMode("14ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Shutter","Dimmer","Dimmer Fine","Focus","Speed","Reset","Effects","Effects"]),
        ],
        physical=Physical(weight_kg=6.35, width_mm=245, height_mm=377, depth_mm=195, power_w=85, lumens=7700, pan_max=540, tilt_max=270, lens_min=12, lens_max=12),
    ))

    # ── American DJ / Vizi Beam 5RX ───────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="American DJ", model="Vizi Beam 5RX", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
            "Color Wheel": _ch("Color Wheel", ChannelType.COLOR_WHEEL, [
                Capability(0,7,"White",color="#ffffff"), Capability(8,14,"Red",color="#ff0000"),
                Capability(15,21,"Orange",color="#ff8000"), Capability(22,28,"Yellow",color="#ffee00"),
                Capability(29,35,"Green",color="#00ff00"), Capability(36,42,"Cyan",color="#00ffff"),
                Capability(43,49,"Blue",color="#0000ff"), Capability(50,56,"Magenta",color="#ff00ff"),
                Capability(57,63,"UV",color="#8000ff"), Capability(64,127,"Split colors"),
                Capability(128,191,"CW rotation"), Capability(192,255,"CCW rotation"),
            ], snap=True),
            "Gobo Wheel": _gobo_wheel(),
            "Prism": _ch("Prism", ChannelType.PRISM, [
                Capability(0,9,"No prism"), Capability(10,127,"Prism static"),
                Capability(128,191,"Prism CW"), Capability(192,255,"Prism CCW"),
            ], snap=True),
            "Focus": _focus(), "Shutter": _shutter(), "Dimmer": _dim(),
            "Speed": _speed(), "Effect": _effects(),
            "Reset": _reset(),
            "Lamp": _ch("Lamp", ChannelType.MAINTENANCE, [Capability(0,9,"Off"),Capability(10,255,"On")], snap=True),
        },
        modes=[
            FixtureMode("12ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Prism","Focus","Shutter","Dimmer","Speed","Reset"]),
            FixtureMode("14ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Color Wheel","Gobo Wheel","Prism","Focus","Shutter","Dimmer","Speed","Effect","Reset","Lamp"]),
        ],
        physical=Physical(weight_kg=11.0, width_mm=290, height_mm=430, depth_mm=260, power_w=200, lumens=20000, pan_max=540, tilt_max=270, lens_min=2, lens_max=2),
    ))

    # ── Martin / MAC Aura XB ──────────────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Martin", model="MAC Aura XB", fixture_type="Moving Head",
        categories=["Moving Head"],
        channels={
            "Pan": _pan(), "Pan Fine": _panf(), "Tilt": _tilt(), "Tilt Fine": _tiltf(),
            "Pan/Tilt Speed": _speed(),
            "Red":   _red(), "Red Fine":   _redf(),
            "Green": _grn(), "Green Fine": _grnf(),
            "Blue":  _blu(), "Blue Fine":  _bluf(),
            "CTO":   _cto(),
            "Dimmer": _dim(), "Dimmer Fine": _dimf(),
            "Shutter": _shutter(),
            "Zoom": _zoom(),
            "Outer Red":   _ch("Outer Red",   ChannelType.RED),
            "Outer Green": _ch("Outer Green", ChannelType.GREEN),
            "Outer Blue":  _ch("Outer Blue",  ChannelType.BLUE),
            "Outer CTO":   _ch("Outer CTO",   ChannelType.CTO),
            "Outer Dimmer": _ch("Outer Dimmer", ChannelType.INTENSITY),
            "Outer Shutter": _ch("Outer Shutter", ChannelType.SHUTTER, _shutter_caps()),
            "Reset": _reset(),
        },
        modes=[
            FixtureMode("17ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Pan/Tilt Speed","Red","Green","Blue","CTO","Dimmer","Shutter","Zoom","Outer Red","Outer Green","Outer Blue","Outer Dimmer","Reset"]),
            FixtureMode("22ch", ["Pan","Pan Fine","Tilt","Tilt Fine","Pan/Tilt Speed","Red","Red Fine","Green","Green Fine","Blue","Blue Fine","CTO","Dimmer","Dimmer Fine","Shutter","Zoom","Outer Red","Outer Green","Outer Blue","Outer CTO","Outer Dimmer","Outer Shutter"]),
        ],
        physical=Physical(weight_kg=9.0, width_mm=296, height_mm=452, depth_mm=237, power_w=145, lumens=5600, pan_max=540, tilt_max=270, lens_min=4, lens_max=60),
    ))

    # ── Generic / Effects / Specials ──────────────────────────────────────────
    fxs.append(FixtureDef(
        manufacturer="Generic", model="UV Flood", fixture_type="UV",
        categories=["Color Changer"],
        channels={"Dimmer": _dim()},
        modes=[FixtureMode("1ch", ["Dimmer"])],
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Pin Spot", fixture_type="Spot",
        categories=["Dimmer"],
        channels={"Dimmer": _dim()},
        modes=[FixtureMode("1ch", ["Dimmer"])],
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="LED Moonflower", fixture_type="Effect",
        categories=["Effect"],
        channels={
            "Rotation": _ch("Rotation", ChannelType.GOBO_ROT),
            "Color":    _ch("Color", ChannelType.COLOR_WHEEL, [Capability(0,9,"Stop"),Capability(10,127,"CW fast–slow"),Capability(128,255,"CCW slow–fast")]),
            "Strobe":   _strobe_rate(),
            "Dimmer":   _dim(),
        },
        modes=[FixtureMode("4ch", ["Rotation","Color","Strobe","Dimmer"])],
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Follow Spot", fixture_type="Spot",
        categories=["Dimmer"],
        channels={
            "Dimmer":  _dim(),
            "Color":   _color_wheel(),
            "Iris":    _iris(),
            "Shutter": _shutter(),
        },
        modes=[FixtureMode("4ch", ["Dimmer","Color","Iris","Shutter"])],
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Blinder 2ch", fixture_type="Blinder",
        categories=["Blinder"],
        channels={
            "Dimmer":  _dim(),
            "Shutter": _shutter(),
        },
        modes=[FixtureMode("2ch", ["Dimmer","Shutter"])],
        physical=Physical(power_w=800),
    ))
    fxs.append(FixtureDef(
        manufacturer="Generic", model="Blinder 4cell", fixture_type="Blinder",
        categories=["Blinder"],
        channels={
            "Cell 1": _ch("Cell 1", ChannelType.INTENSITY),
            "Cell 2": _ch("Cell 2", ChannelType.INTENSITY),
            "Cell 3": _ch("Cell 3", ChannelType.INTENSITY),
            "Cell 4": _ch("Cell 4", ChannelType.INTENSITY),
            "Master": _dim(),
            "Shutter": _shutter(),
        },
        modes=[
            FixtureMode("2ch", ["Master","Shutter"]),
            FixtureMode("6ch", ["Master","Shutter","Cell 1","Cell 2","Cell 3","Cell 4"]),
        ],
        physical=Physical(power_w=1600),
    ))

    return fxs


BUILTIN_FIXTURES: list[FixtureDef] = _build_fixtures()
