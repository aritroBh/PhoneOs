//! Weekly schedules as pure functions of the local weekday and minute of day.
//!
//! The shell supplies the phone's local weekday and time (it owns the clock and the
//! timezone database); the core only does arithmetic over the 7 × 1440 minutes of a week.
//! Daylight-saving jumps are the shell's concern: it asks again with the new local time.

use crate::workflow::{Schedule, Weekday};

/// Minutes in a day.
pub const MINUTES_PER_DAY: u32 = 1_440;
/// Minutes in a week.
pub const MINUTES_PER_WEEK: u32 = 7 * MINUTES_PER_DAY;

/// The scheduled minute of day (`hour * 60 + minute`), clamped to the day.
fn target_minute(schedule: &Schedule) -> u32 {
    (u32::from(schedule.hour.min(23)) * 60 + u32::from(schedule.minute.min(59)))
        .min(MINUTES_PER_DAY - 1)
}

/// Minutes from now (`weekday`, `minute_of_day` in local time) until the next scheduled
/// minute, `0` when now is a scheduled minute. Always below [`MINUTES_PER_WEEK`] for a
/// schedule with at least one day; `u32::MAX` (never) when `days` is empty.
/// `minute_of_day` above 1439 is treated as 1439.
pub fn minutes_until_next(schedule: &Schedule, weekday: Weekday, minute_of_day: u32) -> u32 {
    let now = minute_of_day.min(MINUTES_PER_DAY - 1);
    let target = target_minute(schedule);
    for k in 0..=7u32 {
        if !schedule.days.contains(&weekday.plus(k)) {
            continue;
        }
        let at = k * MINUTES_PER_DAY + target;
        if at >= now {
            return at - now;
        }
    }
    u32::MAX
}

/// Minutes since the most recent scheduled minute at or before now, `0` when now is a
/// scheduled minute. Below [`MINUTES_PER_WEEK`] for a schedule with at least one day;
/// `u32::MAX` when `days` is empty.
pub fn minutes_since_last(schedule: &Schedule, weekday: Weekday, minute_of_day: u32) -> u32 {
    let now = minute_of_day.min(MINUTES_PER_DAY - 1);
    let target = target_minute(schedule);
    for k in 0..=7u32 {
        // k days back.
        if !schedule.days.contains(&weekday.plus(7 - k % 7)) {
            continue;
        }
        let back = k * MINUTES_PER_DAY + now;
        if back >= target {
            return back - target;
        }
    }
    u32::MAX
}

/// Whether the schedule fired within the last `window_minutes` minutes, now included:
/// `minutes_since_last < window_minutes`. A shell that checks every minute passes 1; one
/// woken after a gap passes the minutes since its last check, so nothing is missed and
/// nothing runs twice.
pub fn is_due(
    schedule: &Schedule,
    weekday: Weekday,
    minute_of_day: u32,
    window_minutes: u32,
) -> bool {
    minutes_since_last(schedule, weekday, minute_of_day) < window_minutes
}
