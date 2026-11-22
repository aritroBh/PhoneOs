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
    AccountChange, Call, Credential, Delete, Payment, PublicPost, Purchase, SendMessage,
    SystemSetting,
};

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::text::{bounded, tokens};

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
    // Eighth review: more send synonyms and icon names.
    p("forward", SendMessage),
    p("respond", SendMessage),
    p("up arrow", SendMessage),
    p("invite", SendMessage),
    p("request", SendMessage),
    p("report", SendMessage),
    p("report spam", SendMessage),
    p("report junk", SendMessage),
    p("call", Call),
    p("call back", Call),
    p("dial", Call),
    p("facetime", Call),
    p("face time", Call),
    p("video call", Call),
    p("voice call", Call),
    p("start call", Call),
    p("join call", Call),
    p("answer", Call),
    p("accept call", Call),
    p("decline call", Call),
    p("accept invitation", SendMessage),
    p("accept invite", SendMessage),
    p("decline invitation", SendMessage),
    p("decline invite", SendMessage),
    p("decline", SendMessage),
    p("airdrop", SendMessage),
    p("air drop", SendMessage),
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
    p("your story", PublicPost),
    p("share story", PublicPost),
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
    p("dollarsign", Payment),
    p("creditcard", Purchase),
    p("paperplane", SendMessage),
    p("sell", Payment),
    p("sell now", Payment),
    p("swap", Payment),
    p("convert", Payment),
    p("trade", Payment),
    p("stake", Payment),
    p("add cash", Payment),
    p("give", Payment),
    x("give feedback"),
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
    p("bid", Purchase),
    p("book", Purchase),
    p("order", Purchase),
    x("address book"),
    x("phone book"),
    x("order status"),
    x("order history"),
    x("order details"),
    x("order number"),
    x("order summary"),
    x("track order"),
    x("sort order"),
    x("in order"),
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
    p("clear history and website data", Delete),
    p("delete all data", Delete),
    p("remove all data", Delete),
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
    p("unsend", Delete),
    p("restore", Delete),
    p("dont save", Delete),
    p("do not save", Delete),
    p("replace", Delete),
    p("overwrite", Delete),
    x("restore purchases"),
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
    p("sign out of icloud", AccountChange),
    p("sign out of all devices", AccountChange),
    p("sign out of all sessions", AccountChange),
    p("log out", AccountChange),
    p("logout", AccountChange),
    p("log off", AccountChange),
    p("deactivate", AccountChange),
    p("close account", AccountChange),
    p("cancel subscription", AccountChange),
    p("cancel membership", AccountChange),
    p("cancel plan", AccountChange),
    p("cancel order", AccountChange),
    p("cancel booking", AccountChange),
    p("cancel reservation", AccountChange),
    p("transfer ownership", AccountChange),
    p("block", AccountChange),
    p("block contact", AccountChange),
    p("block this caller", AccountChange),
    p("leave group", AccountChange),
    p("leave conversation", AccountChange),
    p("make admin", AccountChange),
    p("leave", AccountChange),
    p("leave server", AccountChange),
    p("leave channel", AccountChange),
    p("exit group", AccountChange),
    p("unmatch", AccountChange),
    p("unlink", AccountChange),
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
    p("disconnect", SystemSetting),
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
    g("open", RiskCategory::None),
    g("allow access", SystemSetting),
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
    p("invitation", SendMessage),
    p("invited you", SendMessage),
    p("incoming call", SendMessage),
    p("in app purchases", Purchase),
    p("in app purchase", Purchase),
    p("open this page in", SystemSetting),
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
];

/// Field names (label or placeholder) whose value is never shown, in prompts or in the
/// owner's summaries, beyond [`CREDENTIAL_FIELD_PHRASES`]: card expiry and bank account
/// entry fields, and (sixth review) card and identity details: CSC, security number, card
/// number written short, security answers, maiden name, memorable word, date of birth, tax
/// id, driver license, passport, access code. Not used for classification (typing into them is not a credential).
pub const HIDDEN_FIELD_PHRASES: &[Phrase] = &[
    p("expiry", Credential),
    p("expiry date", Credential),
    p("expiration", Credential),
    p("exp date", Credential),
    p("valid thru", Credential),
    p("one time", Credential),
    p("pin code", Credential),
    p("iban", Credential),
    p("routing number", Credential),
    p("routing", Credential),
    p("account number", Credential),
    p("bank account", Credential),
    p("sort code", Credential),
    p("bic", Credential),
    p("swift code", Credential),
    p("security answer", Credential),
    p("mnemonic", Credential),
    // Sixth review: identity and card details whose values were quoted in summaries.
    p("csc", Credential),
    p("cvv2", Credential),
    p("cvc2", Credential),
    p("security number", Credential),
    p("card no", Credential),
    p("card", Credential),
    p("answer", Credential),
    p("maiden name", Credential),
    p("memorable word", Credential),
    p("memorable", Credential),
    p("date of birth", Credential),
    p("birth date", Credential),
    p("birthdate", Credential),
    p("dob", Credential),
    p("tax id", Credential),
    p("taxpayer", Credential),
    p("tin", Credential),
    p("driver license", Credential),
    p("driver licence", Credential),
    p("drivers license", Credential),
    p("drivers licence", Credential),
    p("license number", Credential),
    p("licence number", Credential),
    p("passport", Credential),
    p("national id", Credential),
    p("id number", Credential),
    p("access code", Credential),
];

/// Text-input labels whose Return key publishes or sends what was typed.
pub const COMPOSE_FIELD_PHRASES: &[Phrase] = &[
    p("message", SendMessage),
    p("imessage", SendMessage),
    p("i message", SendMessage),
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
    p("aa", SendMessage),
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

/// Labels that only navigate or back out; tapping them is never boosted to `Payment` on a
/// checkout screen.
pub const NAVIGATION_PHRASES: &[Phrase] = &[
    p("cancel", RiskCategory::None),
    p("back", RiskCategory::None),
    p("close", RiskCategory::None),
    p("not now", RiskCategory::None),
    p("no thanks", RiskCategory::None),
    p("dismiss", RiskCategory::None),
    p("edit", RiskCategory::None),
    p("change", RiskCategory::None),
    p("learn more", RiskCategory::None),
    p("help", RiskCategory::None),
    p("details", RiskCategory::None),
    p("more info", RiskCategory::None),
    p("terms", RiskCategory::None),
    p("privacy policy", RiskCategory::None),
    p("menu", RiskCategory::None),
    p("search", RiskCategory::None),
    p("home", RiskCategory::None),
];

/// Bundle id of the App Store, where "Get" and "Install" start a (possibly paid) download.
pub const APP_STORE_BUNDLE_ID: &str = "com.apple.AppStore";

/// Bundle id of the Settings app, where switches and "Save" change the device
/// (re-exported from `jarvis-proto`).
pub use jarvis_proto::SETTINGS_BUNDLE_ID;

/// Extra phrases that apply only inside apps whose bundle id starts with the given prefix
/// (case-insensitive).
pub const APP_PHRASES: &[(&str, &[Phrase])] = &[
    (
        APP_STORE_BUNDLE_ID,
        &[
            p("get", Purchase),
            p("install", Purchase),
            p("redownload", Purchase),
        ],
    ),
    (
        // Before iOS 18 saved passwords live inside Settings, not the Passwords app.
        SETTINGS_BUNDLE_ID,
        &[
            p("passwords", Credential),
            p("passwords and accounts", Credential),
            p("face id and passcode", AccountChange),
            p("face id passcode", AccountChange),
            p("touch id and passcode", AccountChange),
            p("touch id passcode", AccountChange),
            p("passcode", AccountChange),
            p("install", SystemSetting),
            p("install profile", SystemSetting),
        ],
    ),
    (
        "com.apple.mobilecal",
        &[
            p("accept", SendMessage),
            p("maybe", SendMessage),
            p("decline", SendMessage),
        ],
    ),
    (
        "com.apple.InCallService",
        &[p("accept", Call), p("slide to answer", Call)],
    ),
];

/// Verbs that commit to a purchase when the screen shows a price ("Book" next to
/// "$189 night") or in-app-purchase wording, even without "Total".
pub const PRICE_VERB_PHRASES: &[Phrase] = &[
    p("get", Purchase),
    p("book", Purchase),
    p("order", Purchase),
    p("reserve", Purchase),
    p("rent", Purchase),
    p("install", Purchase),
    p("download", Purchase),
    p("request", Purchase),
    p("join", Purchase),
    p("start", Purchase),
    p("unlock", Purchase),
];

/// Share-sheet actions that stay on the device; any other row on a share sheet (a person,
/// an AirDrop device, an app) sends.
pub const SHARE_SAFE_PHRASES: &[Phrase] = &[
    p("copy", RiskCategory::None),
    p("save image", RiskCategory::None),
    p("save video", RiskCategory::None),
    p("save to files", RiskCategory::None),
    p("add to photos", RiskCategory::None),
    p("print", RiskCategory::None),
    p("options", RiskCategory::None),
    p("edit actions", RiskCategory::None),
    p("add to reading list", RiskCategory::None),
    p("add bookmark", RiskCategory::None),
    p("find on page", RiskCategory::None),
    p("markup", RiskCategory::None),
    p("airdrop", RiskCategory::None),
];

/// SF Symbol names (as some apps leave them as accessibility labels) and what they mean.
/// A label matches an entry when it equals it or continues it with `.` (`trash.fill`);
/// the longest matching entry wins.
pub const SF_SYMBOLS: &[(&str, RiskCategory)] = &[
    ("paperplane", SendMessage),
    ("arrow.up", SendMessage),
    ("arrow.up.circle", SendMessage),
    ("arrow.up.message", SendMessage),
    ("phone", Call),
    ("video", Call),
    ("envelope", SendMessage),
    ("trash", Delete),
    ("trash.slash", RiskCategory::None),
    ("xmark.bin", Delete),
    ("minus.circle", Delete),
    ("square.and.arrow.up", PublicPost),
    ("cart", Purchase),
    ("bag", Purchase),
    ("creditcard", Purchase),
    ("dollarsign", Payment),
    ("banknote", Payment),
];

/// What an [`AppRule`] applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleTarget {
    /// Any pressable element that is not a tab, a key or plainly navigation.
    AnyPressable,
    /// Table and collection cells.
    Cell,
    /// Switches.
    Switch,
    /// Pressable elements whose text contains a phone number.
    PhoneNumber,
}

/// A behaviour that applies only inside apps whose bundle id starts with `bundle_prefix`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppRule {
    /// Bundle id prefix, compared case-insensitively.
    pub bundle_prefix: &'static str,
    /// Which elements it covers.
    pub target: RuleTarget,
    /// The category a tap on them gets (at least).
    pub category: RiskCategory,
}

/// App-scoped rules: tapping a Phone row dials, a Home tile operates an accessory, a
/// switch in Settings or Control Center changes the device.
pub const APP_RULES: &[AppRule] = &[
    AppRule {
        bundle_prefix: "com.apple.mobilephone",
        target: RuleTarget::Cell,
        category: Call,
    },
    AppRule {
        bundle_prefix: "com.apple.mobilephone",
        target: RuleTarget::PhoneNumber,
        category: Call,
    },
    AppRule {
        bundle_prefix: "com.apple.MobileAddressBook",
        target: RuleTarget::PhoneNumber,
        category: Call,
    },
    AppRule {
        bundle_prefix: "com.apple.facetime",
        target: RuleTarget::Cell,
        category: Call,
    },
    AppRule {
        bundle_prefix: "com.apple.Home",
        target: RuleTarget::AnyPressable,
        category: SystemSetting,
    },
    AppRule {
        bundle_prefix: SETTINGS_BUNDLE_ID,
        target: RuleTarget::Switch,
        category: SystemSetting,
    },
    AppRule {
        bundle_prefix: jarvis_proto::SPRINGBOARD_BUNDLE_ID,
        target: RuleTarget::Switch,
        category: SystemSetting,
    },
    AppRule {
        bundle_prefix: "com.apple.controlcenter",
        target: RuleTarget::Switch,
        category: SystemSetting,
    },
];

/// Highest-risk-first ordering used to combine several signals into one category.
pub(crate) fn severity(c: RiskCategory) -> u8 {
    match c {
        RiskCategory::None => 0,
        // Never produced by classification; ranked lowest so it never hides a real signal.
        RiskCategory::FirstUse => 1,
        // "Unknown" yields to any definite signal (a known trash symbol, a checkout).
        RiskCategory::Unverified => 2,
        SystemSetting => 3,
        // Below sending: on a screen with both, context-driven taps read as sending.
        Call => 4,
        SendMessage => 5,
        PublicPost => 6,
        AccountChange => 7,
        Credential => 8,
        Delete => 9,
        Purchase => 10,
        Payment => 11,
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

/// A phrase table indexed by its first word, phrases pre-tokenized.
#[derive(Debug)]
struct Compiled {
    by_first: BTreeMap<String, Vec<(Vec<String>, Phrase)>>,
}

fn compile(table: &[Phrase]) -> Compiled {
    let mut by_first: BTreeMap<String, Vec<(Vec<String>, Phrase)>> = BTreeMap::new();
    for phrase in table {
        let words = tokens(phrase.text);
        if let Some(first) = words.first() {
            by_first
                .entry(first.clone())
                .or_default()
                .push((words, *phrase));
        }
    }
    Compiled { by_first }
}

/// Every table the crate matches against, compiled once.
fn compiled_tables() -> &'static [(&'static [Phrase], Compiled)] {
    static TABLES: OnceLock<Vec<(&'static [Phrase], Compiled)>> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut all: Vec<&'static [Phrase]> = vec![
            ACTION_PHRASES,
            CONTEXT_PHRASES,
            CREDENTIAL_FIELD_PHRASES,
            HIDDEN_FIELD_PHRASES,
            COMPOSE_FIELD_PHRASES,
            SETTINGS_CONTEXT_PHRASES,
            SETTINGS_ACTION_PHRASES,
            NAVIGATION_PHRASES,
            PRICE_VERB_PHRASES,
            SHARE_SAFE_PHRASES,
        ];
        all.extend(APP_PHRASES.iter().map(|(_, t)| *t));
        all.into_iter().map(|t| (t, compile(t))).collect()
    })
}

/// Runs `f` with the compiled form of `table` (cached for the crate's own tables).
fn with_compiled<R>(table: &[Phrase], f: impl FnOnce(&Compiled) -> R) -> R {
    let cached = compiled_tables()
        .iter()
        .find(|(t, _)| t.len() == table.len() && *t == table);
    match cached {
        Some((_, c)) => f(c),
        None => f(&compile(table)),
    }
}

/// Joins runs of two or more single-letter tokens, so a spelled-out `P a y` or `D.E.L.E.T.E`
/// reads as one word.
fn join_spelled(toks: &[String]) -> Vec<String> {
    let single = |t: &String| t.chars().count() == 1 && t.chars().all(char::is_alphabetic);
    let mut out: Vec<String> = Vec::with_capacity(toks.len());
    let mut run: Vec<&String> = Vec::new();
    let flush = |run: &mut Vec<&String>, out: &mut Vec<String>| {
        if run.len() >= 2 {
            out.push(run.iter().map(|s| s.as_str()).collect());
        } else {
            out.extend(run.iter().map(|s| (*s).clone()));
        }
        run.clear();
    };
    for t in toks {
        if single(t) {
            run.push(t);
        } else {
            flush(&mut run, &mut out);
            out.push(t.clone());
        }
    }
    flush(&mut run, &mut out);
    out
}

/// Applies `tables` to the label tokens `toks`, and to the same tokens with spelled-out
/// letters joined; the riskier reading wins.
pub(crate) fn match_tokens(toks: &[String], tables: &[&[Phrase]]) -> LabelRisk {
    let plain = match_tokens_once(toks, tables);
    let joined = join_spelled(toks);
    if joined.as_slice() == toks {
        return plain;
    }
    let spelled = match_tokens_once(&joined, tables);
    LabelRisk {
        category: max_risk(plain.category, spelled.category),
        generic: plain.generic || spelled.generic,
    }
}

/// Applies `tables` to the label tokens `toks` in O(tokens × phrases sharing a first
/// word), then drops every match strictly inside a longer one with a single sweep.
fn match_tokens_once(toks: &[String], tables: &[&[Phrase]]) -> LabelRisk {
    // (start, end, category, generic) for every raw match.
    let mut found: Vec<(usize, usize, RiskCategory, bool)> = Vec::new();
    for table in tables {
        with_compiled(table, |c| {
            for (i, t) in toks.iter().enumerate() {
                let Some(cands) = c.by_first.get(t) else {
                    continue;
                };
                for (words, phrase) in cands {
                    let end = i.saturating_add(words.len());
                    if toks.get(i..end) == Some(words.as_slice()) {
                        found.push((i, end, phrase.category, phrase.inherits_context));
                    }
                }
            }
        });
    }
    // Start ascending, end descending: every earlier group either starts earlier or is
    // longer, so a match is swallowed exactly when an earlier group reaches its end.
    found.sort_by_key(|&(s, e, _, _)| (s, std::cmp::Reverse(e)));
    let mut out = LabelRisk::default();
    let mut best_end: Option<usize> = None;
    let mut group: Option<(usize, usize)> = None;
    let mut group_swallowed = false;
    for &(s, e, category, generic) in &found {
        if group != Some((s, e)) {
            if let Some((_, ge)) = group {
                best_end = Some(best_end.map_or(ge, |b| b.max(ge)));
            }
            group = Some((s, e));
            group_swallowed = best_end.is_some_and(|b| b >= e);
        }
        if !group_swallowed {
            out.category = max_risk(out.category, category);
            out.generic |= generic;
        }
    }
    out
}

/// Applies only the phrases of `tables` that cover *all* of `toks` (spelled-out letters
/// joined too).
pub(crate) fn match_whole(toks: &[String], tables: &[&[Phrase]]) -> LabelRisk {
    let mut out = LabelRisk::default();
    for candidate in [toks.to_vec(), join_spelled(toks)] {
        let Some(first) = candidate.first() else {
            continue;
        };
        for table in tables {
            with_compiled(table, |c| {
                for (words, phrase) in c.by_first.get(first).into_iter().flatten() {
                    if *words == candidate {
                        out.category = max_risk(out.category, phrase.category);
                        out.generic |= phrase.inherits_context;
                    }
                }
            });
        }
    }
    out
}

/// Applies `tables` to `label` (at most [`crate::text::MAX_LABEL_CHARS`] of it, head and
/// tail).
pub(crate) fn match_label(label: &str, tables: &[&[Phrase]]) -> LabelRisk {
    match_tokens(&tokens(&bounded(label)), tables)
}

/// Whether `bundle_id` starts with `prefix`, ignoring ASCII case.
pub(crate) fn in_app(bundle_id: &str, prefix: &str) -> bool {
    bundle_id
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// The phrase tables that apply to the tapped element inside `bundle_id`.
pub(crate) fn action_tables(bundle_id: &str) -> Vec<&'static [Phrase]> {
    let mut tables = vec![ACTION_PHRASES];
    for (app, phrases) in APP_PHRASES {
        if in_app(bundle_id, app) {
            tables.push(phrases);
        }
    }
    tables
}

/// The meaning of `label` if it is an SF Symbol name (`paperplane.fill`, `trash`):
/// `Some(category)` from [`SF_SYMBOLS`] (`Some(RiskCategory::None)` for an unknown one),
/// or `None` when it does not look like a dotted symbol name. Undotted names such as
/// `trash` or `paperplane` are ordinary words for the phrase table.
pub(crate) fn sf_symbol_risk(label: &str) -> Option<RiskCategory> {
    let l = label.trim();
    let symbol_chars = l.contains('.')
        && !l.starts_with('.')
        && !l.ends_with('.')
        && l.chars().any(|c| c.is_ascii_lowercase())
        && l.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.');
    if !symbol_chars {
        return None;
    }
    let best = SF_SYMBOLS
        .iter()
        .filter(|(name, _)| {
            l == *name
                || l.strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .max_by_key(|(name, _)| name.len());
    Some(best.map_or(RiskCategory::None, |(_, c)| *c))
}

/// The category the action phrase table assigns to `label` on its own, without screen
/// context or app-specific rows. Useful for testing and extending the table.
pub fn label_risk(label: &str) -> RiskCategory {
    match_label(label, &[ACTION_PHRASES]).category
}
