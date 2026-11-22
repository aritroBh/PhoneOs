//! Redaction positives and negatives.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, el, field};
use jarvis_proto::Role;
use jarvis_safety::{
    contains_secret, redact, redact_element, redact_screen, CARD_MASK, CODE_MASK, KEY_MASK,
    SECRET_MASK,
};

#[test]
fn card_numbers_are_masked() {
    for (input, expected) in [
        ("4111 1111 1111 1111", CARD_MASK.to_owned()),
        ("4111111111111111", CARD_MASK.to_owned()),
        ("4111-1111-1111-1111", CARD_MASK.to_owned()),
        (
            "Card: 5555 5555 5555 4444 exp 12/29",
            format!("Card: {CARD_MASK} exp 12/29"),
        ),
        ("Amex 3782 822463 10005.", format!("Amex {CARD_MASK}.")),
        ("6011111111111117", CARD_MASK.to_owned()),
        (
            "pay with ４１１１１１１１１１１１１１１１",
            format!("pay with {CARD_MASK}"),
        ),
        // A card inside a longer grouped run is still found.
        (
            "Ref 1234 4111 1111 1111 1111",
            format!("Ref 1234 {CARD_MASK}"),
        ),
    ] {
        assert_eq!(redact(input), expected, "{input}");
        assert!(contains_secret(input), "{input}");
    }
}

#[test]
fn luhn_failures_and_short_runs_are_kept() {
    for input in [
        "4111 1111 1111 1112",       // fails Luhn
        "1234567890123",             // 13 digits, fails Luhn
        "Order 112-4417795-2236547", // Amazon order id, 17 digits, fails Luhn
        "Visa •••• 4242",
    ] {
        assert_eq!(redact(input), input, "{input}");
    }
}

#[test]
fn one_time_codes_near_keywords_are_masked() {
    for (input, expected) in [
        (
            "Your verification code is 482913.",
            format!("Your verification code is {CODE_MASK}."),
        ),
        (
            "482913 is your Instagram code",
            format!("{CODE_MASK} is your Instagram code"),
        ),
        (
            "G-582044 is your Google verification code.",
            format!("G-{CODE_MASK} is your Google verification code."),
        ),
        ("Your OTP: 5521", format!("Your OTP: {CODE_MASK}")),
        ("Passcode 12345678", format!("Passcode {CODE_MASK}")),
        (
            "Use code 123 456 to sign in",
            format!("Use code {CODE_MASK} to sign in"),
        ),
        ("Login code: 123-456", format!("Login code: {CODE_MASK}")),
        ("Your PIN is 0420", format!("Your PIN is {CODE_MASK}")),
        (
            "Apple ID Code: 771305. Don't share it",
            format!("Apple ID Code: {CODE_MASK}. Don't share it"),
        ),
    ] {
        assert_eq!(redact(input), expected, "{input}");
    }
}

#[test]
fn ordinary_numbers_are_kept() {
    for input in [
        "Call me at +1 415-555-2671",
        "(415) 555-2671",
        "+44 20 7946 0958",
        "Meeting on 2026-11-22 at 14:30",
        "22/11/2026",
        "Total $1,234.56",
        "Price: €4999",
        "Subtotal 1234.50",
        "Battery 100%",
        "ZIP code 94103",
        "Postal code 10115",
        "Area code 415",
        "Promo code SPRING2026",
        "Flight 1234 departs at 7",
        "Room 4021",
        "Apartment 12B, 1600 Amphitheatre Pkwy",
        "Order #12345 has shipped",
        "Version 18.2.1",
        "Step 3 of 5",
        "Your code is ready",
        "Error code 0x80070005",
    ] {
        assert_eq!(redact(input), input, "{input}");
    }
}

#[test]
fn api_keys_and_tokens_are_masked() {
    for (input, expected) in [
        (
            "key sk-proj-a8F3kLm29QzX7wYp4Rt6Vb1N here",
            format!("key {KEY_MASK} here"),
        ),
        ("ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789", KEY_MASK.to_owned()),
        ("xoxb-123456789012-abcdefghijkl", KEY_MASK.to_owned()),
        ("AKIAIOSFODNN7EXAMPLE", KEY_MASK.to_owned()),
        ("AIzaSyD-9tSrke72PouQMnMX-a7eZSW0jkFMBWY", KEY_MASK.to_owned()),
        (
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            KEY_MASK.to_owned(),
        ),
        (
            "token Zx8Qp2Lr7Vt5Kw3Nb9Hc4Jd6Mf1Gs0Ya",
            format!("token {KEY_MASK}"),
        ),
        (
            "sha 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            format!("sha {KEY_MASK}"),
        ),
    ] {
        assert_eq!(redact(input), expected, "{input}");
    }
}

#[test]
fn labelled_secrets_are_masked() {
    assert_eq!(
        redact("Wi-Fi password: Tr0ub4dor&3"),
        format!("Wi-Fi password: {SECRET_MASK}")
    );
    assert_eq!(redact("api_key=abc123"), format!("api_key={SECRET_MASK}"));
    assert_eq!(
        redact("Authorization: Bearer abc.def"),
        format!("Authorization: Bearer {SECRET_MASK}")
    );
    // Labels without a value are untouched.
    assert_eq!(redact("Forgot password?"), "Forgot password?");
    assert_eq!(redact("Password"), "Password");
}

#[test]
fn prose_urls_and_ids_are_kept() {
    for input in [
        "https://www.example.com/news/2026/Some-Article-Title-With-Words",
        "https://apps.apple.com/us/app/example/id1234567890",
        "550e8400-e29b-41d4-a716-446655440000",
        "Supercalifragilisticexpialidocious",
        "The quick brown fox jumps over the lazy dog.",
        "iPhone15,2",
    ] {
        assert_eq!(redact(input), input, "{input}");
        assert!(!contains_secret(input), "{input}");
    }
}

#[test]
fn masks_are_stable_under_re_redaction() {
    for input in [
        "Your verification code is 482913.",
        "Card 4111 1111 1111 1111 and key ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789",
        "password: hunter2",
    ] {
        let once = redact(input);
        assert_eq!(redact(&once), once);
    }
}

#[test]
fn elements_and_screens_are_redacted() {
    let mut secure = el(1, Role::SecureTextField, "Password");
    secure.secure = true;
    secure.value = Some("hunter2".to_owned());
    assert_eq!(redact_element(&secure).value, None);

    let mut otp = field(2, Role::TextField, "Verification code");
    otp.value = Some("482913".to_owned());
    assert_eq!(redact_element(&otp).value.as_deref(), Some(SECRET_MASK));

    let mut bubble = el(3, Role::StaticText, "Your code is 771305");
    bubble.value = Some("4111 1111 1111 1111".to_owned());
    let r = redact_element(&bubble);
    assert_eq!(r.label, format!("Your code is {CODE_MASK}"));
    assert_eq!(r.value.as_deref(), Some(CARD_MASK));

    let mut email = field(4, Role::TextField, "Email");
    email.value = Some("alex@example.com".to_owned());
    let s = app_screen(vec![secure, otp, bubble, email.clone()]);
    let rs = redact_screen(&s);
    assert_eq!(rs.elements.len(), 4);
    assert_eq!(rs.elements[3], email);
    assert_eq!(rs.app_bundle_id, s.app_bundle_id);
}
