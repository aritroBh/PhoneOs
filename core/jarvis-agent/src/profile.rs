//! Profile placeholders: the model writes `{{profile.email}}`, the session types the value.
//!
//! Values never enter a prompt: they are substituted only when the runner command is built,
//! and any value that shows up on a later screen (the field now holds the email) is masked
//! back to its placeholder before the screen is rendered. History, flows and the task log
//! keep the placeholder.

use jarvis_memory::Profile;

/// Opening of a placeholder.
const OPEN: &str = "{{";
/// Closing of a placeholder.
const CLOSE: &str = "}}";
/// Prefix inside the braces.
const PREFIX: &str = "profile.";
/// Profile values shorter than this are not masked on screen (too likely to match noise).
const MIN_MASK_CHARS: usize = 3;

/// The canonical placeholder for a profile key.
pub(crate) fn placeholder(key: &str) -> String {
    format!("{OPEN}{PREFIX}{key}{CLOSE}")
}

/// Why substitution failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileError {
    /// `{{profile.KEY}}` names no stored field.
    UnknownKey(String),
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::UnknownKey(k) => {
                let k: String = k.chars().take(40).collect();
                write!(
                    f,
                    "no profile field \"{k}\"; use one of the listed profile keys"
                )
            }
        }
    }
}

/// True when `text` holds at least one `{{profile.…}}` placeholder.
#[cfg(test)]
pub(crate) fn has_placeholder(text: &str) -> bool {
    find_next(text, 0).is_some()
}

/// Finds the next placeholder at or after byte `from`: (start, end, key).
fn find_next(text: &str, from: usize) -> Option<(usize, usize, String)> {
    let mut search = from;
    loop {
        let rest = text.get(search..)?;
        let open = search + rest.find(OPEN)?;
        let after_open = open + OPEN.len();
        let inner_rest = text.get(after_open..)?;
        let close_rel = inner_rest.find(CLOSE)?;
        let inner = inner_rest.get(..close_rel)?.trim();
        let end = after_open + close_rel + CLOSE.len();
        let has_prefix = inner
            .get(..PREFIX.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(PREFIX));
        if has_prefix && !inner.contains(OPEN) {
            let key = inner.get(PREFIX.len()..).unwrap_or_default().trim();
            if !key.is_empty() {
                return Some((open, end, key.to_owned()));
            }
        }
        search = after_open;
    }
}

/// Replaces every placeholder with its profile value (lookup accepts aliases).
pub(crate) fn substitute(text: &str, profile: &Profile) -> Result<String, ProfileError> {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0usize;
    while let Some((start, end, key)) = find_next(text, pos) {
        let field = profile
            .lookup(&key)
            .ok_or_else(|| ProfileError::UnknownKey(key.clone()))?;
        out.push_str(text.get(pos..start).unwrap_or_default());
        out.push_str(&field.value);
        pos = end;
    }
    out.push_str(text.get(pos..).unwrap_or_default());
    Ok(out)
}

/// Replaces every stored profile value found in `text` with its placeholder, longest value
/// first so a value that contains another is masked whole.
pub(crate) fn mask(text: &str, profile: &Profile) -> String {
    let mut values: Vec<(&str, &str)> = profile
        .fields
        .iter()
        .map(|(k, f)| (k.as_str(), f.value.trim()))
        .filter(|(_, v)| v.chars().count() >= MIN_MASK_CHARS)
        .collect();
    values.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    let mut out = text.to_owned();
    for (key, value) in values {
        if out.contains(value) {
            out = out.replace(value, &placeholder(key));
        }
    }
    out
}

/// The profile's keys (never values), sorted, sanitised for the prompt.
pub(crate) fn sanitize_keys(profile: &Profile) -> Vec<String> {
    profile
        .fields
        .keys()
        .map(|k| crate::history::clean(k, 40))
        .filter(|k| !k.is_empty())
        .collect()
}

/// Texts the recorder must treat as sensitive: the values of sensitive fields and their
/// canonical placeholders.
pub(crate) fn sensitive_texts(profile: &Profile) -> Vec<String> {
    profile
        .fields
        .iter()
        .filter(|(_, f)| f.sensitive)
        .flat_map(|(k, f)| [f.value.clone(), placeholder(k)])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        let mut p = Profile::new();
        p.set("email", "alex@example.com", false);
        p.set("phone", "+1 555 0100", true);
        p
    }

    #[test]
    fn substitutes_with_aliases_and_spacing() {
        let p = profile();
        assert_eq!(
            substitute("{{profile.email}}", &p).unwrap(),
            "alex@example.com"
        );
        assert_eq!(
            substitute("mail: {{ profile.E-mail }} / {{profile.mobile}}!", &p).unwrap(),
            "mail: alex@example.com / +1 555 0100!"
        );
        assert_eq!(substitute("no braces {x}", &p).unwrap(), "no braces {x}");
        assert_eq!(substitute("{{other}}", &p).unwrap(), "{{other}}");
        assert_eq!(
            substitute("{{profile.passport}}", &p),
            Err(ProfileError::UnknownKey("passport".into()))
        );
    }

    #[test]
    fn unterminated_and_odd_input_is_left_alone() {
        let p = profile();
        for s in [
            "{{profile.email",
            "{{",
            "}}{{",
            "{{{{profile.email}}",
            "é{{profile.é}}",
        ] {
            let _ = substitute(s, &p);
        }
        assert!(!has_placeholder("{{profile.}}"));
        assert!(has_placeholder("x {{profile.email}} y"));
    }

    #[test]
    fn masks_values_back_to_placeholders() {
        let p = profile();
        assert_eq!(
            mask("Signed in as alex@example.com", &p),
            "Signed in as {{profile.email}}"
        );
        assert_eq!(
            sensitive_texts(&p),
            vec!["+1 555 0100".to_owned(), "{{profile.phone}}".to_owned()]
        );
    }
}
