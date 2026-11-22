//! What a session holds on to as it takes steps, read as the process's resident set.
//!
//! The workspace forbids `unsafe`, which a counting global allocator needs, so this reads
//! `VmRSS` from `/proc/self/status` (Linux only) around a batch of sessions held side by
//! side. It is coarse: it counts allocator slack and the memory freed transient buffers
//! leave behind, not only live bytes. Read the growth from step 1 to step 40 as the
//! retention of a session (the screens, the history, the recorded flow), and the step 1
//! figure as an upper bound on what one screen costs to hold.

use jarvis_agent::Session;
use jarvis_proto::{Effect, Event, RunnerResult, Screen, TimedEvent};

use super::gen::set_variant;
use super::stages::{new_session, observed, tap_reply, tap_targets};

/// The process's resident set in kilobytes, where the platform tells.
pub fn rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// One batch of sessions and what they held after some steps.
#[derive(Debug, Clone)]
pub struct MemoryPoint {
    /// Steps each session had taken.
    pub steps: u32,
    /// Resident growth since the sessions were created, per session, in kilobytes.
    pub kb_per_session: f64,
}

/// A session being driven against a scripted phone: it observes, the model taps another
/// control each step, the tap works and the title changes.
pub struct Driver {
    session: Session,
    effect: Effect,
    clock: u64,
    variant: u32,
    next_target: usize,
    targets: Vec<u32>,
}

impl Driver {
    /// Starts a session; `targets` are the element indices to tap, in rotation.
    pub fn start(context_tokens: u32, targets: Vec<u32>) -> Driver {
        let mut session = new_session(context_tokens, false);
        let effect = session.step(TimedEvent::new(0, Event::Start));
        Driver {
            session,
            effect,
            clock: 0,
            variant: 0,
            next_target: 0,
            targets,
        }
    }

    /// Steps taken so far.
    pub fn steps(&self) -> u32 {
        self.session.progress().step
    }

    /// Whether the session ended.
    pub fn finished(&self) -> bool {
        self.effect.is_finished()
    }

    fn feed(&mut self, event: Event) {
        self.clock += 1_000;
        self.effect = self.session.step(TimedEvent::new(self.clock, event));
    }

    /// Runs until `steps` model actions were taken or the session ended.
    pub fn run_to(&mut self, steps: u32, base: &Screen) {
        let mut guard = 0u32;
        while self.steps() < steps && !self.finished() && guard < steps.saturating_mul(8) + 16 {
            guard += 1;
            let event = match &self.effect {
                Effect::Observe { .. } => {
                    let mut s = base.clone();
                    set_variant(&mut s, self.variant);
                    observed(&s)
                }
                Effect::Infer(_) => {
                    let n = self.targets.len().max(1);
                    let at = self.next_target % n;
                    self.next_target += 1;
                    tap_reply(self.targets.get(at).copied().unwrap_or(1))
                }
                Effect::Act(_) => {
                    self.variant += 1;
                    Event::ActResult(RunnerResult::ok())
                }
                Effect::Confirm(_) => Event::UserDecision { approved: true },
                _ => return,
            };
            self.feed(event);
        }
    }
}

/// Resident growth of `count` sessions on `base` after each of `at_steps` (ascending), per
/// session. `None` where the platform gives no resident set.
pub fn growth(
    base: &Screen,
    targets: &[u32],
    context_tokens: u32,
    count: usize,
    at_steps: &[u32],
) -> Option<Vec<MemoryPoint>> {
    let before = rss_kb()?;
    let mut drivers: Vec<Driver> = (0..count)
        .map(|_| Driver::start(context_tokens, targets.to_vec()))
        .collect();
    let mut points = Vec::new();
    for &steps in at_steps {
        for d in &mut drivers {
            d.run_to(steps, base);
        }
        let after = rss_kb()?;
        points.push(MemoryPoint {
            steps,
            kb_per_session: after.saturating_sub(before) as f64 / count.max(1) as f64,
        });
    }
    drop(drivers);
    Some(points)
}

/// The tap targets of `base`'s compact screen.
pub fn targets_of(base: &Screen) -> Vec<u32> {
    tap_targets(&jarvis_observe::compact(base))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::gen::{generate, GenSpec, Shape, View, DEFAULT_SEED};

    #[test]
    fn a_driver_takes_the_requested_steps() {
        let base = generate(&GenSpec {
            shape: Shape::List,
            view: View::Page,
            nodes: 400,
            seed: DEFAULT_SEED,
        });
        let targets = targets_of(&base);
        assert!(targets.len() >= 3);
        let mut d = Driver::start(4096, targets);
        d.run_to(12, &base);
        assert!(!d.finished());
        assert_eq!(d.steps(), 12);
    }
}
