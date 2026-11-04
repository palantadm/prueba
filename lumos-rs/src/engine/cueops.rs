//! Cue bookkeeping: copy, move and renumber.

use super::playback::{index_of, same_number, sort_cues};
use super::*;

/// Full look of cue `number` with tracking resolved (what the lights do when it is on).
fn resolved_state(cl: &Cuelist, number: f64) -> Option<PerFixture> {
    let mut state = PerFixture::new();
    for c in &cl.cues {
        if c.block || !cl.tracking {
            state = c.changes.clone();
        } else {
            for (f, attrs) in &c.changes {
                state.entry(*f).or_default().extend(attrs.iter().map(|(k, v)| (k.clone(), *v)));
            }
        }
        if same_number(c.number, number) {
            return Some(state);
        }
    }
    None
}

fn check_number(n: f64) -> Result<(), String> {
    if n.is_finite() && n >= 0.0 {
        Ok(())
    } else {
        Err("Número de cue no válido (0 o mayor)".into())
    }
}

impl Engine {
    pub(super) fn cue_ops(&mut self, c: Command) -> Result<(), String> {
        match c {
            Command::CueCopy { cuelist, number, to_cuelist, to_number } => {
                check_number(to_number)?;
                let dest = to_cuelist.unwrap_or(cuelist);
                let src = self.show.cuelists.iter().find(|l| l.id == cuelist).ok_or("La cuelist de origen no existe")?;
                let i = index_of(src, number).ok_or_else(|| format!("El cue {number} no existe"))?;
                let mut cue = src.cues[i].clone();
                if src.tracking {
                    // a copy must look the same wherever it lands: store the whole look and stop tracking from earlier cues
                    cue.changes = resolved_state(src, number).unwrap_or_default();
                    cue.refs = Default::default();
                    cue.block = true;
                }
                cue.number = to_number;
                let dst = self.show.cuelists.iter_mut().find(|l| l.id == dest).ok_or("La cuelist de destino no existe")?;
                if index_of(dst, to_number).is_some() {
                    return Err(format!("Ya existe el cue {to_number} en la cuelist de destino"));
                }
                dst.cues.push(cue);
                sort_cues(dst);
                self.after_cuelist_edit(dest);
            }
            Command::CueMove { cuelist, number, to_number } => {
                check_number(to_number)?;
                let cl = self.show.cuelists.iter_mut().find(|l| l.id == cuelist).ok_or("La cuelist no existe")?;
                let i = index_of(cl, number).ok_or_else(|| format!("El cue {number} no existe"))?;
                if same_number(number, to_number) {
                    return Ok(());
                }
                if index_of(cl, to_number).is_some() {
                    return Err(format!("Ya existe el cue {to_number}"));
                }
                cl.cues[i].number = to_number;
                sort_cues(cl);
                self.after_cuelist_edit(cuelist);
            }
            Command::CueRenumber { cuelist, start, step } => {
                check_number(start)?;
                if !step.is_finite() || step <= 0.0 {
                    return Err("El paso debe ser mayor que 0".into());
                }
                let cl = self.show.cuelists.iter_mut().find(|l| l.id == cuelist).ok_or("La cuelist no existe")?;
                sort_cues(cl);
                for (k, c) in cl.cues.iter_mut().enumerate() {
                    c.number = ((start + step * k as f64) * 1000.0).round() / 1000.0;
                }
                self.after_cuelist_edit(cuelist);
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    fn cue(n: f64, f: u32, attr: &str, v: f32) -> Cue {
        let mut a = Attrs::new();
        a.insert(attr.into(), v);
        let mut changes = PerFixture::new();
        changes.insert(f, a);
        Cue { number: n, label: format!("c{n}"), fade_in: 1.0, fade_out: 1.0, delay: 0.0, follow: None, timecode: None, changes, block: false, refs: Default::default(), timing: Default::default(), curve: Default::default(), stagger: 0.0, notes: String::new() }
    }

    fn rig() -> Engine {
        let mut e = engine();
        add(&mut e, "generic/rgb-par", "3ch", 1);
        e.show.cuelists.push(Cuelist { id: 1, name: "A".into(), cues: vec![cue(1.0, 1, "red", 1.0), cue(2.0, 1, "blue", 0.5), cue(3.0, 1, "green", 0.25)], looped: false, tracking: true, priority: 0, auto_mark: false, rate: 1.0, release_others: false, release_at_end: false });
        e.show.cuelists.push(Cuelist { id: 2, name: "B".into(), cues: vec![], looped: false, tracking: true, priority: 0, auto_mark: false, rate: 1.0, release_others: false, release_at_end: false });
        e
    }
    fn nums(e: &Engine, l: u32) -> Vec<f64> {
        e.show.cuelists.iter().find(|c| c.id == l).unwrap().cues.iter().map(|c| c.number).collect()
    }

    #[test]
    fn copy_in_a_tracking_list_keeps_the_full_look_wherever_it_lands() {
        let mut e = rig();
        e.handle(Command::CueCopy { cuelist: 1, number: 3.0, to_cuelist: Some(2), to_number: 10.0 }).unwrap();
        let c = &e.show.cuelists[1].cues[0];
        assert_eq!(c.number, 10.0);
        assert!(c.block, "stops tracking from earlier cues");
        let a = &c.changes[&1];
        assert!(a["red"] == 1.0 && a["blue"] == 0.5 && a["green"] == 0.25, "red and blue tracked from cues 1 and 2: {a:?}");
        // same list: sorted into place, the source is untouched
        e.handle(Command::CueCopy { cuelist: 1, number: 1.0, to_cuelist: None, to_number: 2.5 }).unwrap();
        assert_eq!(nums(&e, 1), vec![1.0, 2.0, 2.5, 3.0]);
        assert!(!e.show.cuelists[0].cues[0].block);
        assert!(e.handle(Command::CueCopy { cuelist: 1, number: 1.0, to_cuelist: None, to_number: 2.0 }).unwrap_err().contains("Ya existe"));
        assert!(e.handle(Command::CueCopy { cuelist: 1, number: 9.0, to_cuelist: None, to_number: 4.0 }).is_err());
        assert!(e.handle(Command::CueCopy { cuelist: 1, number: 1.0, to_cuelist: Some(9), to_number: 4.0 }).is_err());
        assert!(e.handle(Command::CueCopy { cuelist: 1, number: 1.0, to_cuelist: None, to_number: -1.0 }).is_err());
    }

    #[test]
    fn move_and_renumber_keep_order_and_refuse_clashes() {
        let mut e = rig();
        e.handle(Command::CueMove { cuelist: 1, number: 1.0, to_number: 2.5 }).unwrap();
        assert_eq!(nums(&e, 1), vec![2.0, 2.5, 3.0], "cue 1 now sits between 2 and 3");
        assert!(e.handle(Command::CueMove { cuelist: 1, number: 2.0, to_number: 3.0 }).unwrap_err().contains("Ya existe"));
        e.handle(Command::CueMove { cuelist: 1, number: 2.0, to_number: 2.0 }).unwrap();
        e.handle(Command::CueRenumber { cuelist: 1, start: 10.0, step: 10.0 }).unwrap();
        assert_eq!(nums(&e, 1), vec![10.0, 20.0, 30.0]);
        assert_eq!(e.show.cuelists[0].cues[1].label, "c1", "the cue that was 2.5 (formerly 1) is second");
        e.handle(Command::CueRenumber { cuelist: 1, start: 1.0, step: 0.5 }).unwrap();
        assert_eq!(nums(&e, 1), vec![1.0, 1.5, 2.0]);
        assert!(e.handle(Command::CueRenumber { cuelist: 1, start: 1.0, step: 0.0 }).is_err());
        assert!(e.handle(Command::CueRenumber { cuelist: 9, start: 1.0, step: 1.0 }).is_err());
    }

    #[test]
    fn cuelist_rate_scales_every_time_of_the_cue() {
        let mut e = rig();
        let mut c = cue(1.0, 1, "red", 1.0);
        c.fade_in = 2.0;
        c.fade_out = 4.0;
        c.delay = 1.0;
        c.stagger = 0.5;
        c.follow = Some(8.0);
        c.timing.insert("color".into(), AttrTiming { delay: 2.0, fade: 6.0 });
        e.show.cuelists[0].cues = vec![c];
        e.show.cuelists[0].rate = 2.0;
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        let r = &e.runs[&1];
        assert_eq!((r.delay, r.fade_in, r.fade_out, r.stagger), (0.5, 1.0, 2.0, 0.25), "twice as fast");
        assert_eq!((r.timing["color"].delay, r.timing["color"].fade), (1.0, 3.0));
        e.show.cuelists[0].rate = 0.5;
        e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
        assert_eq!(e.runs[&1].fade_in, 4.0, "half speed");
        e.show.cuelists[0].rate = f32::NAN;
        e.handle(Command::GoTo { cuelist: 1, number: 1.0 }).unwrap();
        assert_eq!(e.runs[&1].fade_in, 2.0, "a broken rate means normal speed");
        // old shows have no rate: it loads as 1
        let cl: Cuelist = serde_json::from_str(r#"{"id":1,"name":"x","cues":[]}"#).unwrap();
        assert_eq!((cl.rate, cl.release_others, cl.release_at_end), (1.0, false, false));
    }

    #[test]
    fn starting_a_list_can_release_the_others_and_go_past_the_end_can_release_itself() {
        let mut e = rig();
        e.show.cuelists[1].cues = vec![cue(1.0, 1, "blue", 1.0)];
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        assert_eq!(e.runs[&1].state, PlayState::Running);
        e.handle(Command::Go { cuelist: 2 }).unwrap();
        assert!(e.runs[&1].state == PlayState::Running, "without the option both play");
        e.handle(Command::ReleaseCuelist { cuelist: 2 }).unwrap();
        e.show.cuelists[1].release_others = true;
        e.runs.get_mut(&2).map(|r| r.state = PlayState::Idle);
        e.handle(Command::Go { cuelist: 2 }).unwrap();
        let a = e.runs.get(&1);
        assert!(a.map_or(true, |r| r.releasing && r.state == PlayState::Idle), "list A was taken off");
        // release at end
        let mut e = rig();
        e.show.cuelists[0].release_at_end = true;
        for _ in 0..3 {
            e.handle(Command::Go { cuelist: 1 }).unwrap();
        }
        assert_eq!(e.runs[&1].current, Some(3.0));
        e.handle(Command::Go { cuelist: 1 }).unwrap();
        assert!(e.runs.get(&1).map_or(true, |r| r.releasing && r.state == PlayState::Idle), "the GO after the last cue released the list");
        let mut e = rig();
        for _ in 0..4 {
            e.handle(Command::Go { cuelist: 1 }).unwrap();
        }
        assert_eq!(e.runs[&1].current, Some(3.0), "by default it just stays on the last cue");
    }
}
