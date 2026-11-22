//! One answer to "what did the owner mean?" across workflow phrases and skill triggers.
//!
//! Rule, in order:
//!
//! 1. A workflow phrase is exact by construction (normalised equality). Two or more
//!    matching workflows, or a workflow together with an exact skill match, need the
//!    owner's choice.
//! 2. One workflow and no exact skill match: the workflow (an exact match beats a loose
//!    one).
//! 3. No workflow: a single clear exact skill match runs; ambiguous or loose skill matches
//!    need the owner's choice ([`crate::Ambiguity::needs_owner_choice`]).
//! 4. Nothing matched: [`Resolution::NoMatch`] (the shell runs it as a free-text task).

use crate::matcher::{Ambiguity, SkillMatch};
use crate::store::SkillStore;
use crate::workflow_store::WorkflowStore;

/// What to do with an utterance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Run this workflow.
    RunWorkflow {
        /// Its id.
        workflow_id: String,
    },
    /// Run this skill match (exact and unambiguous; check `missing` before starting).
    RunSkill(SkillMatch),
    /// Ask the owner which one they meant (or confirm a loose suggestion).
    OwnerChoice,
    /// Neither a workflow nor a skill: an ordinary task.
    NoMatch,
}

/// Every match for an utterance and the decision taken from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtteranceMatch {
    /// Ids of enabled workflows whose phrase trigger matches, ordered by id.
    pub workflows: Vec<String>,
    /// Skill matches, best first.
    pub skills: Vec<SkillMatch>,
    /// The decision (see the module docs).
    pub resolution: Resolution,
}

/// Matches `text` against both stores and applies the module's precedence rule.
pub fn resolve_utterance(
    skills: &SkillStore,
    workflows: &WorkflowStore,
    text: &str,
) -> UtteranceMatch {
    let wf: Vec<String> = workflows
        .match_phrase(text)
        .into_iter()
        .map(|w| w.id.clone())
        .collect();
    let sk = skills.match_task(text);
    let exact_skill = sk.first().is_some_and(|m| m.exact);
    let resolution = match wf.as_slice() {
        [] if sk.is_empty() => Resolution::NoMatch,
        [] if sk.needs_owner_choice() => Resolution::OwnerChoice,
        [] => sk
            .first()
            .cloned()
            .map_or(Resolution::NoMatch, Resolution::RunSkill),
        [one] if !exact_skill => Resolution::RunWorkflow {
            workflow_id: one.clone(),
        },
        _ => Resolution::OwnerChoice,
    };
    UtteranceMatch {
        workflows: wf,
        skills: sk,
        resolution,
    }
}
