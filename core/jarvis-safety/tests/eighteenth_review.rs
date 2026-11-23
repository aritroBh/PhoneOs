#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Eighteenth review probes (adversarial) on the place definitions. Each asserts the SAFE
//! behaviour: a failing probe is a hole.

use jarvis_safety::{url_destination, Confinement, FlowContext, SYSTEM_BROWSER};

fn site(app: &str, host: &str) -> FlowContext {
    FlowContext::Site {
        app: app.to_owned(),
        host: host.to_owned(),
    }
}

/// Any app may draw a node labelled "Address" whose value is a host, outside its web view:
/// its own web content then becomes the place `Site { app: evil, host: bank.example }`. The
/// keying keeps it apart from the real bank (held below), but every question names a site
/// by its host alone, so the owner reads "bank.example" for content the hostile app chose
/// (flow notes "Jarvis has read bank.example", the paste note "into bank.example").
#[test]
fn probe18_site_display_does_not_name_the_app_that_drew_the_host() {
    let real = site(SYSTEM_BROWSER, "bank.example");
    let fake = site("com.example.evil", "bank.example");
    let name = |b: &str| match b {
        "com.apple.mobilesafari" => Some("Safari".to_owned()),
        "com.example.evil" => Some("Evil".to_owned()),
        _ => None,
    };
    assert_ne!(real, fake);
    assert_ne!(
        real.display(name),
        fake.display(name),
        "two different places are named identically in every question"
    );
}

/// Held: the real bank read in Safari is a source for the hostile app's look-alike site.
#[test]
fn probe18_fake_site_is_not_the_real_site_for_confinement() {
    let mut c = Confinement::new();
    c.read(site(SYSTEM_BROWSER, "bank.example"));
    let fake = site("com.example.evil", "bank.example");
    assert_eq!(
        c.unallowed(&fake, false),
        vec![site(SYSTEM_BROWSER, "bank.example")]
    );
}

/// Held: `open_url` destinations follow the browser's host, not a userinfo or fragment
/// trick (validation refuses `@` in the authority and backslashes before this runs).
#[test]
fn probe18_url_destination_hosts() {
    assert_eq!(
        url_destination("https://evil.example#@bank.example"),
        site(SYSTEM_BROWSER, "evil.example")
    );
    assert_eq!(
        url_destination("https://evil.example/?@bank.example"),
        site(SYSTEM_BROWSER, "evil.example")
    );
    assert_eq!(
        url_destination("HTTPS://Bank.Example./x"),
        site(SYSTEM_BROWSER, "bank.example")
    );
    assert_eq!(url_destination("sms:+1555?body=hi"), FlowContext::Unknown);
    assert_eq!(
        url_destination("https://bänk.example/"),
        FlowContext::Unknown
    );
    assert_eq!(url_destination("maps://?q=x"), FlowContext::Unknown);
}

/// Held: an approval into an app does not cover the same app's readable sites.
#[test]
fn probe18_app_approval_does_not_cover_sites() {
    let mut c = Confinement::new();
    let msgs = FlowContext::app("com.apple.MobileSMS");
    c.read(msgs.clone());
    c.allow(
        &FlowContext::app(SYSTEM_BROWSER),
        std::slice::from_ref(&msgs),
    );
    assert_eq!(
        c.unallowed(&site(SYSTEM_BROWSER, "evil.example"), false),
        vec![msgs.clone()]
    );
    // But it does cover the app's unreadable pages (stated).
    assert!(c
        .unallowed(
            &FlowContext::SiteUnreadable(SYSTEM_BROWSER.to_owned()),
            false
        )
        .is_empty());
}
