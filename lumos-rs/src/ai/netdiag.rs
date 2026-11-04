//! Output/network diagnosis for the assistant: reads the universe configuration, the live output status, the Art-Net
//! nodes that answered the latest ArtPoll and this machine's addresses, and turns them into concrete findings with a fix.
//!
//! Pure (no I/O) so it can be unit tested; the tool executor in [`super::tools`] gathers the inputs.

use super::AiState;
use crate::io::{port_address, ARTNET_PORT};
use crate::model::*;
use serde_json::{json, Value};
use std::net::Ipv4Addr;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Sev {
    Error,
    Warn,
    Info,
}

impl Sev {
    fn name(self) -> &'static str {
        match self {
            Sev::Error => "error",
            Sev::Warn => "aviso",
            Sev::Info => "info",
        }
    }
}

struct Finding {
    sev: Sev,
    scope: String,
    problem: String,
    fix: String,
}

fn f(sev: Sev, scope: &str, problem: impl Into<String>, fix: impl Into<String>) -> Finding {
    Finding { sev, scope: scope.into(), problem: problem.into(), fix: fix.into() }
}

fn same_subnet(a: Ipv4Addr, b: Ipv4Addr) -> bool {
    let (x, y) = (a.octets(), b.octets());
    // Art-Net 2.x / 10.x networks use a /8 mask by convention; everything else is treated as /24.
    if (x[0] == 2 || x[0] == 10) && x[0] == y[0] {
        return true;
    }
    x[..3] == y[..3]
}

fn fmt_pa(pa: u16) -> String {
    format!("{pa} (Net {}, Sub {}, Uni {})", pa >> 8, (pa >> 4) & 0xF, pa & 0xF)
}

fn artnet_checks(out: &mut Vec<Finding>, st: &AiState, scope: &str, host: &str, port: u16, pa: u16, lumos_universe: u16) {
    let host = host.trim();
    if port != ARTNET_PORT {
        out.push(f(Sev::Warn, scope, format!("Puerto {port}: Art-Net usa el UDP {ARTNET_PORT} y casi todos los nodos solo escuchan ahí."), format!("Pon el puerto en {ARTNET_PORT} salvo que el nodo esté configurado a propósito en otro.")));
    }
    if pa == 0 && lumos_universe >= 1 {
        out.push(f(
            Sev::Info,
            scope,
            format!("El universo {lumos_universe} de Lumos sale por el port-address 0 (Net 0 / Sub 0 / Uni 0)."),
            "Algunos nodos y pantallas numeran desde 0 y otros desde 1: si el nodo espera «universo 1», sube Uni a 1 (y viceversa). Comprueba qué port-address tiene el puerto del nodo.",
        ));
    }
    let local: Vec<Ipv4Addr> = st.local_ips.iter().filter_map(|s| s.parse().ok()).collect();
    if local.is_empty() {
        out.push(f(Sev::Error, scope, "Este equipo no tiene ninguna dirección IPv4 de red (solo loopback).", "Conecta el cable/Wi-Fi o asigna una IP fija al adaptador que va a la red de iluminación."));
    } else if local.iter().all(|ip| ip.octets()[0] == 169 && ip.octets()[1] == 254) {
        out.push(f(Sev::Warn, scope, "Todas las IP de este equipo son 169.254.x.x (autoasignadas): no hay DHCP.", "Pon una IP fija en el adaptador (p. ej. 2.0.0.10/255.0.0.0 o 192.168.1.10/255.255.255.0) en la misma red que el nodo."));
    }
    if host.is_empty() {
        out.push(f(
            Sev::Info,
            scope,
            "Sin host: se envía por broadcast.",
            "Si hay varios adaptadores (Wi-Fi + cable, VPN, máquinas virtuales) el broadcast puede salir por el equivocado, y muchos switches/routers lo limitan. Para fiarte, pon la IP del nodo (unicast).",
        ));
        return;
    }
    match host.parse::<Ipv4Addr>() {
        Err(_) => out.push(f(Sev::Info, scope, format!("«{host}» es un nombre de host, no una IP."), "Si no resuelve (ver estado de la salida), usa la IP del nodo directamente.")),
        Ok(ip) => {
            if ip.is_unspecified() || ip.is_multicast() {
                out.push(f(Sev::Error, scope, format!("{ip} no es una dirección de destino válida para Art-Net."), "Usa la IP del nodo, o déjalo vacío para broadcast."));
            } else if ip.is_loopback() {
                out.push(f(Sev::Warn, scope, "El destino es 127.0.0.1: la salida no sale de este equipo.", "Pon la IP real del nodo (solo es correcto para pruebas con otro programa en este mismo PC)."));
            } else if ip != Ipv4Addr::BROADCAST && !ip.octets().ends_with(&[255]) && !local.is_empty() && !local.iter().any(|l| same_subnet(*l, ip)) {
                out.push(f(
                    Sev::Warn,
                    scope,
                    format!("El nodo {ip} no está en la misma subred que ninguna IP de este equipo ({}).", st.local_ips.join(", ")),
                    "Cambia la IP del PC para que comparta red con el nodo (misma máscara) o la del nodo. Sin enrutado, un nodo en otra subred no recibe nada.",
                ));
            }
            // Node table
            let node = st.nodes.iter().find(|n| n.ip == host);
            match node {
                Some(n) if n.universes.contains(&pa) => out.push(f(Sev::Info, scope, format!("OK: el nodo «{}» ({}) responde y tiene un puerto de salida en el port-address {pa}.", n.name, n.ip), "")),
                Some(n) => out.push(f(
                    Sev::Error,
                    scope,
                    format!("El nodo «{}» ({}) responde, pero sus puertos de salida están en {:?} y Lumos envía al port-address {}.", n.name, n.ip, n.universes, fmt_pa(pa)),
                    "Ajusta Net/Sub/Uni de esta salida a uno de esos port-address (universe_save), o reconfigura el puerto del nodo para que escuche el que envía Lumos.",
                )),
                None => out.push(f(
                    Sev::Warn,
                    scope,
                    format!("El nodo {ip} no ha respondido a ArtPoll."),
                    "Comprueba: IP y cable/Wi-Fi, que el nodo esté encendido, firewall del PC (UDP 6454 entrada y salida) y misma subred. Ojo: algunos nodos baratos o caseros no responden a ArtPoll aunque reciban datos; confírmalo con un output_test.",
                )),
            }
        }
    }
}

/// Analyse the output chain. `serial`: names of the serial ports present. `universe`: restrict to one universe.
pub fn analyze(st: &AiState, serial: &[String], universe: Option<u16>) -> Value {
    let mut out: Vec<Finding> = Vec::new();
    let sh = &st.show;

    if let Some(u) = universe {
        if !sh.universes.iter().any(|x| x.id == u) {
            out.push(f(Sev::Error, &format!("universo {u}"), "Ese universo no existe en el show.", "Créalo con universe_save (id, nombre y al menos una salida)."));
        }
    }
    if sh.universes.is_empty() {
        out.push(f(Sev::Error, "show", "El show no tiene universos: no se envía nada.", "Crea un universo con una salida (Ajustes → Universos, o universe_save)."));
    }
    // global switches that make every output look dead
    if st.rt.testing {
        out.push(f(Sev::Warn, "global", "Hay un test de salida activo: sustituye a la salida normal.", "Apágalo con output_test {mode: off}."));
    }
    if st.rt.blackout {
        out.push(f(Sev::Warn, "global", "Blackout activo: las salidas envían ceros.", "blackout off."));
    }
    if st.rt.grand_master < 0.05 {
        out.push(f(Sev::Warn, "global", format!("Master general al {:.0}%.", st.rt.grand_master * 100.0), "gm 100."));
    }
    if !st.rt.parked.is_empty() {
        out.push(f(Sev::Info, "global", format!("{} canal(es) aparcado(s) fijan su valor.", st.rt.parked.len()), "unpark si no es intencionado."));
    }
    // fixtures in universes that do not exist
    for u in sh.fixtures.iter().map(|x| x.universe).collect::<std::collections::BTreeSet<_>>() {
        if !sh.universes.iter().any(|x| x.id == u) && universe.map_or(true, |w| w == u) {
            out.push(f(Sev::Error, &format!("universo {u}"), format!("Hay aparatos parcheados en el universo {u}, que no existe en el show."), "Crea el universo o mueve los aparatos con fixture_update."));
        }
    }

    // input side
    if !st.rt.inputs.artnet_error.is_empty() {
        out.push(f(Sev::Error, "entrada Art-Net", st.rt.inputs.artnet_error.clone(), "Otro programa puede estar usando el UDP 6454, o falta permiso/firewall."));
    }
    if !st.rt.inputs.osc_error.is_empty() {
        out.push(f(Sev::Error, "entrada OSC", st.rt.inputs.osc_error.clone(), "Cambia el puerto OSC o cierra el programa que lo ocupa."));
    }

    let mut seen_dest: Vec<(String, u16, String)> = Vec::new(); // (host, pa, scope) for duplicate detection
    let mut seen_sacn: Vec<(u16, u16)> = Vec::new(); // (sacn universe, lumos universe)
    let mut per_universe: Vec<Value> = Vec::new();

    for u in sh.universes.iter().filter(|u| universe.map_or(true, |w| w == u.id)) {
        let patched = sh.fixtures.iter().filter(|x| x.universe == u.id).count();
        let frame = st.frames.iter().find(|(id, _)| *id == u.id).map(|(_, d)| d.as_slice());
        let nonzero = frame.map(|d| d.iter().filter(|b| **b != 0).count()).unwrap_or(0);
        let uscope = format!("universo {}", u.id);
        if u.outputs.is_empty() {
            out.push(f(Sev::Warn, &uscope, "Sin ninguna salida configurada: los datos no salen de Lumos.", "Añade una salida (Art-Net/sACN/serie) con universe_save."));
        } else if u.outputs.iter().all(|o| !o.enabled) {
            out.push(f(Sev::Warn, &uscope, "Todas sus salidas están desactivadas.", "Pon enabled=true en la salida que quieras usar (universe_save)."));
        }
        if patched == 0 {
            out.push(f(Sev::Info, &uscope, "No hay aparatos parcheados en este universo: la trama sale vacía.", "Parchea aparatos aquí o comprueba el universo de cada aparato."));
        } else if nonzero == 0 && !st.rt.blackout && !st.rt.testing {
            out.push(f(Sev::Info, &uscope, "Hay aparatos pero la trama está toda a 0 en este momento.", "Es normal si nada está encendido; sube un dimmer o usa output_test para probar solo la red."));
        }
        let mut outs: Vec<Value> = Vec::new();
        for (i, o) in u.outputs.iter().enumerate() {
            let scope = format!("universo {} · salida {}", u.id, i);
            let status = st.rt.outputs.iter().find(|s| s.universe == u.id && s.index as usize == i);
            let kind_name = match &o.kind {
                OutputKind::ArtNet { .. } => "artnet",
                OutputKind::Sacn { .. } => "sacn",
                OutputKind::Osc { .. } => "osc",
                OutputKind::Serial { .. } => "serie",
                OutputKind::Usb { .. } => "usb",
            };
            if let Some(s) = status {
                match s.state {
                    OutState::Error => out.push(f(Sev::Error, &scope, format!("La salida está en error: {}", s.message), "Corrige la causa que indica el mensaje (host que no resuelve, puerto ocupado, adaptador sin red, dispositivo desconectado…).")),
                    OutState::Off if o.enabled => out.push(f(Sev::Warn, &scope, format!("Salida activada pero sin enviar{}.", if s.message.is_empty() { String::new() } else { format!(" ({})", s.message) }), "Espera un instante y vuelve a mirar; si sigue así, desactívala y actívala de nuevo.")),
                    OutState::Ok if s.frames == 0 => out.push(f(Sev::Warn, &scope, "La salida está abierta pero no ha enviado ninguna trama.", "Comprueba que el motor corre (fps > 0) y vuelve a mirar en unos segundos.")),
                    _ => {}
                }
            }
            if o.enabled {
                match &o.kind {
                    OutputKind::ArtNet { host, port, net, subnet, universe: uni } => {
                        let pa = port_address(*net, *subnet, *uni);
                        artnet_checks(&mut out, st, &scope, host, *port, pa, u.id);
                        let key = (host.trim().to_string(), pa);
                        if let Some((_, _, other)) = seen_dest.iter().find(|(h, p, _)| *h == key.0 && *p == key.1) {
                            out.push(f(Sev::Warn, &scope, format!("Mismo destino y port-address que {other}: dos universos de Lumos se pisan en el mismo puerto del nodo."), "Dale a cada universo de Lumos un port-address distinto (Net/Sub/Uni)."));
                        }
                        seen_dest.push((key.0, key.1, scope.clone()));
                    }
                    OutputKind::Sacn { universe: su, priority, unicast } => {
                        if *su == 0 || *su > 63999 {
                            out.push(f(Sev::Error, &scope, format!("Universo sACN {su} fuera de rango (1–63999)."), "Usa un número entre 1 y 63999."));
                        }
                        if *priority == 0 || *priority > 200 {
                            out.push(f(Sev::Warn, &scope, format!("Prioridad sACN {priority}: el estándar usa 1–200 (100 por defecto)."), "Pon 100 salvo que necesites ganar/perder frente a otra consola."));
                        }
                        if unicast.as_deref().map_or(true, |s| s.trim().is_empty()) {
                            out.push(f(Sev::Info, &scope, format!("sACN por multicast 239.255.{}.{}.", su >> 8, su & 0xFF), "Necesita IGMP en el switch y que el adaptador correcto envíe multicast; el Wi-Fi suele perderlo. Si el receptor no ve datos, prueba unicast a su IP."));
                        } else if let Some(h) = unicast.as_deref().and_then(|s| s.trim().parse::<Ipv4Addr>().ok()) {
                            let local: Vec<Ipv4Addr> = st.local_ips.iter().filter_map(|s| s.parse().ok()).collect();
                            if !local.is_empty() && !h.is_loopback() && !local.iter().any(|l| same_subnet(*l, h)) {
                                out.push(f(Sev::Warn, &scope, format!("El receptor sACN {h} no está en la misma subred que este equipo ({}).", st.local_ips.join(", ")), "Pon el PC y el receptor en la misma red."));
                            }
                        }
                        if let Some((_, other)) = seen_sacn.iter().find(|(x, _)| x == su) {
                            out.push(f(Sev::Warn, &scope, format!("El universo sACN {su} ya lo usa el universo {other} de Lumos."), "Cada universo sACN debe tener un número distinto."));
                        }
                        seen_sacn.push((*su, u.id));
                    }
                    OutputKind::Osc { host, port, .. } => {
                        if host.trim().is_empty() || *port == 0 {
                            out.push(f(Sev::Error, &scope, "La salida OSC no tiene host o puerto.", "Indica host y puerto del receptor."));
                        }
                    }
                    OutputKind::Serial { port, .. } => {
                        if port.trim().is_empty() {
                            out.push(f(Sev::Error, &scope, "No hay puerto serie elegido.", "Elige el puerto del adaptador (Ajustes → Universos)."));
                        } else if !serial.iter().any(|p| p == port) {
                            out.push(f(Sev::Warn, &scope, format!("El puerto «{port}» no aparece entre los puertos serie detectados ({}).", if serial.is_empty() { "ninguno".into() } else { serial.join(", ") }), "Reconecta el adaptador, instala su driver (FTDI/CH340) o elige el puerto correcto. En Linux hace falta pertenecer al grupo dialout/uucp."));
                        }
                    }
                    OutputKind::Usb { .. } => {}
                }
            }
            outs.push(json!({
                "index": i, "kind": kind_name, "enabled": o.enabled,
                "estado": status.map(|s| match s.state { OutState::Ok => "ok", OutState::Off => "off", OutState::Error => "error" }),
                "mensaje": status.map(|s| s.message.clone()), "tramas": status.map(|s| s.frames),
                "config": serde_json::to_value(&o.kind).unwrap_or_default(),
            }));
        }
        per_universe.push(json!({ "id": u.id, "name": u.name, "fixtures": patched, "channels_non_zero": nonzero, "outputs": outs }));
    }

    out.sort_by_key(|x| x.sev);
    let errors = out.iter().filter(|x| x.sev == Sev::Error).count();
    let warns = out.iter().filter(|x| x.sev == Sev::Warn).count();
    json!({
        "resumen": format!("{errors} error(es), {warns} aviso(s)"),
        "hallazgos": out.iter().map(|x| json!({ "gravedad": x.sev.name(), "donde": x.scope, "problema": x.problem, "arreglo": x.fix })).collect::<Vec<_>>(),
        "universos": per_universe,
        "nodos_artnet": st.nodes.iter().map(|n| json!({ "ip": n.ip, "nombre": n.name, "puertos_salida": n.universes, "visto_hace_ms": n.last_seen_ms })).collect::<Vec<_>>(),
        "ips_de_este_equipo": st.local_ips,
        "puertos_serie": serial,
        "entradas": { "osc": { "activo": st.rt.inputs.osc_listening, "puerto": st.rt.inputs.osc_port, "error": st.rt.inputs.osc_error }, "artnet": { "activo": st.rt.inputs.artnet_listening, "error": st.rt.inputs.artnet_error } },
        "nota": "Los avisos son hipótesis: confirma con output_test (envía un nivel fijo por la salida) y comprueba si el aparato/nodo reacciona.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Runtime, Show};

    fn blank_rt() -> Runtime {
        serde_json::from_value(json!({
            "grand_master": 1.0, "blackout": false, "programmer": {}, "executors": [], "cuelists": [], "effects_running": [], "outputs": [],
            "inputs": { "osc_listening": false, "osc_port": 0, "osc_error": "", "artnet_listening": false, "artnet_error": "", "recent": [] },
            "timecode": { "running": false, "text": "", "frames": 0 }, "learn": null, "fps": 44.0
        }))
        .expect("runtime json")
    }
    fn fixture(id: u32, universe: u16) -> Fixture {
        serde_json::from_value(json!({ "id": id, "name": "F", "profile": "p", "mode": "m", "universe": universe, "address": 1 })).expect("fixture json")
    }

    fn state(show: Show) -> AiState {
        AiState { show, rt: blank_rt(), profiles: vec![], frames: vec![], merged: Default::default(), nodes: vec![], local_ips: vec!["192.168.1.10".into()] }
    }
    fn art(host: &str, port: u16, net: u8, sub: u8, uni: u8) -> OutputCfg {
        OutputCfg { enabled: true, kind: OutputKind::ArtNet { host: host.into(), port, net, subnet: sub, universe: uni } }
    }
    fn show_with(outputs: Vec<OutputCfg>) -> Show {
        let mut s = Show::default();
        s.universes = vec![UniverseCfg { id: 1, name: "U1".into(), outputs }];
        s
    }
    fn msgs(v: &Value) -> String {
        v["hallazgos"].as_array().unwrap().iter().map(|h| format!("{} {}", h["problema"], h["arreglo"])).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn flags_other_subnet_and_wrong_port() {
        let st = state(show_with(vec![art("10.9.9.9", 6455, 0, 0, 1)]));
        let r = analyze(&st, &[], None);
        let m = msgs(&r);
        assert!(m.contains("misma subred"), "{m}");
        assert!(m.contains("6454"), "{m}");
        assert!(r["resumen"].as_str().unwrap().contains("aviso"));
    }

    #[test]
    fn node_universe_mismatch_is_an_error() {
        let mut st = state(show_with(vec![art("192.168.1.50", 6454, 0, 0, 1)]));
        st.nodes = vec![ArtNode { ip: "192.168.1.50".into(), name: "Nodo".into(), long_name: String::new(), universes: vec![0, 2], last_seen_ms: 100 }];
        let r = analyze(&st, &[], None);
        assert!(r["hallazgos"][0]["gravedad"] == "error", "{r}");
        assert!(msgs(&r).contains("sus puertos de salida están en [0, 2]"));
    }

    #[test]
    fn matching_node_is_ok_and_silent_node_warns() {
        let mut st = state(show_with(vec![art("192.168.1.50", 6454, 0, 0, 1)]));
        st.nodes = vec![ArtNode { ip: "192.168.1.50".into(), name: "Nodo".into(), long_name: String::new(), universes: vec![1], last_seen_ms: 5 }];
        let ok = analyze(&st, &[], None);
        assert!(msgs(&ok).contains("OK: el nodo"));
        assert!(!ok["hallazgos"].as_array().unwrap().iter().any(|h| h["gravedad"] == "error"));
        st.nodes.clear();
        assert!(msgs(&analyze(&st, &[], None)).contains("no ha respondido a ArtPoll"));
    }

    #[test]
    fn reports_output_error_disabled_and_missing_universe() {
        let mut st = state(show_with(vec![art("", 6454, 0, 0, 0)]));
        st.rt.outputs = vec![OutputStatus { universe: 1, index: 0, state: OutState::Error, message: "Permission denied".into(), frames: 0 }];
        let m = msgs(&analyze(&st, &[], None));
        assert!(m.contains("Permission denied"));
        let mut st2 = state(show_with(vec![OutputCfg { enabled: false, kind: OutputKind::Serial { driver: SerialDriver::EnttecOpen, port: "COM3".into() } }]));
        st2.show.fixtures.push(fixture(1, 7));
        let m2 = msgs(&analyze(&st2, &[], None));
        assert!(m2.contains("desactivadas"), "{m2}");
        assert!(m2.contains("universo 7, que no existe"), "{m2}");
        let none = analyze(&st2, &[], Some(9));
        assert!(msgs(&none).contains("no existe en el show"));
    }

    #[test]
    fn serial_port_missing_and_sacn_range() {
        let st = state(show_with(vec![
            OutputCfg { enabled: true, kind: OutputKind::Serial { driver: SerialDriver::EnttecPro, port: "/dev/ttyUSB9".into() } },
            OutputCfg { enabled: true, kind: OutputKind::Sacn { universe: 0, priority: 250, unicast: None } },
        ]));
        let m = msgs(&analyze(&st, &["/dev/ttyUSB0".into()], None));
        assert!(m.contains("/dev/ttyUSB9") && m.contains("/dev/ttyUSB0"));
        assert!(m.contains("fuera de rango") && m.contains("Prioridad sACN"));
    }

    #[test]
    fn duplicate_destination_and_no_network() {
        let mut show = show_with(vec![art("192.168.1.50", 6454, 0, 0, 1)]);
        show.universes.push(UniverseCfg { id: 2, name: "U2".into(), outputs: vec![art("192.168.1.50", 6454, 0, 0, 1)] });
        let mut st = state(show);
        st.local_ips.clear();
        let m = msgs(&analyze(&st, &[], None));
        assert!(m.contains("se pisan"));
        assert!(m.contains("ninguna dirección IPv4"));
    }

    #[test]
    fn same_subnet_rules() {
        assert!(same_subnet("192.168.1.5".parse().unwrap(), "192.168.1.200".parse().unwrap()));
        assert!(!same_subnet("192.168.1.5".parse().unwrap(), "192.168.2.5".parse().unwrap()));
        assert!(same_subnet("10.0.0.2".parse().unwrap(), "10.7.7.7".parse().unwrap()));
    }
}
