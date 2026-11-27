#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
use jarvis_memory::*;

#[test]
fn alias_table() {
    let table = [
        ("email", "email"),
        ("E-mail", "email"),
        ("mail", "email"),
        ("EMAIL", "email"),
        ("phone", "phone"),
        ("mobile", "phone"),
        ("tel", "phone"),
        ("first name", "first_name"),
        ("Given Name", "first_name"),
        ("first_name", "first_name"),
        ("last name", "last_name"),
        ("surname", "last_name"),
        ("family name", "last_name"),
        ("address line 1", "address_line_1"),
        ("city", "city"),
        ("state", "state"),
        ("province", "state"),
        ("State/Province", "state"),
        ("zip", "postal_code"),
        ("postcode", "postal_code"),
        ("postal code", "postal_code"),
        ("country", "country"),
        ("date of birth", "date_of_birth"),
        ("DOB", "date_of_birth"),
        ("birthday", "date_of_birth"),
        ("full name", "full_name"),
    ];
    for (alias, canon) in table {
        let mut p = Profile::new();
        p.set(canon, "v", false);
        assert_eq!(
            p.lookup(alias).map(|f| f.value.as_str()),
            Some("v"),
            "{alias}"
        );
        assert!(p.lookup("nickname").is_none());
    }
}

#[test]
fn set_through_alias_and_sensitive_flag() {
    let mut p = Profile::new();
    p.set("E-mail", "a@b.c", false);
    p.set("dob", "1990-01-01", true);
    p.set("Favourite Colour", "green", false);
    assert!(p.fields.contains_key("email") && p.fields.contains_key("date_of_birth"));
    assert!(p.lookup("birthday").unwrap().sensitive);
    assert_eq!(p.lookup("favourite colour").unwrap().value, "green");
    assert_eq!(p.remove("mail").unwrap().value, "a@b.c");
    assert!(p.lookup("email").is_none());
}

#[test]
fn lookup_finds_non_canonical_stored_keys() {
    let json = r#"{"schema_version":1,"profile":{"fields":{"e-mail":{"value":"x@y.z"}}}}"#;
    let p = MemoryStore::from_json(json).unwrap().profile;
    assert_eq!(p.lookup("email").unwrap().value, "x@y.z");
}

#[test]
fn guess_field_for_form_label() {
    let table = [
        ("Email", Some("email")),
        ("Email address", Some("email")),
        ("Your e-mail", Some("email")),
        ("Phone number", Some("phone")),
        ("Mobile", Some("phone")),
        ("First name", Some("first_name")),
        ("Given name *", Some("first_name")),
        ("Surname", Some("last_name")),
        ("Zip code", Some("postal_code")),
        ("Postcode", Some("postal_code")),
        ("Address line 1", Some("address_line_1")),
        ("Street address", Some("address_line_1")),
        ("State / Province", Some("state")),
        ("Date of birth", Some("date_of_birth")),
        ("Email or phone", None),
        ("Country code", None),
        ("Name", None),
        ("Address", None),
        ("Password", None),
        ("", None),
        ("Search", None),
    ];
    for (label, want) in table {
        assert_eq!(Profile::field_for_element_label(label), want, "{label:?}");
    }
}

/// `Debug` output (logs, panics, `{:?}` in the shell's bridge) never shows a value marked
/// sensitive, for a field, the profile or the whole store; plain values stay readable.
#[test]
fn debug_output_never_shows_sensitive_values() {
    let mut store = jarvis_memory::MemoryStore::new();
    store.profile.set("card_number", "4111111111111111", true);
    store.profile.set("passport", "X1234567", true);
    store.profile.set("email", "alex@example.com", false);
    store.approve_app("com.example.shop");
    let field = format!("{:?}", store.profile.lookup("card_number").unwrap());
    let profile = format!("{:?}", store.profile);
    let whole = format!("{store:?}");
    let pretty = format!("{store:#?}");
    for out in [&field, &profile, &whole, &pretty] {
        assert!(!out.contains("4111111111111111"), "{out}");
        assert!(!out.contains("X1234567"), "{out}");
        assert!(out.contains("<redacted>"), "{out}");
    }
    assert!(profile.contains("alex@example.com"), "{profile}");
    assert!(whole.contains("com.example.shop"), "{whole}");
}
