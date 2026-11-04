//! Calendar and astronomical clock: run a console line at a time of day, at sunrise or at sunset, on chosen weekdays.
//! Pure functions over minutes and dates so they are easy to test; the engine asks once a second.

use crate::model::{ScheduleRule, ScheduleWhen};
use chrono::{Datelike, NaiveDate};

fn julian_day(d: NaiveDate) -> f64 {
    // days since 1970-01-01 → JD of 0h UTC
    d.num_days_from_ce() as f64 - 719_163.0 + 2_440_587.5
}

/// `(sunrise, sunset)` in minutes after 0h UTC of that date (`None` = the sun does not rise / set that day).
/// NOAA solar calculator; accurate to about a minute at normal latitudes. `lon` east positive.
pub fn sun_times(lat: f64, lon: f64, date: NaiveDate) -> (Option<f64>, Option<f64>) {
    let jd = julian_day(date) + 0.5; // noon UTC
    let t = (jd - 2_451_545.0) / 36_525.0;
    let rad = std::f64::consts::PI / 180.0;
    let l0 = (280.46646 + t * (36_000.76983 + t * 0.0003032)).rem_euclid(360.0);
    let m = 357.52911 + t * (35_999.05029 - 0.0001537 * t);
    let e = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
    let c = (m * rad).sin() * (1.914602 - t * (0.004817 + 0.000014 * t)) + (2.0 * m * rad).sin() * (0.019993 - 0.000101 * t) + (3.0 * m * rad).sin() * 0.000289;
    let true_long = l0 + c;
    let omega = 125.04 - 1934.136 * t;
    let lambda = true_long - 0.00569 - 0.00478 * (omega * rad).sin();
    let eps0 = 23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60.0) / 60.0;
    let eps = eps0 + 0.00256 * (omega * rad).cos();
    let decl = ((eps * rad).sin() * (lambda * rad).sin()).asin();
    let y = (eps * rad / 2.0).tan().powi(2);
    let eqtime = 4.0
        * (y * (2.0 * l0 * rad).sin() - 2.0 * e * (m * rad).sin() + 4.0 * e * y * (m * rad).sin() * (2.0 * l0 * rad).cos() - 0.5 * y * y * (4.0 * l0 * rad).sin() - 1.25 * e * e * (2.0 * m * rad).sin())
        / rad;
    let cos_ha = (90.833 * rad).cos() / ((lat * rad).cos() * decl.cos()) - (lat * rad).tan() * decl.tan();
    if !(-1.0..=1.0).contains(&cos_ha) {
        return (None, None); // polar day / night
    }
    let ha = cos_ha.acos() / rad;
    (Some(720.0 - 4.0 * (lon + ha) - eqtime), Some(720.0 - 4.0 * (lon - ha) - eqtime))
}

/// Minute of the local day (0..1440) at which `rule` fires on `date`, or `None` (polar day, or no location for a sun rule).
pub fn minute_of(rule: &ScheduleRule, date: NaiveDate, loc: Option<(f64, f64)>, tz_offset_min: i32) -> Option<i32> {
    let local = |utc: f64, off: i32| ((utc + off as f64 + off_day(date)).round() as i32).rem_euclid(1440);
    match rule.when {
        ScheduleWhen::Time { hour, minute } => Some((hour.min(23) as i32) * 60 + minute.min(59) as i32),
        ScheduleWhen::Sunrise { offset_min } => {
            let (lat, lon) = loc?;
            Some(local(sun_times(lat, lon, date).0? + offset_min as f64, tz_offset_min))
        }
        ScheduleWhen::Sunset { offset_min } => {
            let (lat, lon) = loc?;
            Some(local(sun_times(lat, lon, date).1? + offset_min as f64, tz_offset_min))
        }
    }
}

fn off_day(_d: NaiveDate) -> f64 {
    0.0
}

/// Bit of `days` for a weekday (Monday = bit 0 … Sunday = bit 6).
pub fn day_bit(date: NaiveDate) -> u8 {
    1 << date.weekday().num_days_from_monday()
}

/// Does `rule` fire at this local `minute` of `date`?
pub fn due(rule: &ScheduleRule, date: NaiveDate, minute: i32, loc: Option<(f64, f64)>, tz_offset_min: i32) -> bool {
    rule.enabled && rule.days & day_bit(date) != 0 && minute_of(rule, date, loc, tz_offset_min) == Some(minute)
}

/// The next firing of `rule` after `(date, minute)`: `(days ahead, minute of that day)`, looking up to 8 days.
pub fn next_fire(rule: &ScheduleRule, date: NaiveDate, minute: i32, loc: Option<(f64, f64)>, tz_offset_min: i32) -> Option<(u32, i32)> {
    if !rule.enabled {
        return None;
    }
    for ahead in 0..8u32 {
        let d = date + chrono::Duration::days(ahead as i64);
        if rule.days & day_bit(d) == 0 {
            continue;
        }
        if let Some(m) = minute_of(rule, d, loc, tz_offset_min) {
            if ahead > 0 || m > minute {
                return Some((ahead, m));
            }
        }
    }
    None
}

pub fn hhmm(minute: i32) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(when: ScheduleWhen, days: u8) -> ScheduleRule {
        ScheduleRule { id: 1, name: "r".into(), enabled: true, when, days, action: "scene 1 on".into() }
    }
    fn d(y: i32, m: u32, dd: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, dd).unwrap()
    }

    #[test]
    fn sun_times_match_known_values() {
        // Madrid, summer solstice: solar noon ≈ 12:17 UTC (lon −3.7°, equation of time −1.8 min), half-day 7 h 32 min → 04:45 / 19:48 UTC (06:45 / 21:48 CEST)
        let (r, s) = sun_times(40.4168, -3.7038, d(2026, 6, 21));
        assert!((r.unwrap() - (4.0 * 60.0 + 45.0)).abs() < 5.0, "{r:?}");
        assert!((s.unwrap() - (19.0 * 60.0 + 48.0)).abs() < 5.0, "{s:?}");
        // winter: noon ≈ 12:13 UTC, half-day 4 h 38 min → 07:34 / 16:51 UTC (08:34 / 17:51 CET)
        let (r, s) = sun_times(40.4168, -3.7038, d(2026, 12, 21));
        assert!((r.unwrap() - (7.0 * 60.0 + 34.0)).abs() < 5.0, "{r:?}");
        assert!((s.unwrap() - (16.0 * 60.0 + 51.0)).abs() < 5.0, "{s:?}");
        // equator at the equinox: about 06:00 and 18:00 solar time (a few minutes off because of refraction / equation of time)
        let (r, s) = sun_times(0.0, 0.0, d(2026, 3, 20));
        assert!((r.unwrap() - 6.0 * 60.0).abs() < 12.0 && (s.unwrap() - 18.0 * 60.0).abs() < 12.0, "{r:?} {s:?}");
        // A Coruña (Galicia) is ~1 h later in clock time than Barcelona's sun
        let (rc, _) = sun_times(43.37, -8.40, d(2026, 6, 21));
        let (rb, _) = sun_times(41.39, 2.17, d(2026, 6, 21));
        assert!(rc.unwrap() - rb.unwrap() > 35.0);
        // polar day / night
        assert_eq!(sun_times(80.0, 0.0, d(2026, 6, 21)), (None, None));
        assert_eq!(sun_times(80.0, 0.0, d(2026, 12, 21)), (None, None));
    }

    #[test]
    fn rules_fire_on_their_minute_and_weekdays_only() {
        let r = rule(ScheduleWhen::Time { hour: 19, minute: 30 }, 0b001_0101); // Mon, Wed, Fri
        let mon = d(2026, 11, 2);
        assert_eq!(mon.weekday().num_days_from_monday(), 0);
        assert!(due(&r, mon, 19 * 60 + 30, None, 60));
        assert!(!due(&r, mon, 19 * 60 + 31, None, 60));
        assert!(!due(&r, d(2026, 11, 3), 19 * 60 + 30, None, 60), "Tuesday is not selected");
        let mut off = r.clone();
        off.enabled = false;
        assert!(!due(&off, mon, 19 * 60 + 30, None, 60));
        // a sun rule needs a location
        let sun = rule(ScheduleWhen::Sunset { offset_min: -15 }, 0x7F);
        assert!(minute_of(&sun, mon, None, 60).is_none());
        // Madrid winter, UTC+1: sunset ≈ 17:51 → with −15 min ≈ 17:36 local
        let m = minute_of(&sun, d(2026, 12, 21), Some((40.4168, -3.7038)), 60).unwrap();
        assert!((m - (17 * 60 + 36)).abs() < 5, "{}", hhmm(m));
    }

    #[test]
    fn next_fire_looks_ahead_over_days_and_skips_unselected_ones() {
        let r = rule(ScheduleWhen::Time { hour: 8, minute: 0 }, 0b000_0001); // Mondays only
        let tue = d(2026, 11, 3);
        assert_eq!(next_fire(&r, tue, 9 * 60, None, 60), Some((6, 8 * 60)), "next Monday");
        let mon = d(2026, 11, 2);
        assert_eq!(next_fire(&r, mon, 7 * 60, None, 60), Some((0, 8 * 60)), "later today");
        assert_eq!(next_fire(&r, mon, 8 * 60, None, 60), Some((7, 8 * 60)), "already passed: next week");
        let never = rule(ScheduleWhen::Time { hour: 8, minute: 0 }, 0);
        assert_eq!(next_fire(&never, mon, 0, None, 60), None);
        assert_eq!(hhmm(8 * 60 + 5), "08:05");
    }
}
