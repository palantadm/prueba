//! External programs over OSC: targets (where they are), the user's own programs, the virtual-console OSC action and
//! "put this ready-made action on the console". The catalog and address filling live in [`crate::oscpresets`].

use super::script::{osc_arg, valid_osc_target};
use super::*;
use crate::oscpresets;

impl Engine {
    fn osc_endpoint(&self, target: u32, host: &str, port: u16) -> Result<(String, u16), String> {
        if target == 0 {
            return Ok((host.to_string(), port));
        }
        self.show.osc_targets.iter().find(|t| t.id == target).map(|t| (t.host.clone(), t.port)).ok_or_else(|| format!("El destino OSC {target} no existe; créalo en Ajustes → Programas externos"))
    }

    /// Send `address` + fixed `args` (+ `v` per `value`) to a target. Used by the console widgets and by `OscSend`.
    pub(super) fn osc_emit(&mut self, target: u32, host: &str, port: u16, address: &str, args: &[String], value: OscValue, v: f32) -> Result<(), String> {
        let (host, port) = self.osc_endpoint(target, host, port)?;
        valid_osc_target(&host, port, address, args)?;
        let mut a: Vec<rosc::OscType> = args.iter().map(|s| osc_arg(s)).collect();
        match value {
            OscValue::Float => a.push(rosc::OscType::Float(if v.is_finite() { v } else { 0.0 })),
            OscValue::Int => a.push(rosc::OscType::Int(if v.is_finite() { v.round() as i32 } else { 0 })),
            OscValue::None => {}
        }
        if self.io.osc_send_args(&host, port, address, a) {
            Ok(())
        } else {
            Err(format!("No se pudo enviar OSC a {host}:{port} (¿nombre o IP incorrectos?)"))
        }
    }

    fn catalog(&self) -> Vec<OscSoftware> {
        oscpresets::all(&self.show.osc_software)
    }

    pub(super) fn osc_command(&mut self, c: Command) -> Result<(), String> {
        match c {
            Command::OscTargetSave { mut target } => {
                target.name = target.name.trim().chars().take(60).collect();
                target.host = target.host.trim().to_string();
                if target.name.is_empty() {
                    return Err("El destino necesita un nombre".into());
                }
                valid_osc_target(&target.host, target.port, "/x", &[])?;
                if !target.software.is_empty() && !self.catalog().iter().any(|s| s.id == target.software) {
                    return Err(format!("No hay un programa «{}» (déjalo vacío para uno genérico)", target.software));
                }
                if target.id == 0 {
                    target.id = next_id(self.show.osc_targets.iter().map(|t| t.id));
                }
                match self.show.osc_targets.iter_mut().find(|t| t.id == target.id) {
                    Some(t) => *t = target,
                    None => self.show.osc_targets.push(target),
                }
                self.touch();
            }
            Command::OscTargetRemove { id } => {
                let used = self.show.vconsole.iter().flat_map(|p| p.widgets.iter()).filter(|w| matches!(&w.action, VcAction::Osc { target, .. } if *target == id)).count();
                if used > 0 {
                    return Err(format!("{used} control(es) de la consola virtual usan este destino; bórralos o cámbialos antes"));
                }
                self.show.osc_targets.retain(|t| t.id != id);
                self.touch();
            }
            Command::OscSoftwareSave { software } => {
                oscpresets::validate(&software)?;
                match self.show.osc_software.iter_mut().find(|s| s.id == software.id) {
                    Some(s) => *s = software,
                    None => self.show.osc_software.push(software),
                }
                self.touch();
            }
            Command::OscSoftwareRemove { id } => {
                if self.show.osc_targets.iter().any(|t| t.software == id) {
                    return Err("Hay destinos que usan este programa; bórralos antes".into());
                }
                self.show.osc_software.retain(|s| s.id != id);
                self.touch();
            }
            Command::OscSend { target, action, params, value } => {
                let cat = self.catalog();
                let (_, a) = oscpresets::find(&cat, &action)?;
                let (addr, args) = oscpresets::resolve(a, &params)?;
                let v = value.unwrap_or(a.max);
                self.osc_emit(target, "", 0, &addr, &args, a.value, v)?;
            }
            Command::VcAddOsc { target, action, params, page, label } => self.vc_add_osc(target, &action, &params, page, label)?,
            _ => {}
        }
        Ok(())
    }

    /// Free `w`×`h` spot on a page, scanning row by row; the page grows downwards when full.
    fn vc_free_spot(page: &mut VcPage, w: u16, h: u16) -> (u16, u16) {
        let w = w.min(page.cols).max(1);
        let busy = |x: u16, y: u16, pg: &VcPage| pg.widgets.iter().any(|o| x < o.x + o.w && o.x < x + w && y < o.y + o.h && o.y < y + h);
        let mut y = 0;
        loop {
            if y + h > page.rows {
                if page.rows >= 64 {
                    return (0, 0);
                }
                page.rows = (y + h).min(64);
            }
            for x in 0..=(page.cols - w) {
                if !busy(x, y, page) {
                    return (x, y);
                }
            }
            y += 1;
        }
    }

    fn vc_add_osc(&mut self, target: u32, action: &str, params: &BTreeMap<String, String>, page: Option<u32>, label: Option<String>) -> Result<(), String> {
        if !self.show.osc_targets.iter().any(|t| t.id == target) {
            return Err(format!("El destino OSC {target} no existe; créalo primero (osc_target_save)"));
        }
        let cat = self.catalog();
        let (sw, a) = oscpresets::find(&cat, action)?;
        let (address, args) = oscpresets::resolve(a, params)?;
        if self.show.vconsole.is_empty() {
            self.show.vconsole.push(VcPage { id: 1, name: "Page 1".into(), cols: 12, rows: 8, widgets: vec![] });
        }
        let pi = match page {
            Some(id) => self.show.vconsole.iter().position(|p| p.id == id).ok_or_else(|| format!("La página {id} de la consola virtual no existe"))?,
            None => 0,
        };
        let wid = self.show.vconsole.iter().flat_map(|p| p.widgets.iter().map(|w| w.id)).max().unwrap_or(0) + 1;
        let fader = a.kind == OscKind::Fader;
        let default_label = {
            let vals: Vec<String> = a.params.iter().map(|p| params.get(&p.key).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| p.default.clone())).collect();
            if vals.is_empty() { format!("{} · {}", sw.name.split(' ').next().unwrap_or(&sw.name), a.label) } else { format!("{} {}", a.label, vals.join(".")) }
        };
        let (w, h) = if fader { (1, 4) } else { (2, 1) };
        let pg = &mut self.show.vconsole[pi];
        let (x, y) = Self::vc_free_spot(pg, w, h);
        pg.widgets.push(VcWidget {
            id: wid,
            kind: if fader { VcKind::Slider } else { VcKind::Button },
            label: label.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).unwrap_or(default_label).chars().take(60).collect(),
            x,
            y,
            w,
            h,
            color: "#7c9cff".into(),
            action: VcAction::Osc { target, host: String::new(), port: 0, address, args, value: a.value },
            mode: if a.kind == OscKind::Toggle { VcMode::Toggle } else { VcMode::Trigger },
            orientation: "vertical".into(),
            inputs: vec![],
            feedback: vec![],
            out_min: a.min,
            out_max: a.max,
        });
        self.vc.force_refresh = true;
        self.touch();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;
    use rosc::{OscPacket, OscType};
    use std::net::UdpSocket;
    use std::time::Duration;

    fn listener() -> (UdpSocket, u16) {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        s.set_read_timeout(Some(Duration::from_millis(1000))).unwrap();
        let port = s.local_addr().unwrap().port();
        (s, port)
    }
    fn got(s: &UdpSocket) -> (String, Vec<OscType>) {
        let mut buf = [0u8; 512];
        let (n, _) = s.recv_from(&mut buf).expect("an OSC message");
        match rosc::decoder::decode_udp(&buf[..n]).unwrap().1 {
            OscPacket::Message(m) => (m.addr, m.args),
            p => panic!("{p:?}"),
        }
    }
    fn target(e: &mut Engine, software: &str, port: u16) -> u32 {
        e.handle(Command::OscTargetSave { target: OscTarget { id: 0, name: "PC vídeo".into(), software: software.into(), host: "127.0.0.1".into(), port } }).unwrap();
        e.show.osc_targets.last().unwrap().id
    }
    fn add(e: &mut Engine, t: u32, action: &str, params: &[(&str, &str)]) -> u32 {
        let params = params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        e.handle(Command::VcAddOsc { target: t, action: action.into(), params, page: None, label: None }).unwrap();
        e.show.vconsole[0].widgets.last().unwrap().id
    }

    #[test]
    fn a_resolume_button_triggers_a_clip_and_a_fader_sets_opacity() {
        let mut e = engine();
        let (rx, port) = listener();
        let t = target(&mut e, "resolume", port);
        let b = add(&mut e, t, "resolume/clip_connect", &[("layer", "2"), ("clip", "3")]);
        let f = add(&mut e, t, "resolume/layer_opacity", &[("layer", "2")]);
        let w = e.show.vconsole[0].widgets.iter().find(|w| w.id == b).unwrap();
        assert_eq!(w.label, "Lanzar clip 2.3");
        assert!(matches!(w.kind, VcKind::Button) && w.mode == VcMode::Trigger);
        e.handle(Command::VcPress { widget: b, down: true }).unwrap();
        assert_eq!(got(&rx), ("/composition/layers/2/clips/3/connect".to_string(), vec![OscType::Int(1)]));
        // a trigger sends nothing on release
        e.handle(Command::VcPress { widget: b, down: false }).unwrap();
        e.handle(Command::VcValue { widget: f, value: 0.25 }).unwrap();
        assert_eq!(got(&rx), ("/composition/layers/2/video/opacity".to_string(), vec![OscType::Float(0.25)]));
        assert_ne!((e.show.vconsole[0].widgets[0].x, e.show.vconsole[0].widgets[0].y), (e.show.vconsole[0].widgets[1].x, e.show.vconsole[0].widgets[1].y), "widgets do not overlap");
    }

    #[test]
    fn qlab_go_has_no_value_and_ableton_leads_with_integer_arguments() {
        let mut e = engine();
        let (rx, port) = listener();
        let t = target(&mut e, "", port);
        let go = add(&mut e, t, "qlab/go", &[]);
        let fire = add(&mut e, t, "ableton/clip_fire", &[("track", "1"), ("clip", "4")]);
        let tempo = add(&mut e, t, "ableton/tempo", &[]);
        e.handle(Command::VcPress { widget: go, down: true }).unwrap();
        assert_eq!(got(&rx), ("/go".to_string(), vec![]));
        e.handle(Command::VcPress { widget: fire, down: true }).unwrap();
        assert_eq!(got(&rx), ("/live/clip/fire".to_string(), vec![OscType::Int(1), OscType::Int(4)]));
        e.handle(Command::VcValue { widget: tempo, value: 0.5 }).unwrap();
        let (a, v) = got(&rx);
        assert_eq!(a, "/live/song/set/tempo");
        assert_eq!(v, vec![OscType::Float(130.0)], "fader range 60–200 BPM");
    }

    #[test]
    fn toggle_sends_on_then_off_and_a_repeated_fader_value_is_not_resent() {
        let mut e = engine();
        let (rx, port) = listener();
        let t = target(&mut e, "resolume", port);
        let b = add(&mut e, t, "resolume/layer_bypass", &[]);
        e.handle(Command::VcPress { widget: b, down: true }).unwrap();
        assert_eq!(got(&rx).1, vec![OscType::Int(1)]);
        e.handle(Command::VcPress { widget: b, down: false }).unwrap();
        e.handle(Command::VcPress { widget: b, down: true }).unwrap();
        assert_eq!(got(&rx).1, vec![OscType::Int(0)]);
        let f = add(&mut e, t, "resolume/crossfader", &[]);
        e.handle(Command::VcValue { widget: f, value: 0.5 }).unwrap();
        e.handle(Command::VcValue { widget: f, value: 0.5 }).unwrap();
        assert_eq!(got(&rx).1, vec![OscType::Float(0.5)]);
        rx.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let mut buf = [0u8; 64];
        assert!(rx.recv_from(&mut buf).is_err(), "only one message for a repeated value");
    }

    #[test]
    fn targets_and_user_software_are_validated() {
        let mut e = engine();
        let t = target(&mut e, "resolume", 7000);
        assert!(e.handle(Command::OscTargetSave { target: OscTarget { id: 0, name: "x".into(), software: "nope".into(), host: "1.2.3.4".into(), port: 1 } }).is_err());
        assert!(e.handle(Command::OscTargetSave { target: OscTarget { id: 0, name: "x".into(), software: "".into(), host: "1.2.3.4".into(), port: 0 } }).is_err());
        assert!(e.handle(Command::VcAddOsc { target: 99, action: "qlab/go".into(), params: Default::default(), page: None, label: None }).unwrap_err().contains("no existe"));
        assert!(e.handle(Command::VcAddOsc { target: t, action: "qlab/zzz".into(), params: Default::default(), page: None, label: None }).is_err());
        // a target in use cannot be removed
        let b = add(&mut e, t, "resolume/disconnect_all", &[]);
        assert!(e.handle(Command::OscTargetRemove { id: t }).unwrap_err().contains("consola virtual"));
        let _ = b;
        // user software becomes usable like the catalog
        let sw = OscSoftware { id: "mesa".into(), name: "Mesa de sonido".into(), port: 9000, note: String::new(), builtin: false, actions: vec![OscAction { id: "mute".into(), label: "Mute".into(), group: String::new(), address: "/ch/{n}/mute".into(), args: vec![], value: OscValue::Int, kind: OscKind::Toggle, params: vec![OscParam { key: "n".into(), label: "Canal".into(), default: "1".into() }], min: 0.0, max: 1.0, note: String::new() }] };
        e.handle(Command::OscSoftwareSave { software: sw }).unwrap();
        let id = add(&mut e, t, "mesa/mute", &[("n", "7")]);
        let w = e.show.vconsole[0].widgets.iter().find(|w| w.id == id).unwrap();
        assert!(matches!(&w.action, VcAction::Osc { address, .. } if address == "/ch/7/mute"));
        assert!(e.handle(Command::OscSoftwareSave { software: OscSoftware { id: "qlab".into(), name: "x".into(), port: 1, note: String::new(), builtin: false, actions: vec![] } }).is_err(), "cannot shadow the catalog");
    }
}
