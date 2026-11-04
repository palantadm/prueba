//! Calendar / astronomical clock inside the engine: fires `ScheduleRule`s once a second at the right local minute.

use super::*;
use crate::schedule;
use chrono::{Datelike, Local, NaiveDate, Timelike};

#[derive(Default)]
pub struct Sched {
    /// rule id → (date, minute) it last ran at, so one minute never fires twice.
    fired: HashMap<u32, (NaiveDate, i32)>,
    checked: Option<Instant>,
    cache: parking_lot::Mutex<(i64, ScheduleRt)>,
}

/// `(local date, minute of day, UTC offset in minutes)` right now.
fn clock() -> (NaiveDate, i32, i32) {
    let n = Local::now();
    (n.date_naive(), (n.hour() * 60 + n.minute()) as i32, n.offset().local_minus_utc() / 60)
}

impl Engine {
    fn sched_loc(&self) -> Option<(f64, f64)> {
        self.show.settings.location.map(|p| (p.lat, p.lon))
    }

    pub(super) fn schedule_save(&mut self, mut r: ScheduleRule) -> Result<(), String> {
        r.name = r.name.chars().take(60).collect();
        r.days &= 0x7F;
        if r.days == 0 {
            return Err("Elige al menos un día de la semana".into());
        }
        if r.action.trim().is_empty() {
            return Err("La regla necesita una línea de consola que ejecutar (p. ej. «scene 1 on»)".into());
        }
        if let Err(e) = crate::engine::cmdline_check(&r.action) {
            return Err(format!("La acción no es una línea de consola válida: {e}"));
        }
        if let ScheduleWhen::Time { hour, minute } = r.when {
            if hour > 23 || minute > 59 {
                return Err("Hora no válida".into());
            }
        }
        if let ScheduleWhen::Sunrise { offset_min } | ScheduleWhen::Sunset { offset_min } = r.when {
            if !(-360..=360).contains(&offset_min) {
                return Err("El desfase respecto al sol debe estar entre −360 y +360 minutos".into());
            }
            if self.show.settings.location.is_none() {
                return Err("Para amanecer/atardecer hace falta la ubicación (Ajustes → Programación)".into());
            }
        }
        if r.id == 0 {
            r.id = next_id(self.show.schedule.iter().map(|x| x.id));
        }
        match self.show.schedule.iter_mut().find(|x| x.id == r.id) {
            Some(x) => *x = r,
            None => self.show.schedule.push(r),
        }
        *self.sched.cache.lock() = (-1, ScheduleRt::default());
        self.touch();
        Ok(())
    }

    pub(super) fn schedule_remove(&mut self, id: u32) {
        self.show.schedule.retain(|r| r.id != id);
        self.sched.fired.remove(&id);
        *self.sched.cache.lock() = (-1, ScheduleRt::default());
        self.touch();
    }

    /// Called every tick; checks the clock about once a second.
    pub(super) fn schedule_tick(&mut self) {
        if self.show.schedule.is_empty() || self.sched.checked.map_or(false, |t| t.elapsed() < Duration::from_millis(900)) {
            return;
        }
        self.sched.checked = Some(Instant::now());
        let (date, minute, off) = clock();
        self.schedule_run_at(date, minute, off);
    }

    pub(super) fn schedule_run_at(&mut self, date: NaiveDate, minute: i32, tz_off: i32) {
        let loc = self.sched_loc();
        let rules = self.show.schedule.clone();
        for r in rules.into_iter().filter(|r| schedule::due(r, date, minute, loc, tz_off)) {
            if self.sched.fired.get(&r.id) == Some(&(date, minute)) {
                continue;
            }
            self.sched.fired.insert(r.id, (date, minute));
            let line = format!("programación «{}»: {}", r.name, r.action);
            match self.handle(Command::CommandLine { text: r.action.clone() }) {
                Ok(()) => self.auto.log.push(&line, true, "ejecutada"),
                Err(e) => self.auto.log.push(&line, false, e),
            }
        }
    }

    pub(super) fn schedule_rt(&self) -> ScheduleRt {
        let (date, minute, off) = clock();
        let key = date.num_days_from_ce() as i64 * 1440 + minute as i64;
        {
            let c = self.sched.cache.lock();
            if c.0 == key {
                return c.1.clone();
            }
        }
        let loc = self.sched_loc();
        let sun = |pick: usize| {
            let probe = ScheduleRule { id: 0, name: String::new(), enabled: true, when: if pick == 0 { ScheduleWhen::Sunrise { offset_min: 0 } } else { ScheduleWhen::Sunset { offset_min: 0 } }, days: 0x7F, action: String::new() };
            schedule::minute_of(&probe, date, loc, off).map(schedule::hhmm)
        };
        let mut next: Vec<ScheduleNext> = self.show.schedule.iter().filter_map(|r| schedule::next_fire(r, date, minute, loc, off).map(|(d, m)| ScheduleNext { rule: r.id, at: schedule::hhmm(m), days_ahead: d })).collect();
        next.sort_by_key(|n| (n.days_ahead, n.at.clone()));
        let rt = ScheduleRt { next, sunrise: sun(0), sunset: sun(1), now: schedule::hhmm(minute) };
        *self.sched.cache.lock() = (key, rt.clone());
        rt
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests_support::*;
    use super::*;

    fn rule(when: ScheduleWhen, action: &str) -> ScheduleRule {
        ScheduleRule { id: 0, name: "Entrada".into(), enabled: true, when, days: 0x7F, action: action.into() }
    }
    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    #[test]
    fn rules_run_once_at_their_minute_and_are_logged() {
        let mut e = engine();
        e.handle(Command::ScheduleSave { rule: rule(ScheduleWhen::Time { hour: 19, minute: 30 }, "blackout on") }).unwrap();
        assert_eq!(e.show.schedule[0].id, 1);
        e.schedule_run_at(day(2), 19 * 60 + 29, 60);
        assert!(!e.blackout);
        e.schedule_run_at(day(2), 19 * 60 + 30, 60);
        assert!(e.blackout);
        e.blackout = false;
        e.schedule_run_at(day(2), 19 * 60 + 30, 60); // same minute again (next tick): no second run
        assert!(!e.blackout, "one run per minute");
        e.schedule_run_at(day(3), 19 * 60 + 30, 60);
        assert!(e.blackout, "next day runs again");
        assert!(e.auto.log.0.iter().any(|l| l.text.contains("programación «Entrada»") && l.ok));
    }

    #[test]
    fn a_failing_action_is_logged_not_fatal() {
        let mut e = engine();
        e.handle(Command::ScheduleSave { rule: rule(ScheduleWhen::Time { hour: 8, minute: 0 }, "scene 99 on") }).unwrap();
        e.schedule_run_at(day(2), 8 * 60, 60);
        assert!(e.auto.log.0.iter().any(|l| !l.ok && l.text.contains("programación")), "{:?}", e.auto.log.0.iter().map(|l| (&l.text, l.ok)).collect::<Vec<_>>());
    }

    #[test]
    fn validation_and_sun_rules_need_a_location() {
        let mut e = engine();
        let sunset = rule(ScheduleWhen::Sunset { offset_min: -10 }, "scene 1 on");
        assert!(e.handle(Command::ScheduleSave { rule: sunset.clone() }).unwrap_err().contains("ubicación"));
        e.show.settings.location = Some(GeoPos { lat: 43.37, lon: -8.40 });
        e.handle(Command::ScheduleSave { rule: sunset }).unwrap();
        let rt = e.schedule_rt();
        assert!(rt.sunrise.is_some() && rt.sunset.is_some(), "A Coruña has a sunrise and sunset every day");
        assert_eq!(rt.next.len(), 1);
        let mut bad = rule(ScheduleWhen::Time { hour: 8, minute: 0 }, "esto no es consola ###");
        assert!(e.handle(Command::ScheduleSave { rule: bad.clone() }).unwrap_err().contains("línea de consola"));
        bad.action = " ".into();
        assert!(e.handle(Command::ScheduleSave { rule: bad.clone() }).unwrap_err().contains("necesita"));
        bad.action = "go".into();
        bad.days = 0;
        assert!(e.handle(Command::ScheduleSave { rule: bad.clone() }).unwrap_err().contains("día"));
        bad.days = 0x7F;
        bad.when = ScheduleWhen::Sunrise { offset_min: 9000 };
        assert!(e.handle(Command::ScheduleSave { rule: bad }).unwrap_err().contains("360"));
        e.handle(Command::ScheduleRemove { id: 1 }).unwrap();
        assert!(e.show.schedule.is_empty());
    }
}
