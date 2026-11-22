//! The user profile used for form filling, with alias lookup.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Canonical key and the normalised aliases that resolve to it.
const ALIASES: &[(&str, &[&str])] = &[
    (
        "email",
        &["email", "e mail", "mail", "email address", "e mail address"],
    ),
    (
        "phone",
        &[
            "phone",
            "mobile",
            "tel",
            "telephone",
            "cell",
            "mobile phone",
            "cell phone",
            "phone number",
            "mobile number",
        ],
    ),
    (
        "first_name",
        &["first name", "given name", "forename", "firstname"],
    ),
    (
        "last_name",
        &["last name", "surname", "family name", "lastname"],
    ),
    ("full_name", &["full name"]),
    (
        "address_line_1",
        &[
            "address line 1",
            "address line one",
            "address 1",
            "address1",
            "street address",
            "street",
        ],
    ),
    ("city", &["city", "town"]),
    ("state", &["state", "province", "state province"]),
    (
        "postal_code",
        &[
            "zip",
            "zipcode",
            "zip code",
            "postcode",
            "post code",
            "postal code",
        ],
    ),
    ("country", &["country", "country region"]),
    (
        "date_of_birth",
        &[
            "date of birth",
            "dob",
            "birthday",
            "birth date",
            "birthdate",
        ],
    ),
];

/// Words that decorate a form label without changing what it asks for.
const NOISE: &[&str] = &[
    "your", "enter", "the", "a", "required", "optional", "please", "my", "number", "address",
];

fn normalise(s: &str) -> String {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .flat_map(|t| t.chars().flat_map(char::to_lowercase).chain(Some(' ')))
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn canonical_of_normalised(n: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(canon, aliases)| *canon == n.replace(' ', "_") || aliases.contains(&n))
        .map(|(canon, _)| *canon)
}

/// Resolves a field name or alias ("e-mail", "Mobile", "Zip code") to its canonical key.
pub fn canonical_key(name: &str) -> Option<&'static str> {
    canonical_of_normalised(&normalise(name))
}

/// One stored profile value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileField {
    /// The value to type.
    pub value: String,
    /// True for values that must not appear in logs or be typed without care.
    #[serde(default)]
    pub sensitive: bool,
}

/// User profile fields keyed by canonical name (`email`, `phone`, `first_name`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Profile {
    /// Fields by key, sorted.
    #[serde(default)]
    pub fields: BTreeMap<String, ProfileField>,
}

impl Profile {
    /// An empty profile.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a field under its canonical key (or its normalised name when it has none).
    pub fn set(&mut self, name: &str, value: impl Into<String>, sensitive: bool) {
        let key = storage_key(name);
        if key.is_empty() {
            return;
        }
        self.fields.insert(
            key,
            ProfileField {
                value: value.into(),
                sensitive,
            },
        );
    }

    /// Removes a field by name or alias.
    pub fn remove(&mut self, name: &str) -> Option<ProfileField> {
        let key = self.find_key(name)?;
        self.fields.remove(&key)
    }

    /// Looks a field up by name or alias, case- and punctuation-insensitively.
    pub fn lookup(&self, name: &str) -> Option<&ProfileField> {
        let key = self.find_key(name)?;
        self.fields.get(&key)
    }

    fn find_key(&self, name: &str) -> Option<String> {
        let direct = storage_key(name);
        if self.fields.contains_key(&direct) {
            return Some(direct);
        }
        let canon = canonical_key(name)?;
        self.fields
            .keys()
            .find(|k| canonical_key(k) == Some(canon))
            .cloned()
    }

    /// Guesses which canonical profile field a form field labelled `label` wants.
    ///
    /// Strict on purpose: the whole label (minus decoration such as "your", "number",
    /// "address") must be a known alias. "Email or phone", "Country code", "Name" and
    /// "Address" return `None`.
    pub fn field_for_element_label(label: &str) -> Option<&'static str> {
        let n = normalise(label);
        if let Some(c) = canonical_of_normalised(&n) {
            return Some(c);
        }
        let stripped: Vec<&str> = n.split(' ').filter(|t| !NOISE.contains(t)).collect();
        if stripped.is_empty() {
            return None;
        }
        canonical_of_normalised(&stripped.join(" "))
    }
}

fn storage_key(name: &str) -> String {
    let n = normalise(name);
    canonical_of_normalised(&n).map_or_else(|| n.replace(' ', "_"), str::to_owned)
}
