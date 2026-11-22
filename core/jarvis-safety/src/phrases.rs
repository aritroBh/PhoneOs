//! The phrase tables behind risk classification, and the matcher that applies them.
//!
//! Each [`Phrase`] is a short English word sequence. A phrase matches a label when its
//! tokens appear contiguously among the label's tokens (see `text::tokens`), which makes
//! matching case-insensitive and word-bounded: `send` matches "Send" and "Send Message" but
//! not "Sender" or "Resend".
//!
//! When matches overlap, a longer match swallows any shorter one inside it. That is how
//! exemptions work: `resend code` (category `None`) swallows the `resend` match inside
//! "Resend code", while "Resend" alone still matches `resend`. An exemption only covers its
//! own words, so "Pay · Resend code" is still a payment.
//!
//! To extend coverage, add rows. Prefer adding a specific exemption over removing a broad
//! risky phrase.

use jarvis_proto::RiskCategory;
use jarvis_proto::RiskCategory::{
    AccountChange, Credential, Delete, Payment, PublicPost, Purchase, SendMessage, SystemSetting,
};

use crate::text::tokens;

/// One row of a phrase table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Phrase {
    /// Space-separated lowercase words, tokenized like labels.
    pub text: &'static str,
    /// Category the phrase signals; `None` marks an exemption.
    pub category: RiskCategory,
    /// Whether the phrase is a generic confirmation ("OK", "Continue") whose real meaning
    /// comes from the rest of the screen.
    pub inherits_context: bool,
}

/// A phrase that signals `category` on its own.
const fn p(text: &'static str, category: RiskCategory) -> Phrase {
    Phrase {
        text,
        category,
        inherits_context: false,
    }
}

/// A generic confirmation: signals `category` and also takes on the screen's context risk.
const fn g(text: &'static str, category: RiskCategory) -> Phrase {
    Phrase {
        text,
        category,
        inherits_context: true,
    }
}

/// An exemption: swallows shorter risky matches inside it.
const fn x(text: &'static str) -> Phrase {
    p(text, RiskCategory::None)
}

/// Phrases matched against the label of the element being acted on, and against every
/// other element's text when building screen context.
pub const ACTION_PHRASES: &[Phrase] = &[
    // --- Sending ---
    p("send", SendMessage),
    p("send message", SendMessage),
    p("send now", SendMessage),
    p("send email", SendMessage),
    p("send text", SendMessage),
    p("send reply", SendMessage),
    p("send invite", SendMessage),
    p("send invitation", SendMessage),
    p("send request", SendMessage),
    p("send feedback", SendMessage),
    p("resend", SendMessage),
    p("reply", SendMessage),
    p("reply all", SendMessage),
    p("invite", SendMessage),
    p("request", SendMessage),
    p("report", SendMessage),
    p("report spam", SendMessage),
    p("report junk", SendMessage),
    p("call", SendMessage),
    p("call back", SendMessage),
    p("dial", SendMessage),
    p("facetime", SendMessage),
    p("video call", SendMessage),
    p("voice call", SendMessage),
    p("start call", SendMessage),
    p("join call", SendMessage),
    x("send code"),
    x("send a code"),
    x("send me a code"),
    x("send new code"),
    x("send verification code"),
    x("resend code"),
    x("resend a code"),
    x("resend verification code"),
    x("resend link"),
    x("text me a code"),
    x("call history"),
    x("missed call"),
    x("missed calls"),
    x("recent calls"),
    x("call log"),
    // --- Public posting ---
    p("post", PublicPost),
    p("post now", PublicPost),
    p("post comment", PublicPost),
    p("publish", PublicPost),
    p("tweet", PublicPost),
    p("retweet", PublicPost),
    p("repost", PublicPost),
    p("quote", PublicPost),
    p("share", PublicPost),
    p("share now", PublicPost),
    p("share post", PublicPost),
    p("upload", PublicPost),
    p("go live", PublicPost),
    p("comment", PublicPost),
    p("add to story", PublicPost),
    p("share to story", PublicPost),
    p("make public", PublicPost),
    // --- Payment (moving money) ---
    p("pay", Payment),
    p("pay now", Payment),
    p("pay with", Payment),
    p("apple pay", Payment),
    p("gpay", Payment),
    p("google pay", Payment),
    p("paypal", Payment),
    p("zelle", Payment),
    p("venmo", Payment),
    p("send money", Payment),
    p("send payment", Payment),
    p("request money", Payment),
    p("transfer", Payment),
    p("transfer money", Payment),
    p("transfer funds", Payment),
    p("wire", Payment),
    p("donate", Payment),
    p("donate now", Payment),
    p("tip", Payment),
    p("add tip", Payment),
    p("withdraw", Payment),
    p("cash out", Payment),
    p("top up", Payment),
    p("add money", Payment),
    p("add funds", Payment),
    p("make payment", Payment),
    p("make a payment", Payment),
    p("confirm payment", Payment),
    p("authorize", Payment),
    p("authorize payment", Payment),
    p("approve payment", Payment),
    p("double click to pay", Payment),
    p("slide to pay", Payment),
    p("paynow", Payment),
    // --- Purchase ---
    p("buy", Purchase),
    p("buy now", Purchase),
    p("buynow", Purchase),
    p("purchase", Purchase),
    p("confirm purchase", Purchase),
    p("complete purchase", Purchase),
    p("place order", Purchase),
    p("place your order", Purchase),
    p("order now", Purchase),
    p("submit order", Purchase),
    p("complete order", Purchase),
    p("confirm order", Purchase),
    p("checkout", Purchase),
    p("check out", Purchase),
    p("proceed to checkout", Purchase),
    p("subscribe", Purchase),
    p("subscribe now", Purchase),
    p("start subscription", Purchase),
    p("start free trial", Purchase),
    p("start trial", Purchase),
    p("try free", Purchase),
    p("try it free", Purchase),
    p("free trial", Purchase),
    p("upgrade", Purchase),
    p("upgrade now", Purchase),
    p("go premium", Purchase),
    p("get premium", Purchase),
    p("unlock premium", Purchase),
    p("remove ads", Purchase),
    p("rent", Purchase),
    p("pre order", Purchase),
    p("preorder", Purchase),
    p("renew", Purchase),
    p("place bid", Purchase),
    p("make offer", Purchase),
    p("book now", Purchase),
    p("book it", Purchase),
    p("book this", Purchase),
    p("book ride", Purchase),
    p("book trip", Purchase),
    p("book flight", Purchase),
    p("book hotel", Purchase),
    p("book table", Purchase),
    p("book appointment", Purchase),
    p("reserve", Purchase),
    p("request ride", Purchase),
    p("confirm ride", Purchase),
    p("confirm booking", Purchase),
    p("in app purchase", Purchase),
    x("purchase history"),
    // --- Deletion ---
    p("delete", Delete),
    p("delete all", Delete),
    p("delete forever", Delete),
    p("delete account", Delete),
    p("delete app", Delete),
    p("remove", Delete),
    p("remove all", Delete),
    p("erase", Delete),
    p("erase all content and settings", Delete),
    p("clear all", Delete),
    p("clear history", Delete),
    p("clear data", Delete),
    p("clear website data", Delete),
    p("clear browsing data", Delete),
    p("clear cache", Delete),
    p("clear chat", Delete),
    p("clear conversation", Delete),
    p("reset", Delete),
    p("reset all settings", Delete),
    p("factory reset", Delete),
    p("unsubscribe", Delete),
    p("trash", Delete),
    p("move to trash", Delete),
    p("empty trash", Delete),
    p("discard", Delete),
    p("wipe", Delete),
    p("forget", Delete),
    p("forget this device", Delete),
    p("forget network", Delete),
    p("unpair", Delete),
    p("uninstall", Delete),
    p("offload app", Delete),
    p("destroy", Delete),
    p("revoke", Delete),
    p("unfriend", Delete),
    p("unfollow", Delete),
    p("deleteall", Delete),
    x("clear search"),
    x("clear text"),
    // --- Account changes ---
    p("change password", AccountChange),
    p("update password", AccountChange),
    p("reset password", AccountChange),
    p("set password", AccountChange),
    p("change passcode", AccountChange),
    p("turn passcode off", AccountChange),
    p("change email", AccountChange),
    p("update email", AccountChange),
    p("change phone number", AccountChange),
    p("change username", AccountChange),
    p("sign out", AccountChange),
    p("signout", AccountChange),
    p("sign off", AccountChange),
    p("log out", AccountChange),
    p("logout", AccountChange),
    p("log off", AccountChange),
    p("deactivate", AccountChange),
    p("close account", AccountChange),
    p("cancel subscription", AccountChange),
    p("cancel membership", AccountChange),
    p("cancel plan", AccountChange),
    p("transfer ownership", AccountChange),
    p("block", AccountChange),
    p("block contact", AccountChange),
    p("block this caller", AccountChange),
    p("leave group", AccountChange),
    p("leave conversation", AccountChange),
    p("make admin", AccountChange),
    p("turn off two factor", AccountChange),
    p("two factor", AccountChange),
    // --- Device settings ---
    p("turn off", SystemSetting),
    p("turn on", SystemSetting),
    p("enable", SystemSetting),
    p("disable", SystemSetting),
    g("allow", SystemSetting),
    p("always allow", SystemSetting),
    p("allow once", SystemSetting),
    p("allow full access", SystemSetting),
    p("trust", SystemSetting),
    p("trust this computer", SystemSetting),
    p("install profile", SystemSetting),
    p("install now", SystemSetting),
    p("download and install", SystemSetting),
    p("update now", SystemSetting),
    p("restart", SystemSetting),
    p("shut down", SystemSetting),
    p("power off", SystemSetting),
    p("slide to power off", SystemSetting),
    x("dont allow"),
    x("do not allow"),
    // --- Credentials ---
    p("show password", Credential),
    p("reveal password", Credential),
    p("view password", Credential),
    p("copy password", Credential),
    p("share password", Credential),
    p("export passwords", Credential),
    p("show passwords", Credential),
    p("show pin", Credential),
    p("show code", Credential),
    p("copy code", Credential),
    p("reveal", Credential),
    p("show secret", Credential),
    p("recovery key", Credential),
    p("recovery phrase", Credential),
    p("seed phrase", Credential),
    // --- Generic confirmations: meaning comes from the screen ---
    g("confirm", RiskCategory::None),
    g("continue", RiskCategory::None),
    g("ok", RiskCategory::None),
    g("okay", RiskCategory::None),
    g("done", RiskCategory::None),
    g("yes", RiskCategory::None),
    g("next", RiskCategory::None),
    g("proceed", RiskCategory::None),
    g("agree", RiskCategory::None),
    g("accept", RiskCategory::None),
    g("approve", RiskCategory::None),
    g("complete", RiskCategory::None),
    g("finish", RiskCategory::None),
    g("go", RiskCategory::None),
    g("got it", RiskCategory::None),
    g("sure", RiskCategory::None),
    g("im sure", RiskCategory::None),
    g("i understand", RiskCategory::None),
    g("submit", SendMessage),
    g("save", RiskCategory::None),
    g("apply", RiskCategory::None),
    g("update", RiskCategory::None),
    g("join", RiskCategory::None),
];

/// Phrases that only count as *context* (text elsewhere on the screen), never as the label
/// of the tapped element: "Recently Deleted" is navigation, but "will be deleted" in an
/// alert explains what its "OK" button does.
pub const CONTEXT_PHRASES: &[Phrase] = &[
    p("total", Payment),
    p("subtotal", Payment),
    p("order total", Payment),
    p("amount due", Payment),
    p("payment", Payment),
    p("payment method", Payment),
    p("card ending", Payment),
    p("card number", Payment),
    p("billing", Payment),
    p("charged", Payment),
    p("will be charged", Payment),
    p("visa", Payment),
    p("mastercard", Payment),
    p("amex", Payment),
    p("american express", Payment),
    p("per month", Purchase),
    p("per year", Purchase),
    p("monthly", Purchase),
    p("annually", Purchase),
    p("subscription", Purchase),
    p("order summary", Purchase),
    p("your order", Purchase),
    p("cart", Purchase),
    p("shipping", Purchase),
    p("deleted", Delete),
    p("removed", Delete),
    p("erased", Delete),
    p("permanently", Delete),
    p("cannot be undone", Delete),
    p("can not be undone", Delete),
    p("cant be undone", Delete),
    p("irreversible", Delete),
    p("will be lost", Delete),
    p("will be sent", SendMessage),
    p("new password", AccountChange),
    p("current password", AccountChange),
    p("confirm password", AccountChange),
];

/// Text-input labels that make typing into the field a credential entry.
pub const CREDENTIAL_FIELD_PHRASES: &[Phrase] = &[
    p("password", Credential),
    p("passcode", Credential),
    p("passphrase", Credential),
    p("pin", Credential),
    p("cvv", Credential),
    p("cvc", Credential),
    p("security code", Credential),
    p("verification code", Credential),
    p("one time code", Credential),
    p("one time password", Credential),
    p("otp", Credential),
    p("2 fa", Credential),
    p("authentication code", Credential),
    p("auth code", Credential),
    p("login code", Credential),
    p("card number", Credential),
    p("credit card", Credential),
    p("debit card", Credential),
    p("expiration date", Credential),
    p("ssn", Credential),
    p("social security", Credential),
    p("secret", Credential),
    p("api key", Credential),
    p("token", Credential),
    p("recovery key", Credential),
    p("seed phrase", Credential),
    p("recovery phrase", Credential),
    x("pin code"),
];

/// Text-input labels whose Return key publishes or sends what was typed.
pub const COMPOSE_FIELD_PHRASES: &[Phrase] = &[
    p("message", SendMessage),
    p("imessage", SendMessage),
    p("text message", SendMessage),
    p("reply", SendMessage),
    p("write a reply", SendMessage),
    p("email", SendMessage),
    p("chat", SendMessage),
    p("comment", PublicPost),
    p("add a comment", PublicPost),
    p("tweet", PublicPost),
    p("post", PublicPost),
    p("whats happening", PublicPost),
    p("whats on your mind", PublicPost),
    p("caption", PublicPost),
    p("write a caption", PublicPost),
    p("review", PublicPost),
    x("search messages"),
    x("email address"),
];

/// Text that marks a screen as an account or settings screen.
pub const SETTINGS_CONTEXT_PHRASES: &[Phrase] = &[
    p("settings", AccountChange),
    p("account", AccountChange),
    p("profile", AccountChange),
    p("preferences", AccountChange),
    p("privacy", AccountChange),
    p("security", AccountChange),
    p("password", AccountChange),
    p("personal information", AccountChange),
];

/// Labels that commit changes when tapped on a settings screen.
pub const SETTINGS_ACTION_PHRASES: &[Phrase] = &[
    p("save", AccountChange),
    p("save changes", AccountChange),
    p("update", AccountChange),
    p("apply", AccountChange),
    p("change", AccountChange),
    p("submit", AccountChange),
    p("confirm", AccountChange),
];

/// Bundle id of the App Store, where "Get" and "Install" start a (possibly paid) download.
pub const APP_STORE_BUNDLE_ID: &str = "com.apple.AppStore";

/// Bundle id of the Settings app, where switches and "Save" change the device.
pub const SETTINGS_BUNDLE_ID: &str = "com.apple.Preferences";

/// Extra phrases that apply only inside one app.
pub const APP_PHRASES: &[(&str, &[Phrase])] = &[(
    APP_STORE_BUNDLE_ID,
    &[
        p("get", Purchase),
        p("install", Purchase),
        p("redownload", Purchase),
    ],
)];

/// Highest-risk-first ordering used to combine several signals into one category.
pub(crate) fn severity(c: RiskCategory) -> u8 {
    match c {
        RiskCategory::None => 0,
        SystemSetting => 1,
        SendMessage => 2,
        PublicPost => 3,
        AccountChange => 4,
        Credential => 5,
        Delete => 6,
        Purchase => 7,
        Payment => 8,
    }
}

/// The riskier of two categories.
pub(crate) fn max_risk(a: RiskCategory, b: RiskCategory) -> RiskCategory {
    if severity(b) > severity(a) {
        b
    } else {
        a
    }
}

/// What a label says once the phrase tables are applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LabelRisk {
    /// The riskiest category among the surviving matches.
    pub category: RiskCategory,
    /// Whether a surviving match is a generic confirmation.
    pub generic: bool,
}

/// Applies `tables` to the label tokens `toks`.
pub(crate) fn match_tokens(toks: &[String], tables: &[&[Phrase]]) -> LabelRisk {
    // (start, end, phrase) for every raw match.
    let mut found: Vec<(usize, usize, &Phrase)> = Vec::new();
    for table in tables {
        for phrase in table.iter() {
            let words = tokens(phrase.text);
            let n = words.len();
            if n == 0 || n > toks.len() {
                continue;
            }
            for (start, window) in toks.windows(n).enumerate() {
                if window == words.as_slice() {
                    found.push((start, start.saturating_add(n), phrase));
                }
            }
        }
    }
    let mut out = LabelRisk::default();
    for &(s, e, phrase) in &found {
        let len = e.saturating_sub(s);
        let swallowed = found
            .iter()
            .any(|&(s2, e2, _)| s2 <= s && e <= e2 && e2.saturating_sub(s2) > len);
        if swallowed {
            continue;
        }
        out.category = max_risk(out.category, phrase.category);
        out.generic |= phrase.inherits_context;
    }
    out
}

/// Applies `tables` to `label`.
pub(crate) fn match_label(label: &str, tables: &[&[Phrase]]) -> LabelRisk {
    match_tokens(&tokens(label), tables)
}

/// The phrase tables that apply to the tapped element inside `bundle_id`.
pub(crate) fn action_tables(bundle_id: &str) -> Vec<&'static [Phrase]> {
    let mut tables = vec![ACTION_PHRASES];
    for (app, phrases) in APP_PHRASES {
        if app.eq_ignore_ascii_case(bundle_id) {
            tables.push(phrases);
        }
    }
    tables
}

/// The category the action phrase table assigns to `label` on its own, without screen
/// context or app-specific rows. Useful for testing and extending the table.
pub fn label_risk(label: &str) -> RiskCategory {
    match_label(label, &[ACTION_PHRASES]).category
}
