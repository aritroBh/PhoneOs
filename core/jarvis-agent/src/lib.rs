//! Sans-IO agent session for the Jarvis core: the observe, decide, gate, act, verify loop driven
//! by `TimedEvent`s and emitting `Effect`s, plus prompt building and tolerant action parsing.

#![forbid(unsafe_code)]
