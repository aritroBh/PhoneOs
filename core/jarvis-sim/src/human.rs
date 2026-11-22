//! The owner, as a policy: how `Confirm` and `AskUser` are answered.

use std::collections::{BTreeSet, VecDeque};

use jarvis_proto::{ConfirmRequest, RiskCategory};

/// How confirmation requests are decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmPolicy {
    /// Approve everything.
    ApproveAll,
    /// Decline everything.
    DeclineAll,
    /// Use these decisions in order; decline once they run out.
    Scripted(VecDeque<bool>),
    /// Approve everything except these categories.
    DeclineCategories(BTreeSet<RiskCategory>),
}

/// The simulated owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Human {
    confirm: ConfirmPolicy,
    answers: VecDeque<String>,
    typed: VecDeque<String>,
}

impl Human {
    /// Approves every confirmation.
    pub fn approve_all() -> Self {
        Self::with(ConfirmPolicy::ApproveAll)
    }

    /// Declines every confirmation.
    pub fn decline_all() -> Self {
        Self::with(ConfirmPolicy::DeclineAll)
    }

    /// Approves everything except the listed categories.
    pub fn decline_categories(categories: &[RiskCategory]) -> Self {
        Self::with(ConfirmPolicy::DeclineCategories(
            categories.iter().copied().collect(),
        ))
    }

    /// Decides confirmations from a list, declining after it runs out.
    pub fn scripted(decisions: &[bool]) -> Self {
        Self::with(ConfirmPolicy::Scripted(decisions.iter().copied().collect()))
    }

    fn with(confirm: ConfirmPolicy) -> Self {
        Self {
            confirm,
            answers: VecDeque::new(),
            typed: VecDeque::new(),
        }
    }

    /// Answers to `AskUser`, in order (an empty string once they run out).
    pub fn with_answers<S: AsRef<str>>(mut self, answers: &[S]) -> Self {
        self.answers = answers.iter().map(|a| a.as_ref().to_owned()).collect();
        self
    }

    /// Text the owner types on the phone at each `AskUser`, in order, into the first secure
    /// field on the screen (else the focused field). This is how a password gets entered:
    /// the agent hands the field over, the owner types, then answers.
    pub fn types_on_ask<S: AsRef<str>>(mut self, typed: &[S]) -> Self {
        self.typed = typed.iter().map(|a| a.as_ref().to_owned()).collect();
        self
    }

    /// A few words for transcripts: how this owner decides.
    pub fn describe(&self) -> String {
        match &self.confirm {
            ConfirmPolicy::ApproveAll => "approves everything".to_owned(),
            ConfirmPolicy::DeclineAll => "declines everything".to_owned(),
            ConfirmPolicy::Scripted(q) => format!("follows {} scripted decisions", q.len()),
            ConfirmPolicy::DeclineCategories(c) => {
                let names: Vec<String> = c.iter().map(|x| format!("{x:?}")).collect();
                format!("approves except {}", names.join(", "))
            }
        }
    }

    /// Decides one confirmation.
    pub fn confirm(&mut self, request: &ConfirmRequest) -> bool {
        match &mut self.confirm {
            ConfirmPolicy::ApproveAll => true,
            ConfirmPolicy::DeclineAll => false,
            ConfirmPolicy::Scripted(q) => q.pop_front().unwrap_or(false),
            ConfirmPolicy::DeclineCategories(c) => !c.contains(&request.category),
        }
    }

    /// Answers one question: the reply text and the text the owner types on the phone first.
    pub fn answer(&mut self, _question: &str) -> (String, Option<String>) {
        let typed = self.typed.pop_front();
        let reply = self.answers.pop_front().unwrap_or_else(|| {
            if typed.is_some() {
                "done".to_owned()
            } else {
                String::new()
            }
        });
        (reply, typed)
    }
}
