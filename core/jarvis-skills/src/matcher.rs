//! Matching the owner's request against skill triggers.
//!
//! Both sides are normalised like `jarvis_memory::intent_key` (case, punctuation,
//! whitespace, spelling variants such as `Wi-Fi`/`wifi`, filler words such as "please" and
//! "the"). Then, per trigger:
//!
//! - **Literal trigger** (no slots): equal word sequences score [`SCORE_EXACT`] and are
//!   *exact*. Otherwise a word-set overlap (Dice coefficient) of at least
//!   [`MIN_OVERLAP_PERMILLE`] is a *loose* match scoring up to [`SCORE_OVERLAP_MAX`], unless
//!   the request holds a negating word the trigger lacks ("do not", "never", "stop"...) or
//!   the words that differ include an opposite pair (on/off, enable/disable, open/close,
//!   send/delete...): those are no match at all.
//! - **Slotted trigger** (`text {contact} {message}`): the literal words must appear in
//!   order (filler words in the request may sit between them) with nothing else outside
//!   the slots, so a slotted match is always exact. Every capture holds at least one
//!   non-filler word (`call the` does not match `call {who}`). Slots before the last start
//!   at their first non-filler word (`text the mom hi` -> contact `mom`) and capture as few
//!   words as possible; the last slot takes the rest as written, leading words included
//!   (`text mom the train is late` -> message `the train is late`). Values are cut
//!   from the original text. The score grows with the number of literal words (more
//!   specific triggers win), between [`SCORE_SLOTTED_BASE`] and [`SCORE_EXACT`] (exclusive).
//!
//! Each skill reports its best trigger; disabled skills never match. Results are sorted by
//! score, then id.
//!
//! **Only an exact, unambiguous best match may run without asking the owner**
//! ([`Ambiguity::needs_owner_choice`]): when the best two are within [`AMBIGUITY_MARGIN`]
//! the result is ambiguous, and a loose match is only a suggestion.
//!
//! Cost: requests longer than [`MAX_TASK_CHARS`] characters match nothing (checked without
//! scanning the whole text). The request is tokenised once; triggers are parsed once when
//! a skill enters the store ([`crate::SkillStore`] keeps them compiled). Per trigger the
//! work is an `O(W)` check that every literal word occurs, then `O(T × W)` for `T` trigger
//! parts and `W` request words in a scratch table reused across triggers; no backtracking.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::skill::{parse_trigger, Part, Skill};
use crate::store::SkillStore;
use crate::text::{longer_than, opposite, words, Word, NEGATORS};

/// Longest request matched, in characters. Longer text matches nothing.
pub const MAX_TASK_CHARS: usize = 500;
/// Score of an exact literal match (scores are per mille, 0 to 1000).
pub const SCORE_EXACT: u32 = 1000;
/// Lowest score of a slotted match.
pub const SCORE_SLOTTED_BASE: u32 = 800;
/// Score added per literal word of a slotted trigger (at most six count).
pub const SCORE_PER_LITERAL: u32 = 30;
/// Highest score of a loose (overlap) literal match.
pub const SCORE_OVERLAP_MAX: u32 = 800;
/// Lowest word overlap (Dice coefficient, per mille) that counts as a loose match.
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
    /// True for an exact literal match (score 1000) or a fully matched slotted trigger;
    /// false for a loose word-overlap match. Only exact matches may run without asking.
    #[serde(default)]
    pub exact: bool,
}

/// Ambiguity of a sorted match list.
pub trait Ambiguity {
    /// True when at least two skills match and the best two are within
    /// [`AMBIGUITY_MARGIN`]: report both, do not guess.
    fn is_ambiguous(&self) -> bool;

    /// True when the owner must choose before anything runs: the result is ambiguous, or
    /// the best match is not exact. False for no match at all (nothing to choose) and for a
    /// single clear exact match.
    fn needs_owner_choice(&self) -> bool;
}

impl Ambiguity for [SkillMatch] {
    fn is_ambiguous(&self) -> bool {
        match self {
            [a, b, ..] => a.score.saturating_sub(b.score) < AMBIGUITY_MARGIN,
            _ => false,
        }
    }

    fn needs_owner_choice(&self) -> bool {
        match self.first() {
            None => false,
            Some(best) => !best.exact || self.is_ambiguous(),
        }
    }
}

impl Ambiguity for Vec<SkillMatch> {
    fn is_ambiguous(&self) -> bool {
        self.as_slice().is_ambiguous()
    }

    fn needs_owner_choice(&self) -> bool {
        self.as_slice().needs_owner_choice()
    }
}

/// A trigger parsed once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledTrigger {
    /// Index into the skill's `triggers`.
    index: usize,
    parts: Vec<Part>,
    slotted: bool,
    /// Literal words in order.
    lits: Vec<String>,
}

/// A skill's triggers parsed once (unparsable triggers are left out; validation refuses
/// them anyway).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct CompiledSkill {
    triggers: Vec<CompiledTrigger>,
}

/// Parses every trigger of `skill`.
pub(crate) fn compile(skill: &Skill) -> CompiledSkill {
    let triggers = skill
        .triggers
        .iter()
        .enumerate()
        .filter_map(|(index, t)| {
            let parts = parse_trigger(t).ok()?;
            let slotted = parts.iter().any(|p| matches!(p, Part::Slot(_)));
            let lits = parts
                .iter()
                .filter_map(|p| match p {
                    Part::Lit(l) => Some(l.clone()),
                    Part::Slot(_) => None,
                })
                .collect();
            Some(CompiledTrigger {
                index,
                parts,
                slotted,
                lits,
            })
        })
        .collect();
    CompiledSkill { triggers }
}

impl SkillStore {
    /// The enabled skills whose triggers match `text`, best first (score, then id).
    /// Deterministic; see the module docs for the rules and the cost bound.
    pub fn match_task(&self, text: &str) -> Vec<SkillMatch> {
        let (skills, compiled) = self.compiled();
        match_compiled(skills, compiled, text)
    }
}

/// [`SkillStore::match_task`] over any list of skills (parses their triggers on the fly).
pub fn match_skills(skills: &[Skill], text: &str) -> Vec<SkillMatch> {
    let compiled: Vec<CompiledSkill> = skills.iter().map(compile).collect();
    match_compiled(skills, &compiled, text)
}

/// The tokenised request, computed once per call.
struct Request<'a> {
    text: &'a str,
    ws: Vec<Word>,
    tokens: Vec<&'a str>,
    token_set: BTreeSet<&'a str>,
    /// Index in `ws` of each non-filler word, in order.
    real_pos: Vec<usize>,
    /// Per distinct non-filler word: a bitset over non-filler positions where it occurs.
    occurs: BTreeMap<&'a str, Vec<u64>>,
    /// u64 words per bitset row (bits 0..=m for m non-filler words).
    row_words: usize,
    negators: Vec<&'static str>,
}

pub(crate) fn match_compiled(
    skills: &[Skill],
    compiled: &[CompiledSkill],
    text: &str,
) -> Vec<SkillMatch> {
    if longer_than(text, MAX_TASK_CHARS) {
        return Vec::new();
    }
    let ws = words(text);
    // Normal forms live in `ws`; borrow them for the token views.
    let norms: Vec<String> = ws.iter().map(|w| w.norm.clone()).collect();
    let tokens: Vec<&str> = ws
        .iter()
        .zip(norms.iter())
        .filter(|(w, _)| !w.filler)
        .map(|(_, n)| n.as_str())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let real_pos: Vec<usize> = ws
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.filler)
        .map(|(i, _)| i)
        .collect();
    let m = tokens.len();
    let row_words = (m + 1).div_ceil(64);
    let mut occurs: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for (c, tok) in tokens.iter().enumerate() {
        let bits = occurs.entry(tok).or_insert_with(|| vec![0; row_words]);
        set_bit(bits, c);
    }
    let token_set: BTreeSet<&str> = tokens.iter().copied().collect();
    let negators = NEGATORS
        .iter()
        .copied()
        .filter(|w| token_set.contains(w))
        .collect();
    let req = Request {
        text,
        ws,
        tokens,
        token_set,
        real_pos,
        occurs,
        row_words,
        negators,
    };
    let mut scratch: Vec<u64> = Vec::new();
    let mut out: Vec<SkillMatch> = Vec::new();
    for (skill, comp) in skills.iter().zip(compiled.iter()) {
        if !skill.enabled {
            continue;
        }
        let mut best: Option<(u32, bool, BTreeMap<String, String>, usize)> = None;
        for t in &comp.triggers {
            let found = if t.slotted {
                match_slotted(t, &req, &mut scratch).map(|(s, a)| (s, true, a))
            } else {
                match_literal(t, &req).map(|(s, exact)| (s, exact, BTreeMap::new()))
            };
            if let Some((score, exact, args)) = found {
                if best.as_ref().is_none_or(|(b, _, _, _)| score > *b) {
                    best = Some((score, exact, args, t.index));
                }
            }
        }
        if let Some((score, exact, args, index)) = best {
            let missing = skill.missing_params(&args);
            out.push(SkillMatch {
                skill_id: skill.id.clone(),
                score,
                args,
                matched_trigger: skill.triggers.get(index).cloned().unwrap_or_default(),
                missing,
                exact,
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

/// `(score, exact)` of a literal trigger.
fn match_literal(t: &CompiledTrigger, req: &Request<'_>) -> Option<(u32, bool)> {
    if t.lits
        .iter()
        .map(String::as_str)
        .eq(req.tokens.iter().copied())
    {
        return Some((SCORE_EXACT, true));
    }
    let lit_set: BTreeSet<&str> = t.lits.iter().map(String::as_str).collect();
    let common = lit_set.intersection(&req.token_set).count();
    let total = lit_set.len() + req.token_set.len();
    if total == 0 {
        return None;
    }
    let dice = u32::try_from(2 * common * 1000 / total).unwrap_or(0);
    if dice < MIN_OVERLAP_PERMILLE {
        return None;
    }
    // A negation the trigger does not contain turns the request around.
    if req.negators.iter().any(|w| !lit_set.contains(w)) {
        return None;
    }
    // So does swapping a word for its opposite ("turn on" for "turn off").
    let only_request: Vec<&str> = req.token_set.difference(&lit_set).copied().collect();
    let only_trigger: Vec<&str> = lit_set.difference(&req.token_set).copied().collect();
    if only_request
        .iter()
        .any(|a| only_trigger.iter().any(|b| opposite(a, b)))
    {
        return None;
    }
    // Below exact even at full set overlap (same words, different order or repeats).
    Some((
        (dice * SCORE_OVERLAP_MAX / 1000).min(SCORE_EXACT - 1),
        false,
    ))
}

fn set_bit(bits: &mut [u64], i: usize) {
    if let Some(w) = bits.get_mut(i / 64) {
        *w |= 1u64 << (i % 64);
    }
}

fn get_bit(bits: &[u64], i: usize) -> bool {
    bits.get(i / 64)
        .is_some_and(|w| w & (1u64 << (i % 64)) != 0)
}

/// Index of the highest set bit, if any.
fn highest_bit(bits: &[u64]) -> Option<usize> {
    bits.iter()
        .enumerate()
        .rev()
        .find(|(_, w)| **w != 0)
        .map(|(i, w)| i * 64 + 63 - w.leading_zeros() as usize)
}

/// Matches a slotted trigger.
///
/// Every row of the table depends only on the next non-filler word, so it runs over the
/// `m` non-filler words: `reach[t][c]` is whether parts `t..` match from non-filler word
/// `c` on, stored as a bitset over `c in 0..=m`. Rows are built backwards: the end row is
/// `{m}` (only filler may follow); a literal row is `occurs(l) & (next >> 1)`; the last
/// slot's row is `{c < m}` (it needs a word); an earlier slot's row is `{c < h}` with `h`
/// the highest bit of the next row (some later split point exists). That is `O(T × m/64)`
/// per trigger. The walk forwards then picks the shortest capture for every slot but the
/// last, mapping non-filler positions back to the original words for the values.
fn match_slotted(
    t: &CompiledTrigger,
    req: &Request<'_>,
    scratch: &mut Vec<u64>,
) -> Option<(u32, BTreeMap<String, String>)> {
    // Cheap rejection: every literal word must occur somewhere.
    if !t.lits.iter().all(|l| req.token_set.contains(l.as_str())) {
        return None;
    }
    let m = req.tokens.len();
    let nw = req.row_words;
    let t_len = t.parts.len();
    scratch.clear();
    scratch.resize((t_len + 1) * nw, 0);
    if let Some(end) = scratch.get_mut(t_len * nw..) {
        set_bit(end, m);
    }
    for ti in (0..t_len).rev() {
        let (head, tail) = scratch.split_at_mut((ti + 1) * nw);
        let row = head.get_mut(ti * nw..)?;
        let next = tail.get(..nw)?;
        match t.parts.get(ti)? {
            Part::Lit(l) => {
                let occ = req.occurs.get(l.as_str())?;
                for (w, r) in row.iter_mut().enumerate() {
                    let here = next.get(w).copied().unwrap_or(0) >> 1;
                    let carry = next.get(w + 1).copied().unwrap_or(0) << 63;
                    *r = occ.get(w).copied().unwrap_or(0) & (here | carry);
                }
            }
            Part::Slot(_) => {
                let limit = if ti + 1 == t_len {
                    Some(m)
                } else {
                    highest_bit(next)
                };
                if let Some(h) = limit {
                    for c in 0..h {
                        set_bit(row, c);
                    }
                }
            }
        }
    }
    if !get_bit(scratch.get(..nw)?, 0) {
        return None;
    }
    // Walk forwards: `c` is the non-filler position, `i` the original word position.
    let mut args = BTreeMap::new();
    let mut c = 0usize;
    let mut i = 0usize;
    let mut literals = 0u32;
    let n = req.ws.len();
    for (ti, part) in t.parts.iter().enumerate() {
        match part {
            Part::Lit(_) => {
                i = req.real_pos.get(c)? + 1;
                c += 1;
                literals = literals.saturating_add(1);
            }
            Part::Slot(name) => {
                let (start, last) = if ti + 1 == t_len {
                    // A free-text last slot keeps the owner's leading words ("the train
                    // is late").
                    c = m;
                    (i, n.checked_sub(1)?)
                } else {
                    // Earlier slots start at their first real word ("the mom" -> "mom").
                    let next = scratch.get((ti + 1) * nw..(ti + 2) * nw)?;
                    let j = (c + 1..=m).find(|j| get_bit(next, *j))?;
                    let start = *req.real_pos.get(c)?;
                    let last = *req.real_pos.get(j - 1)?;
                    c = j;
                    i = last + 1;
                    (start, last)
                };
                let first = req.ws.get(start)?;
                let last = req.ws.get(last)?;
                let value = req.text.get(first.start..last.end)?.trim().to_owned();
                args.insert(name.clone(), value);
            }
        }
    }
    let score = SCORE_SLOTTED_BASE + SCORE_PER_LITERAL * literals.min(6);
    Some((score.min(SCORE_EXACT - 1), args))
}
