//! Weekly schedules as pure functions of the local weekday and minute of day.
//!
//! The shell supplies the phone's local weekday and time (it owns the clock and the
//! timezone database); the core only does arithmetic over the 7 × 1440 minutes of a week.
//! Daylight-saving jumps are the shell's concern: it asks again with the new local time.
//!
//! Out-of-range input is a [`SkillError::Schedule`] error, never clamped: a schedule with
//! no days, an hour above 23 or a minute above 59, a `minute_of_day` above 1439, a weekday
//! index above 6 ([`Weekday::from_index`]).

use crate::error::SkillError;
use crate::workflow::{Schedule, Weekday};

/// Minutes in a day.
pub const MINUTES_PER_DAY: u32 = 1_440;
/// Minutes in a week.
pub const MINUTES_PER_WEEK: u32 = 7 * MINUTES_PER_DAY;

impl Weekday {
    /// The weekday for `0` (Monday) to `6` (Sunday).
    pub fn from_index(index: u32) -> Result<Weekday, SkillError> {
        usize::try_from(index)
            .ok()
            .and_then(|i| Weekday::ALL.get(i).copied())
            .ok_or_else(|| SkillError::Schedule(format!("weekday {index} is not 0 to 6")))
    }
}

/// Checks the schedule and the local time; returns `(target minute, now)`.
fn checked(schedule: &Schedule, minute_of_day: u32) -> Result<(u32, u32), SkillError> {
    if schedule.days.is_empty() {
        return Err(SkillError::Schedule("no days".to_owned()));
    }
    if schedule.hour > 23 || schedule.minute > 59 {
        return Err(SkillError::Schedule(format!(
            "{}:{:02} is not a time of day",
            schedule.hour, schedule.minute
        )));
    }
    if minute_of_day >= MINUTES_PER_DAY {
        return Err(SkillError::Schedule(format!(
            "minute of day {minute_of_day} is not 0 to 1439"
        )));
    }
    Ok((
        u32::from(schedule.hour) * 60 + u32::from(schedule.minute),
        minute_of_day,
    ))
}

/// Minutes from now (`weekday`, `minute_of_day` in local time) until the next scheduled
/// minute, `0` when now is a scheduled minute; always below [`MINUTES_PER_WEEK`].
pub fn minutes_until_next(
    schedule: &Schedule,
    weekday: Weekday,
    minute_of_day: u32,
) -> Result<u32, SkillError> {
    let (target, now) = checked(schedule, minute_of_day)?;
    for k in 0..=7u32 {
        if !schedule.days.contains(&weekday.plus(k)) {
            continue;
        }
        let at = k * MINUTES_PER_DAY + target;
        if at >= now {
            return Ok(at - now);
        }
    }
    Err(SkillError::Schedule("no day matched".to_owned()))
}

/// Minutes since the most recent scheduled minute at or before now, `0` when now is a
/// scheduled minute; always below [`MINUTES_PER_WEEK`].
pub fn minutes_since_last(
    schedule: &Schedule,
    weekday: Weekday,
    minute_of_day: u32,
) -> Result<u32, SkillError> {
    let (target, now) = checked(schedule, minute_of_day)?;
    for k in 0..=7u32 {
        // k days back.
        if !schedule.days.contains(&weekday.plus(7 - k % 7)) {
            continue;
        }
        let back = k * MINUTES_PER_DAY + now;
        if back >= target {
            return Ok(back - target);
        }
    }
    Err(SkillError::Schedule("no day matched".to_owned()))
}

/// Whether the schedule fired within the last `window_minutes` minutes, now included:
/// `minutes_since_last < window_minutes`. A shell that checks every minute passes 1; one
/// woken after a gap passes the minutes since its last check, so nothing is missed and
/// nothing runs twice. A window of 0 is never due.
pub fn is_due(
    schedule: &Schedule,
    weekday: Weekday,
    minute_of_day: u32,
    window_minutes: u32,
) -> Result<bool, SkillError> {
    Ok(minutes_since_last(schedule, weekday, minute_of_day)? < window_minutes)
}
