//! Table-driven classification of single tapped labels, with no screen context.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::{app_screen, button};
use jarvis_proto::{Action, RiskCategory};
use jarvis_safety::{classify, label_risk};

use RiskCategory::{
    AccountChange, Credential, Delete, Payment, PublicPost, Purchase, SendMessage, SystemSetting,
};

/// Classifies tapping a lone button labelled `label` in a neutral app.
fn tap(label: &str) -> RiskCategory {
    let s = app_screen(vec![button(1, label)]);
    classify(&Action::Tap { element: 1 }, &s, None)
}

fn check(category: RiskCategory, labels: &[&str]) {
    let wrong: Vec<String> = labels
        .iter()
        .filter_map(|l| {
            let got = tap(l);
            (got != category).then(|| format!("{l:?}: expected {category:?}, got {got:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "misclassified:\n{}", wrong.join("\n"));
}

#[test]
fn send_message_labels() {
    check(
        SendMessage,
        &[
            "Send",
            "SEND",
            "send",
            "Send Message",
            "Send Now",
            "Send email",
            "Send to Alex",
            "Reply",
            "Reply All",
            "Send Reply",
            "Invite",
            "Send Invitation",
            "Resend",
            "Submit",
            "Submit application",
            "Report Junk",
            "Call",
            "Call Mobile",
            "FaceTime",
            "Video Call",
            "Send Feedback",
            "sendButton",
            "Se\u{200B}nd",
            "Ѕеnd",
            "ＳＥＮＤ",
            "Send →",
        ],
    );
}

#[test]
fn public_post_labels() {
    check(
        PublicPost,
        &[
            "Post",
            "Post Now",
            "Tweet",
            "Retweet",
            "Repost",
            "Publish",
            "Share",
            "Share Post",
            "Share to Story",
            "Upload",
            "Go Live",
            "Comment",
            "Post comment",
            "Make Public",
        ],
    );
}

#[test]
fn payment_labels() {
    check(
        Payment,
        &[
            "Pay",
            "Pay Now",
            "PAY NOW",
            "Pay $42.10",
            "Pay with Apple Pay",
            "Apple Pay",
            "PayPal",
            "Send Money",
            "Send $20",
            "Send 20 USD",
            "Transfer",
            "Transfer Money",
            "Transfer Funds",
            "Donate",
            "Donate $5",
            "Add Tip",
            "Withdraw",
            "Cash Out",
            "Top Up",
            "Add Money",
            "Confirm Payment",
            "Make a Payment",
            "Authorize",
            "Double Click to Pay",
            "Slide to Pay",
            "Request Money",
            "Zelle",
            "PayNow",
            "P\u{200D}ay",
            "𝐏𝐚𝐲",
            "Pаy",
        ],
    );
}

#[test]
fn purchase_labels() {
    check(
        Purchase,
        &[
            "Buy",
            "Buy Now",
            "Buy now with 1-Click",
            "Purchase",
            "Confirm Purchase",
            "Complete Purchase",
            "Place Order",
            "Place your order",
            "PlaceOrder",
            "Order Now",
            "Checkout",
            "Check Out",
            "Proceed to checkout",
            "Subscribe",
            "Subscribe Now",
            "Start Free Trial",
            "Try it free",
            "Upgrade",
            "Upgrade to Premium",
            "Go Premium",
            "Remove Ads",
            "Rent",
            "Pre-Order",
            "Renew",
            "Book Now",
            "Reserve",
            "Request Ride",
            "$4.99",
            "€9,99",
            "Add to Cart – $19.99",
            "Buy for £3",
        ],
    );
}

#[test]
fn delete_labels() {
    check(
        Delete,
        &[
            "Delete",
            "DELETE",
            "Delete All",
            "Delete Message",
            "Delete 3 Photos",
            "Delete Account",
            "Delete App",
            "Delete Forever",
            "Remove",
            "Remove All",
            "Remove Download",
            "Erase",
            "Erase All Content and Settings",
            "Erase iPhone",
            "Clear All",
            "Clear History",
            "Clear History and Website Data",
            "Clear Cache",
            "Reset",
            "Reset All Settings",
            "Unsubscribe",
            "Trash",
            "Move to Trash",
            "Empty Trash",
            "Discard",
            "Discard Draft",
            "Forget This Device",
            "Forget This Network",
            "Unpair",
            "Uninstall",
            "Offload App",
            "Unfollow",
            "De\u{00AD}lete",
            "Dеlete",
        ],
    );
}

#[test]
fn account_change_labels() {
    check(
        AccountChange,
        &[
            "Change Password",
            "Update Password",
            "Reset Password",
            "Change Passcode",
            "Turn Passcode Off",
            "Change Email",
            "Sign Out",
            "Sign Out of iCloud",
            "Log Out",
            "Logout",
            "Log off",
            "Deactivate",
            "Deactivate Account",
            "Close Account",
            "Cancel Subscription",
            "Block",
            "Block this Caller",
            "Leave Group",
            "Turn Off Two-Factor",
        ],
    );
}

#[test]
fn system_setting_labels() {
    check(
        SystemSetting,
        &[
            "Allow",
            "Always Allow",
            "Allow Once",
            "Allow While Using App",
            "Allow Full Access",
            "Turn Off",
            "Turn On Bluetooth",
            "Enable",
            "Disable",
            "Trust",
            "Trust This Computer",
            "Install Profile",
            "Download and Install",
            "Restart",
            "Shut Down",
            "slide to power off",
        ],
    );
}

#[test]
fn credential_labels() {
    check(
        Credential,
        &[
            "Show Password",
            "Reveal Password",
            "Copy Password",
            "Share Password",
            "Export Passwords",
            "Show Code",
            "Copy Code",
            "Reveal",
            "Show Recovery Key",
        ],
    );
}

/// Navigation and harmless controls that must not be flagged.
#[test]
fn harmless_labels() {
    check(
        RiskCategory::None,
        &[
            "Messages",
            "Mail",
            "Sender",
            "Sent",
            "Sending…",
            "Send code",
            "Send me a code",
            "Resend code",
            "Resend verification code",
            "Payments",
            "Payment Methods",
            "Purchases",
            "Purchase History",
            "Orders",
            "Order Status",
            "Recently Deleted",
            "Deleted Messages",
            "Posts",
            "Shared with You",
            "Buyer Protection",
            "Missed Call",
            "Call History",
            "Back",
            "Cancel",
            "Not Now",
            "Don't Allow",
            "Don’t Allow",
            "Search",
            "Clear Search",
            "Settings",
            "Wi-Fi",
            "General",
            "Edit",
            "Close",
            "Add to Cart",
            "Archive",
            "Mark as Read",
            "Forgot password?",
            "Sign In",
            "Log In",
            "Learn more",
            "Postal Code",
            "Paywall FAQ",
            "Delivery address",
            "Undo",
            "Continue",
            "OK",
            "Done",
            "Confirm",
        ],
    );
}

#[test]
fn label_risk_matches_whole_words_only() {
    assert_eq!(label_risk("Sender"), RiskCategory::None);
    assert_eq!(label_risk("Resend"), SendMessage);
    assert_eq!(label_risk("Undelete"), RiskCategory::None);
    assert_eq!(label_risk("Repayment schedule"), RiskCategory::None);
    assert_eq!(label_risk("Pay"), Payment);
    // An exemption covers only its own words.
    assert_eq!(label_risk("Pay · Resend code"), Payment);
    assert_eq!(label_risk("Resend code and Delete"), Delete);
    // The more specific phrase wins over the shorter one inside it.
    assert_eq!(label_risk("Reset Password"), AccountChange);
    assert_eq!(label_risk("Send Money"), Payment);
    // Several risks in one label: the most severe wins.
    assert_eq!(label_risk("Delete and Send"), Delete);
}

#[test]
fn keyboard_delete_key_is_not_a_deletion() {
    let s = app_screen(vec![common::el(1, jarvis_proto::Role::Key, "delete")]);
    assert_eq!(
        classify(&Action::Tap { element: 1 }, &s, None),
        RiskCategory::None
    );
    let s = app_screen(vec![button(1, "delete")]);
    assert_eq!(classify(&Action::Tap { element: 1 }, &s, None), Delete);
}

#[test]
fn long_press_reads_the_label_like_a_tap() {
    let s = app_screen(vec![button(1, "Delete")]);
    let a = Action::LongPress {
        element: 1,
        duration_ms: 800,
    };
    assert_eq!(classify(&a, &s, None), Delete);
}

#[test]
fn app_store_get_is_a_purchase_only_in_the_app_store() {
    let store = common::screen("com.apple.AppStore", "App Store", vec![button(1, "Get")]);
    assert_eq!(
        classify(&Action::Tap { element: 1 }, &store, None),
        Purchase
    );
    let other = app_screen(vec![button(1, "Get")]);
    assert_eq!(
        classify(&Action::Tap { element: 1 }, &other, None),
        RiskCategory::None
    );
}
