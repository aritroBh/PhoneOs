//! Review pass 36: risk phrases matched against the look-alike **character sets** of a
//! label, in place of pass 35's enumerated readings.
//!
//! After the normalisation the plain tokenizer shares ([`crate::norm`], review pass 37:
//! canonical decomposition, every Mn/Me mark and every invisible or format character
//! dropped, apostrophes removed without a split, fullwidth, styled and every script's
//! digits folded; Cyrillic and Greek look-alikes folded here) each visible character of a
//! label is one [`Unit`] that stands for a **set** of characters:
//!
//! * a letter for itself lowercased; a capital `I` (or a letter that folds to one) for `i`
//!   and `l`; a lowercase `l` for `l` and `i` where a capital `I` would be natural (first in
//!   its word, or beside an uppercase letter);
//! * a digit for itself, and where its run of look-alike characters touches a letter of the
//!   same word also `0` o, `1` i/l, `2` z, `3` e, `4` a, `5` s, `6` b/g, `7` t, `8` b,
//!   `9` g;
//! * `@` a or e (review pass 38: every `@`, an address's too — the plain reading keeps it
//!   a separator, both are carried), `$` s, `|` and `¡` i/l, `€` e/c, `¢` c, `¥` y, `£` l/e
//!   touching a letter of the word, and `!` i/l with a letter on both sides, decided after
//!   the spelled-out joins;
//! * review pass 38: punctuation inside a whitespace-delimited word (ASCII punctuation and
//!   [`crate::norm::is_word_punctuation`]: `De-lete`, `De.lete`, `Sign_Out`) is a *soft*
//!   boundary, as a case step is; any other symbol or space separates words.
//!
//! Boundaries: hard ones (a separator, the start and the end) and soft ones (a lowercase to
//! uppercase step, an uppercase letter before an uppercase letter followed by a lowercase
//! one, a letter/digit step, the joins of a spelled-out `P a y`, a soft space, punctuation
//! inside a word). A phrase matches when its
//! characters are in the units' sets position by position from a boundary to a boundary;
//! soft boundaries inside the match are ignored, a hard one is allowed only where the phrase
//! has a space, and a space of the phrase may also fall on a soft boundary or on none
//! (`DeleteAccount`, `deleteaccount`). The phrases are walked as a trie from each boundary,
//! so a label costs one pass times the phrases sharing its prefix, whatever the number of
//! ambiguous positions: there is no combination of readings to miss.
//!
//! **Exceptions** (a phrase of category `None`: `dont allow`, `resend code`, the generic
//! `ok`, …) swallow a match inside them only when they match **literally** — every
//! character the one the tokenizer reads and every space on a hard or soft boundary, which
//! is what the plain matcher honours. A look-alike spelling of an exception (`D0n't AIIow`)
//! never turns a risk word into a harmless one; the plain matcher's exceptions on ordinary
//! labels are honoured as before. Lowering tests (navigation, share-safe words) also read
//! literal matches only.

use std::cell::RefCell;
use std::sync::OnceLock;

use jarvis_proto::RiskCategory;

use crate::norm::{is_word_punctuation, piece, Piece};
use crate::phrases::{max_risk, LabelRisk, Phrase};
use crate::text::{fold_confusable, fold_upper_confusable, tokens};

/// The trie symbol of a phrase space (letters are 0..26, digits 26..36).
const SPACE: u8 = 36;
/// No literal character (a symbol read as a letter, a letter outside ASCII).
const NO_LIT: u8 = u8::MAX;

/// How a unit takes part in words and boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Kind {
    Lower,
    Upper,
    /// A letter without case (another script), or one no phrase can hold.
    Caseless,
    /// A decimal digit (any script, read as ASCII).
    Digit,
    /// A symbol read as a letter (`@`, `$`, `|`, `!`).
    Sym,
    /// A run of separators (spaces, punctuation, other symbols): a hard boundary.
    #[default]
    Sep,
}

/// A soft boundary before a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Soft {
    #[default]
    No,
    /// A case or letter/digit step: the plain tokenizer splits a word there.
    Step,
    /// The join of a spelled-out word (`P a y`): the plain matcher reads it joined.
    Glue,
    /// Review pass 38: punctuation inside a word (`De-lete`): the plain tokenizer splits
    /// there. Not counted against [`MAX_SOFT_BOUNDARIES`]: it stands where a separator
    /// stood, which started a walk as well.
    Punct,
}

/// One visible character of a label (or a run of separators).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Unit {
    /// The characters it stands for: bits 0..26 the letters, 26..36 the digits.
    set: u64,
    /// The character the plain tokenizer reads ([`NO_LIT`] for none).
    lit: u8,
    kind: Kind,
    soft: Soft,
    /// The folded character, for the look-alike rules (`I`, `l`, a digit, a symbol).
    src: char,
    /// A capital `I` or a letter that folds to one.
    capital_i: bool,
    /// Review pass 37 (F3): a soft space (review pass 38: or punctuation) lies before it
    /// inside its word (a soft boundary): it starts a word as the plain tokenizer reads it.
    narrow: bool,
}

fn sym_of(b: char) -> Option<u8> {
    match b {
        'a'..='z' => Some(b as u8 - b'a'),
        '0'..='9' => Some(26 + (b as u8 - b'0')),
        ' ' => Some(SPACE),
        _ => None,
    }
}

fn bit(c: char) -> u64 {
    sym_of(c).map_or(0, |s| 1u64 << s)
}

fn is_letter(k: Kind) -> bool {
    matches!(k, Kind::Lower | Kind::Upper | Kind::Caseless)
}

/// The letters a digit or symbol of a word stands for (review pass 35's table; review pass
/// 37 (F6): `¡` i/l, `€` e/c, `¢` c, `¥` y, `£` l/e, `9` g, `6` b/g, `2` z; review pass 38:
/// `@` also e, `Del@te`).
fn lookalike_letters(c: char) -> &'static [char] {
    match c {
        '0' => &['o'],
        '1' | '|' | '!' | '\u{A1}' => &['i', 'l'],
        '2' => &['z'],
        '3' => &['e'],
        '4' => &['a'],
        '@' => &['a', 'e'],
        '5' | '$' => &['s'],
        '6' => &['b', 'g'],
        '7' => &['t'],
        '8' => &['b'],
        '9' => &['g'],
        '\u{20AC}' => &['e', 'c'],
        '\u{A2}' => &['c'],
        '\u{A5}' => &['y'],
        '\u{A3}' => &['l', 'e'],
        _ => &[],
    }
}

/// A symbol that may be read as a letter where it touches one (`!` and `¡` aside: see
/// [`build_units`]).
fn is_lookalike_symbol(c: char) -> bool {
    matches!(
        c,
        '@' | '$' | '|' | '!' | '\u{A1}' | '\u{20AC}' | '\u{A2}' | '\u{A5}' | '\u{A3}'
    )
}

/// Review pass 38: whether `c` is punctuation that is a soft boundary inside a word: ASCII
/// punctuation and symbols other than the look-alike symbols (`@ $ | !`, read as letters),
/// the word punctuation of [`is_word_punctuation`] and (review pass 39) the drawn format
/// characters ([`crate::norm::is_drawn_format`]).
fn is_soft_punctuation(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_punctuation() && !is_lookalike_symbol(c)
    } else {
        // Review pass 39: a drawn format character (`U+0890`) too: the word is read across
        // it (as when it was dropped), and the intruder rule asks.
        is_word_punctuation(c) || crate::norm::is_drawn_format(c)
    }
}

/// Builds the units of `text` into `out` (cleared first). Every character first goes
/// through the normalisation the plain tokenizer shares ([`crate::norm::piece`]).
pub(crate) fn build_units(text: &str, out: &mut Vec<Unit>) {
    out.clear();
    // Review pass 39: multi-letter compatibility forms are their letters (`㏒` log).
    let text = crate::norm::expanded(text);
    let push = |u: Unit, out: &mut Vec<Unit>| {
        if u.kind == Kind::Sep && out.last().is_some_and(|l| l.kind == Kind::Sep) {
            return;
        }
        out.push(u);
    };
    let (mut narrow, mut punct) = (false, false);
    for raw in text.chars() {
        let c = match piece(raw) {
            None => continue,
            Some(Piece::Narrow) => {
                narrow = true;
                continue;
            }
            Some(Piece::Emoji(_)) => {
                narrow = false;
                punct = false;
                push(Unit::default(), out);
                continue;
            }
            Some(Piece::Char(c)) => c,
        };
        // Review pass 38: punctuation is decided by what follows it: inside a word a soft
        // boundary, else a separator.
        if is_soft_punctuation(c) {
            punct = true;
            continue;
        }
        let mut unit = if c.is_ascii_digit() {
            Unit {
                set: bit(c),
                lit: sym_of(c).unwrap_or(NO_LIT),
                kind: Kind::Digit,
                src: c,
                ..Unit::default()
            }
        } else if is_lookalike_symbol(c) {
            // A candidate: decided below, after the spelled-out joins (review pass 37,
            // F2: `$ E N D` is one word whose `$` touches its `E`).
            Unit {
                lit: NO_LIT,
                kind: Kind::Sym,
                src: c,
                ..Unit::default()
            }
        } else if c.is_alphabetic() {
            let (letter, kind) = if let Some(a) = fold_upper_confusable(c) {
                (a, Kind::Upper)
            } else {
                // A letter whose lowercase is several characters (`İ` is `i` and a
                // combining dot) is kept whole by the tokenizer: no phrase holds it.
                let mut low = c.to_lowercase();
                let lower = match (low.next(), low.next()) {
                    (Some(l), None) => l,
                    _ => c,
                };
                let kind = if c.is_uppercase() {
                    Kind::Upper
                } else if c.is_lowercase() {
                    Kind::Lower
                } else {
                    Kind::Caseless
                };
                (fold_confusable(lower), kind)
            };
            let ascii = letter.is_ascii_lowercase();
            Unit {
                set: if ascii { bit(letter) } else { 0 },
                lit: if ascii {
                    sym_of(letter).unwrap_or(NO_LIT)
                } else {
                    NO_LIT
                },
                kind,
                src: if ascii { letter } else { c },
                capital_i: kind == Kind::Upper && letter == 'i',
                ..Unit::default()
            }
        } else if c.is_numeric() {
            // A numeral the tokenizer keeps in a word (`²`, `Ⅻ`): no phrase holds it.
            Unit {
                lit: NO_LIT,
                kind: Kind::Digit,
                src: c,
                ..Unit::default()
            }
        } else {
            Unit::default()
        };
        // Review pass 37 (F3): a narrow space inside a word is a soft boundary (the plain
        // tokenizer splits there; the eye sees one word). Review pass 38: so is punctuation.
        let inside = unit.kind != Kind::Sep && out.last().is_some_and(|l| l.kind != Kind::Sep);
        let (was_narrow, was_punct) = (std::mem::take(&mut narrow), std::mem::take(&mut punct));
        if inside && was_narrow {
            unit.soft = Soft::Step;
            unit.narrow = true;
        } else if inside && was_punct {
            unit.soft = Soft::Punct;
            unit.narrow = true;
        } else if was_punct && unit.kind != Kind::Sep {
            // Punctuation at a word's start or after a separator separates.
            push(Unit::default(), out);
        }
        push(unit, out);
    }
    glue_spelled(out);
    let kind_at = |out: &[Unit], k: Option<usize>| k.and_then(|k| out.get(k)).map(|u| u.kind);
    // Symbols: a letter when their run of look-alikes touches a letter (`!` on both sides).
    let n = out.len();
    let mut k = 0usize;
    while k < n {
        let is_look = |u: &Unit| {
            (u.kind == Kind::Sym) || (u.kind == Kind::Digit && !lookalike_letters(u.src).is_empty())
        };
        if !out.get(k).is_some_and(is_look) {
            k += 1;
            continue;
        }
        let mut end = k;
        while out.get(end).is_some_and(is_look) {
            end += 1;
        }
        let left = kind_at(out, k.checked_sub(1)).is_some_and(is_letter);
        let right = kind_at(out, Some(end)).is_some_and(is_letter);
        for u in out.get_mut(k..end).into_iter().flatten() {
            let fold = if u.src == '!' {
                left && right
            } else {
                left || right
            };
            if u.kind == Kind::Sym {
                if fold {
                    u.set = lookalike_letters(u.src).iter().fold(0, |s, &l| s | bit(l));
                } else {
                    *u = Unit::default();
                }
            }
        }
        k = end;
    }
    // A symbol that stayed a separator may now stand next to another one.
    out.dedup_by(|b, a| a.kind == Kind::Sep && b.kind == Kind::Sep);
    // Digits (after the joins: `D.3.L.E.T.E` is one word), `I` and `l`, soft boundaries.
    let n = out.len();
    let mut k = 0usize;
    while k < n {
        let is_look = |u: &Unit| {
            matches!(u.kind, Kind::Sym | Kind::Digit) && !lookalike_letters(u.src).is_empty()
        };
        if !out.get(k).is_some_and(is_look) {
            k += 1;
            continue;
        }
        let mut end = k;
        while out.get(end).is_some_and(is_look) {
            end += 1;
        }
        let left = kind_at(out, k.checked_sub(1)).is_some_and(is_letter);
        let right = kind_at(out, Some(end)).is_some_and(is_letter);
        if left || right {
            for u in out.get_mut(k..end).into_iter().flatten() {
                if u.kind == Kind::Digit {
                    u.set |= lookalike_letters(u.src).iter().fold(0, |s, &l| s | bit(l));
                }
            }
        }
        k = end;
    }
    for k in 0..n {
        let prev = kind_at(out, k.checked_sub(1));
        let next = kind_at(out, k.checked_add(1));
        let Some(u) = out.get_mut(k) else { break };
        if u.kind == Kind::Sep {
            continue;
        }
        if u.capital_i {
            u.set |= bit('l');
        }
        let word_initial = u.narrow || prev.is_none_or(|p| p == Kind::Sep);
        if u.kind == Kind::Lower
            && u.src == 'l'
            && (word_initial || prev == Some(Kind::Upper) || next == Some(Kind::Upper))
        {
            u.set |= bit('i');
        }
        let Some(p) = prev.filter(|p| *p != Kind::Sep) else {
            continue;
        };
        if matches!(u.soft, Soft::Glue | Soft::Punct) {
            continue;
        }
        let step = (p == Kind::Lower && u.kind == Kind::Upper)
            || (p == Kind::Upper && u.kind == Kind::Upper && next == Some(Kind::Lower))
            || (is_letter(p) != is_letter(u.kind));
        if step {
            u.soft = Soft::Step;
        }
    }
}

/// Joins runs of two or more one-character words (a spelled-out `P a y`, `D.E.L.E.T.E`):
/// the separators between them are dropped and the joins are soft boundaries, as the plain
/// matcher reads them both joined and apart.
fn glue_spelled(out: &mut Vec<Unit>) {
    let single = |out: &[Unit], k: usize| {
        let me = out
            .get(k)
            .is_some_and(|u| matches!(u.kind, Kind::Lower | Kind::Upper | Kind::Digit | Kind::Sym));
        let left = k
            .checked_sub(1)
            .is_none_or(|p| out.get(p).is_none_or(|u| u.kind == Kind::Sep));
        let right = out.get(k + 1).is_none_or(|u| u.kind == Kind::Sep);
        me && left && right
    };
    // Mark separators lying between two single-character words.
    let n = out.len();
    let mut drop = false;
    for k in 1..n.saturating_sub(1) {
        if out.get(k).is_some_and(|u| u.kind == Kind::Sep)
            && single(out, k - 1)
            && single(out, k + 1)
        {
            drop = true;
            if let Some(u) = out.get_mut(k) {
                u.soft = Soft::Glue;
            }
        }
    }
    if !drop {
        return;
    }
    let mut glue_next = false;
    let mut w = 0usize;
    for r in 0..n {
        let Some(mut u) = out.get(r).copied() else {
            break;
        };
        if u.kind == Kind::Sep && u.soft == Soft::Glue {
            glue_next = true;
            continue;
        }
        if glue_next {
            u.soft = Soft::Glue;
            glue_next = false;
        }
        if let Some(slot) = out.get_mut(w) {
            *slot = u;
        }
        w += 1;
    }
    out.truncate(w);
}

/// A boundary lies before unit `k` (`k == units.len()` is the end).
fn boundary(u: &[Unit], k: usize) -> bool {
    if k == 0 || k >= u.len() {
        return true;
    }
    let here = u.get(k);
    let prev = u.get(k - 1);
    here.is_some_and(|h| h.kind == Kind::Sep || h.soft != Soft::No)
        || prev.is_some_and(|p| p.kind == Kind::Sep)
}

/// A phrase table as a trie over characters and spaces.
#[derive(Debug, Default)]
struct Trie {
    /// Per node: its edges (symbol, child).
    edges: Vec<Vec<(u8, u32)>>,
    /// Per node: the phrases ending there (category, generic).
    ends: Vec<Vec<(RiskCategory, bool)>>,
}

impl Trie {
    fn compile(table: &[Phrase]) -> Trie {
        let mut t = Trie {
            edges: vec![Vec::new()],
            ends: vec![Vec::new()],
        };
        for phrase in table {
            let text = tokens(phrase.text).join(" ");
            if text.is_empty() {
                continue;
            }
            let mut node = 0usize;
            let mut ok = true;
            for c in text.chars() {
                let Some(s) = sym_of(c) else {
                    ok = false;
                    break;
                };
                let found = t
                    .edges
                    .get(node)
                    .and_then(|e| e.iter().find(|(x, _)| *x == s).map(|(_, c)| *c));
                node = match found {
                    Some(c) => c as usize,
                    None => {
                        let id = t.edges.len();
                        t.edges.push(Vec::new());
                        t.ends.push(Vec::new());
                        if let Some(e) = t.edges.get_mut(node) {
                            e.push((s, u32::try_from(id).unwrap_or(u32::MAX)));
                        }
                        id
                    }
                };
            }
            if ok {
                if let Some(e) = t.ends.get_mut(node) {
                    e.push((phrase.category, phrase.inherits_context));
                }
            }
        }
        t
    }
}

/// Every table the crate matches against, compiled once.
fn tries() -> &'static [(&'static [Phrase], Trie)] {
    static TRIES: OnceLock<Vec<(&'static [Phrase], Trie)>> = OnceLock::new();
    TRIES.get_or_init(|| {
        crate::phrases::all_tables()
            .into_iter()
            .map(|t| (t, Trie::compile(t)))
            .collect()
    })
}

fn with_trie<R>(table: &[Phrase], f: impl FnOnce(&Trie) -> R) -> R {
    let cached = tries()
        .iter()
        .find(|(t, _)| std::ptr::eq(t.as_ptr(), table.as_ptr()) && t.len() == table.len())
        .or_else(|| {
            tries()
                .iter()
                .find(|(t, _)| t.len() == table.len() && *t == table)
        });
    match cached {
        Some((_, t)) => f(t),
        None => f(&Trie::compile(table)),
    }
}

/// One phrase found in the units.
#[derive(Debug, Clone, Copy)]
struct Hit {
    start: usize,
    end: usize,
    category: RiskCategory,
    generic: bool,
    /// Every character read as the tokenizer reads it, every space on a boundary.
    literal: bool,
    /// A character read as another than the tokenizer reads it (a look-alike).
    folded: bool,
}

/// Review pass 39 (F5): whether the match `units[start..end]` is a run of digits alone cut
/// out of a word at a soft boundary (the `60` of `S60`, `90` of `XC90`, `60` of `60ft`): a
/// number, not letters — a look-alike reading made only of digits counts only together with
/// a letter of its word (`Lo9`, `8in`, `S3nd`) or as a whole word of its own.
fn numbers_cut_out(units: &[Unit], start: usize, end: usize) -> bool {
    let Some(run) = units.get(start..end) else {
        return false;
    };
    let inside = |u: Option<&Unit>| u.is_some_and(|u| u.kind != Kind::Sep);
    run.iter().all(|u| u.kind == Kind::Digit)
        && (inside(start.checked_sub(1).and_then(|p| units.get(p))) || inside(units.get(end)))
}

struct Walk<'a> {
    trie: &'a Trie,
    units: &'a [Unit],
    start: usize,
    /// Only matches ending at the last unit.
    whole_end: Option<usize>,
}

impl Walk<'_> {
    fn go(&self, node: usize, pos: usize, (literal, folded): (bool, bool), out: &mut Vec<Hit>) {
        #[cfg(feature = "reference")]
        WALK_STEPS.with(|c| c.set(c.get().saturating_add(1)));
        let units = self.units;
        if pos > self.start
            && boundary(units, pos)
            && self.whole_end.is_none_or(|e| e == pos)
            && !numbers_cut_out(units, self.start, pos)
        {
            for &(category, generic) in self.trie.ends.get(node).into_iter().flatten() {
                out.push(Hit {
                    start: self.start,
                    end: pos,
                    category,
                    generic,
                    literal,
                    folded,
                });
            }
        }
        let Some(u) = units.get(pos) else { return };
        for &(s, child) in self.trie.edges.get(node).into_iter().flatten() {
            let child = child as usize;
            if s == SPACE {
                // A phrase space: a run of separators, a soft boundary, or no boundary.
                if pos == self.start {
                    continue;
                }
                if u.kind == Kind::Sep {
                    self.go(child, pos + 1, (literal, folded), out);
                } else {
                    self.go(child, pos, (literal && u.soft != Soft::No, folded), out);
                }
            } else if u.kind != Kind::Sep && u.set & (1u64 << s) != 0 {
                // A step inside a phrase word: the plain tokenizer splits the word there.
                let lit = literal
                    && u.lit == s
                    && (pos == self.start || !matches!(u.soft, Soft::Step | Soft::Punct));
                self.go(child, pos + 1, (lit, folded || u.lit != s), out);
            }
        }
    }
}

thread_local! {
    static UNITS: RefCell<Vec<Unit>> = const { RefCell::new(Vec::new()) };
}

#[cfg(feature = "reference")]
thread_local! {
    /// Review pass 37 (F8): the trie steps walked on this thread (tests count work by it).
    static WALK_STEPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The trie steps walked on this thread so far (feature `reference`, for tests).
#[cfg(feature = "reference")]
pub(crate) fn walk_steps() -> u64 {
    WALK_STEPS.with(std::cell::Cell::get)
}

/// Review pass 37 (F8): the most soft boundaries (case and letter/digit steps, spelled-out
/// joins, narrow spaces) a label's units may hold for the sets to walk it. Every soft
/// boundary is a start of the walk, so a label of alternating letters and digits (`a1a1…`,
/// 511 of them in 512 characters) cost 4 to 5 times an ordinary one. Measured
/// (`jarvis-observe/tests/pass37_fixes.rs`): the most any label, value, label and value
/// together, placeholder or identifier of the 142 real trees in the fixtures holds is 30 (a
/// Safari tab identifier, `TabDocument?IsLoadedUsingDesktopUserAgent=false&…`); this bound
/// is more than three times that. A pressable control whose label and value hold more is
/// `Unverified` (asked) without either reading, as a label past 512 characters is; on
/// other texts the sets give `Unverified` without the walk.
pub(crate) const MAX_SOFT_BOUNDARIES: usize = 96;

/// The soft boundaries of `units` ([`MAX_SOFT_BOUNDARIES`]).
pub(crate) fn soft_boundaries(units: &[Unit]) -> usize {
    units
        .iter()
        .filter(|u| matches!(u.soft, Soft::Step | Soft::Glue))
        .count()
}

/// Runs `f` with the units of `text` (a buffer reused per thread: no allocation once warm).
pub(crate) fn with_units<R>(text: &str, f: impl FnOnce(&[Unit]) -> R) -> R {
    UNITS.with(|cell| match cell.try_borrow_mut() {
        Ok(mut buf) => {
            build_units(text, &mut buf);
            f(&buf)
        }
        Err(_) => {
            let mut buf = Vec::new();
            build_units(text, &mut buf);
            f(&buf)
        }
    })
}

/// Every phrase of `tables` found in `units` (from every boundary, or only covering all of
/// them when `whole`).
fn hits(units: &[Unit], tables: &[&[Phrase]], whole: bool, out: &mut Vec<Hit>) {
    let first = usize::from(units.first().is_some_and(|u| u.kind == Kind::Sep));
    let last = units.len() - usize::from(units.last().is_some_and(|u| u.kind == Kind::Sep));
    if first >= last {
        return;
    }
    // Review pass 37 (F8): a label with more soft boundaries than any real one is asked
    // without the walk (every boundary starts one).
    if soft_boundaries(units) > MAX_SOFT_BOUNDARIES {
        out.push(Hit {
            start: first,
            end: last,
            category: RiskCategory::Unverified,
            generic: false,
            literal: false,
            folded: true,
        });
        return;
    }
    for table in tables {
        with_trie(table, |trie| {
            for start in first..last {
                if whole && start != first {
                    break;
                }
                let Some(u) = units.get(start) else { break };
                if u.kind == Kind::Sep || !boundary(units, start) || u.set == 0 {
                    continue;
                }
                Walk {
                    trie,
                    units,
                    start,
                    whole_end: whole.then_some(last),
                }
                .go(0, start, (true, false), out);
            }
        });
    }
}

/// The phrases of `tables` in `text` read by character sets, a match strictly inside a
/// longer one swallowed only when the longer one is literal or names a risk.
pub(crate) fn match_sets(text: &str, tables: &[&[Phrase]]) -> LabelRisk {
    match_sets_beyond(text, tables, false)
}

/// [`match_sets`]; with `beyond_plain`, only the matches that read a look-alike character
/// as another letter count (the others still swallow): what a look-alike adds to words the
/// plain reading took as content (`Device Rem0ve`, not `FaceTime HD Camera`).
pub(crate) fn match_sets_beyond(text: &str, tables: &[&[Phrase]], beyond_plain: bool) -> LabelRisk {
    with_units(text, |u| match_units(u, tables, beyond_plain))
}

/// [`match_sets_beyond`] on units already built.
pub(crate) fn match_units(units: &[Unit], tables: &[&[Phrase]], beyond_plain: bool) -> LabelRisk {
    let mut found = Vec::new();
    hits(units, tables, false, &mut found);
    found.sort_by_key(|h| (h.start, std::cmp::Reverse(h.end)));
    let mut out = LabelRisk::default();
    let mut best_end: Option<usize> = None;
    let mut k = 0usize;
    while k < found.len() {
        let Some(&Hit { start, end, .. }) = found.get(k) else {
            break;
        };
        let mut j = k;
        let swallowed = best_end.is_some_and(|b| b >= end);
        let mut swallows = false;
        while let Some(h) = found.get(j).filter(|h| h.start == start && h.end == end) {
            if !swallowed && !(beyond_plain && !h.folded) {
                out.category = max_risk(out.category, h.category);
                out.generic |= h.generic;
            }
            swallows |= h.literal || h.category != RiskCategory::None;
            j += 1;
        }
        if swallows {
            best_end = Some(best_end.map_or(end, |b| b.max(end)));
        }
        k = j;
    }
    out
}

/// How many words the units hold between hard boundaries (separators): the plain
/// tokenizer's count without its splits at case and letter/digit steps. Review pass 38:
/// punctuation inside a word still separates words here, so whether a row is content does
/// not change. Review pass 39 (F5): but not between two digits — a number with its
/// separators is one word (`£32,995`, `$12.50`, `1-800-555-0199`), as the plain reading
/// counts it ([`crate::text::row_words`]).
pub(crate) fn hard_words(units: &[Unit]) -> usize {
    units
        .iter()
        .enumerate()
        .filter(|(k, x)| {
            let prev = k.checked_sub(1).and_then(|p| units.get(p));
            x.kind != Kind::Sep
                && ((x.soft == Soft::Punct
                    && !(x.kind == Kind::Digit && prev.is_some_and(|y| y.kind == Kind::Digit)))
                    || prev.is_none_or(|y| y.kind == Kind::Sep))
        })
        .count()
}

/// The phrases of `tables` covering all of `text` read by character sets.
pub(crate) fn match_sets_whole(text: &str, tables: &[&[Phrase]]) -> LabelRisk {
    with_units(text, |u| whole_units(u, tables))
}

/// [`match_sets_whole`] on units already built.
pub(crate) fn whole_units(units: &[Unit], tables: &[&[Phrase]]) -> LabelRisk {
    let mut found = Vec::new();
    hits(units, tables, true, &mut found);
    found.iter().fold(LabelRisk::default(), |mut a, h| {
        a.category = max_risk(a.category, h.category);
        a.generic |= h.generic;
        a
    })
}

/// Whether a phrase of `table` (any row) is in the units read by character sets; only a
/// literal match counts when `literal` (a test that lowers a risk).
pub(crate) fn any_units(units: &[Unit], table: &[Phrase], literal: bool) -> bool {
    let mut found = Vec::new();
    hits(units, &[table], false, &mut found);
    found.iter().any(|h| h.literal || !literal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phrases::{match_label, ACTION_PHRASES};

    fn sets(t: &str) -> RiskCategory {
        match_sets(t, &[ACTION_PHRASES]).category
    }

    #[test]
    fn units_read_lookalikes_inside_words_only() {
        for (t, want) in [
            ("DeIete", RiskCategory::Delete),
            ("BIock", RiskCategory::AccountChange),
            ("SeII Now", RiskCategory::Payment),
            ("Iog Out", RiskCategory::AccountChange),
            ("Bl0ck", RiskCategory::AccountChange),
            ("SeIl", RiskCategory::Payment),
            ("DeLete", RiskCategory::Delete),
            ("dELETE ACCOUNT", RiskCategory::Delete),
            ("DeIeteAccount", RiskCategory::Delete),
            ("deleteaccount", RiskCategory::Delete),
            ("Rem\u{FF10}ve", RiskCategory::Delete),
            ("Sign \u{1D7CE}ut", RiskCategory::AccountChange),
            ("Send M0ney", RiskCategory::Payment),
            ("P a y", RiskCategory::Payment),
            ("D.3.L.E.T.E", RiskCategory::Delete),
            ("Insta11 Now", RiskCategory::SystemSetting),
            ("D0n't AIIow", RiskCategory::SystemSetting),
        ] {
            assert_eq!(sets(t), want, "{t:?}");
        }
        // Ordinary labels: what the plain matcher says.
        for t in [
            "Settings",
            "Top 10",
            "iOS 26",
            "Go!",
            "10:41",
            "john@example.com",
            "Don't Allow",
            "Resend Code",
            "Mall",
            "Mali",
            "MAIL",
            "Install",
            "5G",
            "Se7en",
        ] {
            assert_eq!(sets(t), match_label(t, &[ACTION_PHRASES]).category, "{t:?}");
        }
    }

    /// Pass 35's `lookalike_readings_fold_only_inside_words`, on the sets: the same labels.
    /// Ordinary ones read as the plain matcher reads them; the folded ones read as the word
    /// they draw; a number alone or beside a word stays a number.
    #[test]
    fn lookalike_readings_fold_only_inside_words() {
        for t in [
            "Settings",
            "Delete Account",
            "Cancel",
            "Top 10",
            "iOS 26",
            "Pay $5.00",
            "Go!",
            "10:41",
            "john@example.com",
            "PlaceOrder",
            "iPhone",
            "",
            "Top 10 5",
            "5G",
        ] {
            assert_eq!(sets(t), match_label(t, &[ACTION_PHRASES]).category, "{t:?}");
        }
        for (t, want) in [
            ("DeIete", "Delete"),
            ("CanceI Subscription", "Cancel Subscription"),
            ("SUBMlT", "SUBMIT"),
            ("Sign 0ut", "Sign out"),
            ("D0NATE", "DONATE"),
            ("Send M0ney", "Send Money"),
            ("Subm1t", "Submit"),
            ("De!ete", "Delete"),
            ("P@y $end", "Pay send"),
            ("DeLeTe", "delete"),
            ("Rem\u{200B}0ve", "Remove"),
        ] {
            let real = match_label(want, &[ACTION_PHRASES]);
            assert_ne!(real.category, RiskCategory::None, "{want:?}");
            assert_eq!(sets(t), real.category, "{t:?}");
        }
        // Folding adds no camel-case split: `Send M0ney` is a payment, not a message.
        assert_eq!(sets("Send M0ney"), RiskCategory::Payment);
    }
}
