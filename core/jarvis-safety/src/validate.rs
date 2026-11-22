//! Structural validation of a model-chosen action against the current screen.

use jarvis_proto::{Action, CompactScreen};
use thiserror::Error;

use crate::text::is_invisible;

/// URL schemes `OpenUrl` may use. Everything else (`javascript:`, `data:`, `file:`,
/// `shortcuts:`, `prefs:`, third-party app schemes...) is rejected, because a custom scheme
/// can run an action inside another app with no UI to gate.
///
/// * `http`, `https`: web pages (no embedded `user:pass@` credentials).
/// * `mailto`, `sms`: open a draft only; sending still needs a gated tap on Send.
/// * `tel`, `facetime`, `facetime-audio`: iOS shows its own call prompt; classified as
///   `SendMessage` so Jarvis asks first as well.
/// * `maps`: Apple Maps queries and directions.
pub const ALLOWED_URL_SCHEMES: &[&str] = &[
    "http",
    "https",
    "mailto",
    "sms",
    "tel",
    "facetime",
    "facetime-audio",
    "maps",
];

/// Longest text a single `Type` action may enter, in characters.
pub const MAX_TYPE_CHARS: usize = 2_000;
/// Longest `Wait`, in milliseconds.
pub const MAX_WAIT_MS: u64 = 10_000;
/// Longest `LongPress`, in milliseconds.
pub const MAX_LONG_PRESS_MS: u64 = 5_000;
/// Longest URL `OpenUrl` accepts, in bytes.
pub const MAX_URL_LEN: usize = 2_048;

/// Why an action cannot run on this screen.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ActionError {
    /// The element index is not on the current screen.
    #[error("element {0} is not on this screen")]
    UnknownElement(u32),
    /// The element does not accept interaction.
    #[error("element {0} is disabled")]
    DisabledElement(u32),
    /// `Type` or `Clear` targets something that is not a text input.
    #[error("element {element} is a {role}, not a text input")]
    NotTextInput {
        /// Element index.
        element: u32,
        /// Its role name.
        role: String,
    },
    /// `Type` without an element while no text input is focused.
    #[error("no text field is focused; name the element to type into")]
    NoFocusedInput,
    /// `Type` into a password field. Always blocked: Jarvis never types passwords itself;
    /// it taps the AutoFill suggestion (gated by Face ID) or asks the user to type.
    #[error("element {0} is a password field; use AutoFill or ask the user")]
    SecureFieldTyping(u32),
    /// `Type` with empty text and no Return, or `AskUser` with an empty question.
    #[error("text is empty")]
    EmptyText,
    /// `Type` text over [`MAX_TYPE_CHARS`].
    #[error("text is {len} characters; the limit is {max}")]
    TextTooLong {
        /// Character count.
        len: usize,
        /// The limit.
        max: usize,
    },
    /// `Type` text with newlines or other control characters, which would press Return
    /// (sending) without going through `submit`.
    #[error("text contains newlines or control characters; use submit to press Return")]
    ControlCharacters,
    /// `OpenUrl` with a scheme outside [`ALLOWED_URL_SCHEMES`].
    #[error("URL scheme \"{0}\" is not allowed")]
    UrlSchemeNotAllowed(String),
    /// `OpenUrl` with a URL that cannot be checked safely.
    #[error("malformed URL: {0}")]
    MalformedUrl(&'static str),
    /// `TapPoint` outside the normalized 0..1 square (or not a number).
    #[error("point ({x}, {y}) is outside 0..1")]
    PointOutOfRange {
        /// Normalized x.
        x: f64,
        /// Normalized y.
        y: f64,
    },
    /// `Wait` longer than [`MAX_WAIT_MS`].
    #[error("wait of {ms} ms exceeds {max} ms")]
    WaitTooLong {
        /// Requested milliseconds.
        ms: u64,
        /// The limit.
        max: u64,
    },
    /// `LongPress` longer than [`MAX_LONG_PRESS_MS`].
    #[error("long press of {ms} ms exceeds {max} ms")]
    PressTooLong {
        /// Requested milliseconds.
        ms: u64,
        /// The limit.
        max: u64,
    },
    /// `OpenApp` with an empty app name.
    #[error("app name is empty")]
    EmptyAppName,
    /// `CallTool` with an empty tool name.
    #[error("tool name is empty")]
    EmptyToolName,
    /// `CallTool` whose arguments are not a JSON object.
    #[error("tool arguments must be a JSON object")]
    BadToolArgs,
}

/// Checks that `action` is well-formed and fits `screen`: referenced elements exist and are
/// enabled, typing targets an enabled non-secure text input, text and waits are bounded,
/// URLs use an allowed scheme, `TapPoint` is inside 0..1.
pub fn validate_action(action: &Action, screen: &CompactScreen) -> Result<(), ActionError> {
    match action {
        Action::Tap { element } => enabled_element(screen, *element).map(|_| ()),
        Action::LongPress {
            element,
            duration_ms,
        } => {
            enabled_element(screen, *element)?;
            if *duration_ms > MAX_LONG_PRESS_MS {
                return Err(ActionError::PressTooLong {
                    ms: *duration_ms,
                    max: MAX_LONG_PRESS_MS,
                });
            }
            Ok(())
        }
        Action::Type {
            element,
            text,
            submit,
        } => {
            validate_text(text, *submit)?;
            match element {
                Some(i) => {
                    let el = text_input(screen, *i)?;
                    if el.secure {
                        return Err(ActionError::SecureFieldTyping(*i));
                    }
                    Ok(())
                }
                None => match crate::classify::type_target(None, screen) {
                    Some(el) if !el.enabled => Err(ActionError::DisabledElement(el.index)),
                    Some(el) if el.secure => Err(ActionError::SecureFieldTyping(el.index)),
                    Some(_) => Ok(()),
                    None if screen.keyboard_visible => Ok(()),
                    None => Err(ActionError::NoFocusedInput),
                },
            }
        }
        Action::Clear { element } => text_input(screen, *element).map(|_| ()),
        Action::Scroll { element, .. } | Action::Swipe { element, .. } => match element {
            Some(i) => screen
                .get(*i)
                .map(|_| ())
                .ok_or(ActionError::UnknownElement(*i)),
            None => Ok(()),
        },
        Action::TapPoint { x, y } => {
            let ok = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
            if ok(*x) && ok(*y) {
                Ok(())
            } else {
                Err(ActionError::PointOutOfRange { x: *x, y: *y })
            }
        }
        Action::OpenApp { app } => {
            if app.trim().is_empty() {
                Err(ActionError::EmptyAppName)
            } else {
                Ok(())
            }
        }
        Action::OpenUrl { url } => validate_url(url),
        Action::Wait { ms } => {
            if *ms > MAX_WAIT_MS {
                Err(ActionError::WaitTooLong {
                    ms: *ms,
                    max: MAX_WAIT_MS,
                })
            } else {
                Ok(())
            }
        }
        Action::CallTool { name, args } => {
            if name.trim().is_empty() {
                Err(ActionError::EmptyToolName)
            } else if !(args.is_object() || args.is_null()) {
                Err(ActionError::BadToolArgs)
            } else {
                Ok(())
            }
        }
        Action::AskUser { question } => {
            if question.trim().is_empty() {
                Err(ActionError::EmptyText)
            } else {
                Ok(())
            }
        }
        Action::PressHome | Action::Look | Action::Done { .. } | Action::Fail { .. } => Ok(()),
    }
}

/// The element `index`, which must exist and be enabled.
fn enabled_element(
    screen: &CompactScreen,
    index: u32,
) -> Result<&jarvis_proto::Element, ActionError> {
    let el = screen
        .get(index)
        .ok_or(ActionError::UnknownElement(index))?;
    if !el.enabled {
        return Err(ActionError::DisabledElement(index));
    }
    Ok(el)
}

/// The element `index`, which must exist, be enabled and accept text.
fn text_input(screen: &CompactScreen, index: u32) -> Result<&jarvis_proto::Element, ActionError> {
    let el = enabled_element(screen, index)?;
    if !el.role.is_text_input() {
        return Err(ActionError::NotTextInput {
            element: index,
            role: el.role.as_str().to_owned(),
        });
    }
    Ok(el)
}

/// Checks `Type` text: non-empty unless only pressing Return, bounded, no control chars.
fn validate_text(text: &str, submit: bool) -> Result<(), ActionError> {
    if text.is_empty() && !submit {
        return Err(ActionError::EmptyText);
    }
    let len = text.chars().count();
    if len > MAX_TYPE_CHARS {
        return Err(ActionError::TextTooLong {
            len,
            max: MAX_TYPE_CHARS,
        });
    }
    if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return Err(ActionError::ControlCharacters);
    }
    Ok(())
}

/// The lowercased scheme of `url`, if it has a syntactically valid one
/// (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )` before the first `:`).
pub fn url_scheme(url: &str) -> Option<String> {
    let (scheme, _) = url.split_once(':')?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    Some(scheme.to_ascii_lowercase())
}

/// Checks an `OpenUrl` target.
fn validate_url(url: &str) -> Result<(), ActionError> {
    if url.len() > MAX_URL_LEN {
        return Err(ActionError::MalformedUrl("too long"));
    }
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || is_invisible(c) || c == '\\')
    {
        return Err(ActionError::MalformedUrl(
            "contains whitespace, control characters or backslashes",
        ));
    }
    let scheme = url_scheme(url).ok_or(ActionError::MalformedUrl("missing scheme"))?;
    if !ALLOWED_URL_SCHEMES.contains(&scheme.as_str()) {
        return Err(ActionError::UrlSchemeNotAllowed(scheme));
    }
    let rest = url.get(scheme.len().saturating_add(1)..).unwrap_or("");
    if scheme == "http" || scheme == "https" {
        let after = rest
            .strip_prefix("//")
            .ok_or(ActionError::MalformedUrl("web URL without //host"))?;
        let authority = after.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.contains('@') {
            return Err(ActionError::MalformedUrl("credentials in URL"));
        }
        let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
        if host.is_empty() {
            return Err(ActionError::MalformedUrl("missing host"));
        }
    } else if rest.is_empty() {
        return Err(ActionError::MalformedUrl("nothing after the scheme"));
    }
    Ok(())
}
