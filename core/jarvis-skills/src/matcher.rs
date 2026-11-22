//! Matching the owner's request against skill triggers.
//!
//! Both sides are normalised like `jarvis_memory::intent_key` (case, punctuation,
//! whitespace, spelling variants such as `Wi-Fi`/`wifi`, filler words such as "please" and
//! "the"). Then, per trigger:
//!
//! - **Literal trigger** (no slots): equal word sequences score [`SCORE_EXACT`]; otherwise
//!   a word-set overlap (Dice coefficient) of at least [`MIN_OVERLAP_PERMILLE`] scores up to
//!   [`SCORE_OVERLAP_MAX`].
//! - **Slotted trigger** (`text {contact} {message}`): the literal words must appear in
//!   order (filler words in the request may sit between them), every slot captures at least
//!   one word, slots before the last capture as few words as possible and the last slot
//!   takes the rest (`text mom i am on my way` -> contact `mom`, message `i am on my
//!   way`). Values are cut from the original text. The score grows with the number of
//!   literal words (more specific triggers win), between [`SCORE_SLOTTED_BASE`] and
//!   [`SCORE_EXACT`] (exclusive).
//!
//! Each skill reports its best trigger; disabled skills never match. Results are sorted by
//! score, then id. When the best two are within [`AMBIGUITY_MARGIN`] the result is
//! ambiguous ([`Ambiguity::is_ambiguous`]) and the caller should ask, not guess.
//!
//! Cost: requests longer than [`MAX_TASK_CHARS`] characters match nothing (checked without
//! scanning the whole text). For each trigger the work is `O(T × W)` for `T` trigger parts
//! (bounded by the trigger length cap) and `W` request words, after an `O(W)` check that
//! every literal word occurs at all; there is no backtracking.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::skill::{parse_trigger, Part, Skill};
use crate::store::SkillStore;
use crate::text::{longer_than, words, Word};

/// Longest request matched, in characters. Longer text matches nothing.
pub const MAX_TASK_CHARS: usize = 500;
/// Score of an exact literal match (scores are per mille, 0 to 1000).
pub const SCORE_EXACT: u32 = 1000;
/// Lowest score of a slotted match.
pub const SCORE_SLOTTED_BASE: u32 = 800;
/// Score added per literal word of a slotted trigger (at most six count).
pub const SCORE_PER_LITERAL: u32 = 30;
/// Highest score of a partial (overlap) literal match.
pub const SCORE_OVERLAP_MAX: u32 = 800;
/// Lowest word overlap (Dice coefficient, per mille) that counts as a match.
pub const MIN_OVERLAP_PERMILLE: u32 = 750;
/// Two best matches closer than this (per mille) are ambiguous.
pub const AMBIGUITY_MARGIN: u32 = 50;

/// One skill that matches a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillMatch {
    /// The skill.
    pub skill_id: String,
    /// How well it matches, per mille (see the module docs).
    pub score: u32,
    /// Values captured by the trigger's slots, keyed by parameter name.
    pub args: BTreeMap<String, String>,
    /// The trigger that matched, as written.
    pub matched_trigger: String,
    /// Required parameters the match did not fill (the caller should ask for them).
    #[serde(default)]
    pub missing: Vec<String>,
}

/// Ambiguity of a sorted match list.
pub trait Ambiguity {
    /// True when at least two skills match and the best two are within
    /// [`AMBIGUITY_MARGIN`]: report both, do not guess.
    fn is_ambiguous(&self) -> bool;
}

impl Ambiguity for [SkillMatch] {
    fn is_ambiguous(&self) -> bool {
        match self {
            [a, b, ..] => a.score.saturating_sub(b.score) < AMBIGUITY_MARGIN,
            _ => false,
        }
    }
}

impl Ambiguity for Vec<SkillMatch> {
    fn is_ambiguous(&self) -> bool {
        self.as_slice().is_ambiguous()
    }
}

impl SkillStore {
    /// The enabled skills whose triggers match `text`, best first (score, then id).
    /// Deterministic; see the module docs for the rules and the cost bound.
    pub fn match_task(&self, text: &str) -> Vec<SkillMatch> {
        match_skills(self.list(), text)
    }
}

/// [`SkillStore::match_task`] over any list of skills.
pub fn match_skills(skills: &[Skill], text: &str) -> Vec<SkillMatch> {
    if longer_than(text, MAX_TASK_CHARS) {
        return Vec::new();
    }
    let ws = words(text);
    let tokens: Vec<&str> = ws
        .iter()
        .filter(|w| !w.filler)
        .map(|w| w.norm.as_str())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let token_set: BTreeSet<&str> = tokens.iter().copied().collect();
    let mut out: Vec<SkillMatch> = Vec::new();
    for skill in skills.iter().filter(|s| s.enabled) {
        let mut best: Option<(u32, BTreeMap<String, String>, &str)> = None;
        for trigger in &skill.triggers {
            let Ok(parts) = parse_trigger(trigger) else {
                continue;
            };
            let found = if parts.iter().any(|p| matches!(p, Part::Slot(_))) {
                match_slotted(&parts, &ws, &token_set, text)
            } else {
                match_literal(&parts, &tokens, &token_set).map(|s| (s, BTreeMap::new()))
            };
            if let Some((score, args)) = found {
                if best.as_ref().is_none_or(|(b, _, _)| score > *b) {
                    best = Some((score, args, trigger.as_str()));
                }
            }
        }
        if let Some((score, args, trigger)) = best {
            let missing = skill.missing_params(&args);
            out.push(SkillMatch {
                skill_id: skill.id.clone(),
                score,
                args,
                matched_trigger: trigger.to_owned(),
                missing,
            });
        }
    }
    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.skill_id.cmp(&b.skill_id))
    });
    out
}

fn literal_words(parts: &[Part]) -> impl Iterator<Item = &str> {
    parts.iter().filter_map(|p| match p {
        Part::Lit(l) => Some(l.as_str()),
        Part::Slot(_) => None,
    })
}

fn match_literal(parts: &[Part], tokens: &[&str], token_set: &BTreeSet<&str>) -> Option<u32> {
    let lits: Vec<&str> = literal_words(parts).collect();
    if lits.as_slice() == tokens {
        return Some(SCORE_EXACT);
    }
    let lit_set: BTreeSet<&str> = lits.iter().copied().collect();
    let common = lit_set.intersection(token_set).count();
    let total = lit_set.len() + token_set.len();
    if total == 0 {
        return None;
    }
    let dice = u32::try_from(2 * common * 1000 / total).unwrap_or(0);
    // Below exact even at full set overlap (same words, different order or repeats).
    (dice >= MIN_OVERLAP_PERMILLE).then(|| (dice * SCORE_OVERLAP_MAX / 1000).min(SCORE_EXACT - 1))
}

/// Matches a slotted trigger. `reach[t][i]`: parts `t..` match words `i..`. Computed
/// backwards in `O(T × W)` with a suffix "any" for slots, then walked forwards choosing
/// the shortest capture for every slot but the last.
fn match_slotted(
    parts: &[Part],
    ws: &[Word],
    token_set: &BTreeSet<&str>,
    text: &str,
) -> Option<(u32, BTreeMap<String, String>)> {
    // Cheap rejection: every literal word must occur somewhere.
    if !literal_words(parts).all(|l| token_set.contains(l)) {
        return None;
    }
    let n = ws.len();
    // next_real[i]: first non-filler index >= i (n when none).
    let mut next_real = vec![n; n + 1];
    for i in (0..n).rev() {
        let filler = ws.get(i).is_some_and(|w| w.filler);
        let v = if filler {
            next_real.get(i + 1).copied().unwrap_or(n)
        } else {
            i
        };
        if let Some(slot) = next_real.get_mut(i) {
            *slot = v;
        }
    }
    let t_len = parts.len();
    let mut reach = vec![vec![false; n + 1]; t_len + 1];
    // Only filler words may follow the last part.
    if let Some(last) = reach.get_mut(t_len) {
        for (i, r) in last.iter_mut().enumerate() {
            *r = next_real.get(i).copied() == Some(n);
        }
    }
    for t in (0..t_len).rev() {
        let (head, tail) = reach.split_at_mut(t + 1);
        let (Some(row), Some(next)) = (head.last_mut(), tail.first()) else {
            return None;
        };
        match parts.get(t) {
            Some(Part::Lit(l)) => {
                for i in 0..=n {
                    let k = next_real.get(i).copied().unwrap_or(n);
                    let ok = k < n
                        && ws.get(k).is_some_and(|w| w.norm == *l)
                        && next.get(k + 1).copied().unwrap_or(false);
                    if let Some(r) = row.get_mut(i) {
                        *r = ok;
                    }
                }
            }
            Some(Part::Slot(_)) if t + 1 == t_len => {
                // The last slot takes the rest: at least one word.
                for (i, r) in row.iter_mut().enumerate() {
                    *r = i < n;
                }
            }
            Some(Part::Slot(_)) => {
                // Some j > i with next[j]: a suffix "any" over next.
                let mut any_after = false;
                for i in (0..=n).rev() {
                    if let Some(r) = row.get_mut(i) {
                        *r = any_after;
                    }
                    any_after = any_after || next.get(i).copied().unwrap_or(false);
                }
            }
            None => return None,
        }
    }
    if !reach
        .first()
        .and_then(|r| r.first())
        .copied()
        .unwrap_or(false)
    {
        return None;
    }
    // Walk forwards.
    let mut args = BTreeMap::new();
    let mut i = 0usize;
    let mut literals = 0u32;
    for (t, part) in parts.iter().enumerate() {
        match part {
            Part::Lit(_) => {
                i = next_real.get(i).copied().unwrap_or(n) + 1;
                literals = literals.saturating_add(1);
            }
            Part::Slot(name) => {
                let end = if t + 1 == t_len {
                    n
                } else {
                    let next = reach.get(t + 1)?;
                    (i + 1..=n).find(|j| next.get(*j).copied().unwrap_or(false))?
                };
                let first = ws.get(i)?;
                let last = ws.get(end.checked_sub(1)?)?;
                let value = text.get(first.start..last.end)?.trim().to_owned();
                args.insert(name.clone(), value);
                i = end;
            }
        }
    }
    let score = SCORE_SLOTTED_BASE + SCORE_PER_LITERAL * literals.min(6);
    Some((score.min(SCORE_EXACT - 1), args))
}
