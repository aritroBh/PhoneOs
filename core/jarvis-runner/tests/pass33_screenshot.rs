//! Review pass 33: a snapshot's screenshot larger than the wire carries
//! (`MAX_SCREENSHOT_BYTES`; a noise page on a 1320 x 2868 screen can give a PNG that large)
//! is left out, not refused: the snapshot goes out with its tree, no screenshot, and a note
//! saying so. Before, the encoder refused the snapshot, the server answered an error, and
//! every look with a screenshot failed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use jarvis_proto::{Orientation, Rect, Role, Size, UiNode};
use jarvis_runner::message::{decode_reply, encode_reply, SCREENSHOT_TOO_LARGE_NOTE};
use jarvis_runner::{
    ClientEvent, LockState, Reply, Request, RunnerClientChannel, RunnerServerChannel, Screenshot,
    ServerEvent, Snapshot, MAX_NOTES, MAX_SCREENSHOT_BYTES, MAX_SERVER_FRAME,
};

const KEY: [u8; 32] = [0x33; 32];
const NOW: u64 = 1_000_000;
const FAR: u64 = NOW + 60_000;

fn tree() -> UiNode {
    let rows: Vec<UiNode> = (0..50)
        .map(|i| {
            UiNode::new(
                Role::StaticText,
                Rect::new(0.0, 100.0 + f64::from(i) * 10.0, 440.0, 10.0),
            )
            .with_label(format!("Row {i}"))
        })
        .collect();
    UiNode::new(Role::Application, Rect::new(0.0, 0.0, 440.0, 956.0))
        .with_label("Shop")
        .with_children(vec![UiNode::new(
            Role::Window,
            Rect::new(0.0, 0.0, 440.0, 956.0),
        )
        .with_children(rows)])
}

fn shot(len: usize) -> Screenshot {
    Screenshot {
        data: vec![0xA5; len],
        mime_type: "image/png".into(),
        pixel_width: 1320,
        pixel_height: 2868,
        scale: Some(3.0),
    }
}

fn snapshot(notes: Vec<String>, screenshot: Option<Screenshot>) -> Snapshot {
    Snapshot {
        root: tree(),
        app_bundle_id: "com.example.shop".into(),
        app_name: Some("Shop".into()),
        screen_size: Size::new(440.0, 956.0),
        keyboard_visible: false,
        orientation: Orientation::Portrait,
        device_locked: false,
        lock_state: LockState::Unlocked,
        notes,
        screenshot,
        tree_cut: false,
    }
}

fn round_trip(s: Snapshot) -> Snapshot {
    let bytes = encode_reply(7, &Reply::Snapshot(Box::new(s))).expect("encoded");
    assert!(bytes.len() + jarvis_runner::channel::DATA_OVERHEAD <= MAX_SERVER_FRAME);
    let (id, Reply::Snapshot(back)) = decode_reply(&bytes).expect("decoded") else {
        panic!("not a snapshot")
    };
    assert_eq!(id, 7);
    *back
}

/// At the wire: one byte over the limit, and far over it (a 40 MB PNG), the snapshot is
/// encoded with its whole tree and its notes, no screenshot, and the note last; at the limit
/// the screenshot is carried as before (no note).
#[test]
fn an_oversize_screenshot_is_left_out_with_a_note_and_the_tree_is_sent() {
    for len in [MAX_SCREENSHOT_BYTES + 1, 40 * 1024 * 1024] {
        let back = round_trip(snapshot(vec!["a note".into()], Some(shot(len))));
        assert!(back.screenshot.is_none(), "{len}");
        assert_eq!(back.root, tree(), "{len}");
        assert!(!back.tree_cut, "{len}");
        assert_eq!(
            back.notes,
            vec!["a note".to_owned(), SCREENSHOT_TOO_LARGE_NOTE.to_owned()]
        );
    }
    let back = round_trip(snapshot(
        vec!["a note".into()],
        Some(shot(MAX_SCREENSHOT_BYTES)),
    ));
    assert_eq!(
        back.screenshot.map(|s| s.data.len()),
        Some(MAX_SCREENSHOT_BYTES)
    );
    assert_eq!(back.notes, vec!["a note".to_owned()]);
    // No screenshot asked for: nothing changes, no note.
    let back = round_trip(snapshot(vec![], None));
    assert!(back.screenshot.is_none() && back.notes.is_empty());
}

/// The note is kept when the notes are already full: it takes the last note's place (the
/// count stays at `MAX_NOTES`, which the decoder accepts); more notes than that are still
/// refused, as before.
#[test]
fn the_note_survives_full_notes() {
    let notes: Vec<String> = (0..MAX_NOTES).map(|i| format!("note {i}")).collect();
    let back = round_trip(snapshot(
        notes.clone(),
        Some(shot(MAX_SCREENSHOT_BYTES + 1)),
    ));
    assert_eq!(back.notes.len(), MAX_NOTES);
    assert_eq!(back.notes[..MAX_NOTES - 1], notes[..MAX_NOTES - 1]);
    assert_eq!(back.notes.last().unwrap(), SCREENSHOT_TOO_LARGE_NOTE);
    let mut too_many = notes;
    too_many.push("one more".into());
    assert!(encode_reply(1, &Reply::Snapshot(Box::new(snapshot(too_many, None)))).is_err());
}

/// A runner-supplied note that merely says the same words is not special: a snapshot with
/// its screenshot carries both (nothing is dropped because of a note).
#[test]
fn a_forged_note_does_not_drop_a_screenshot() {
    let back = round_trip(snapshot(
        vec![SCREENSHOT_TOO_LARGE_NOTE.into()],
        Some(shot(1_000)),
    ));
    assert_eq!(back.screenshot.map(|s| s.data.len()), Some(1_000));
}

fn sends_s(events: &[ServerEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            ServerEvent::Send(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn sends_c(events: &[ClientEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Send(b) => Some(b.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Through the channels: the client asks for a snapshot with a screenshot, the runner's
/// observation carries one too large to send; the client gets the snapshot (not an error
/// reply), its tree whole, no screenshot, the note.
#[test]
fn through_the_server_channel_the_client_gets_the_snapshot_without_pixels() {
    let entropy = |seed: u64| -> Vec<u8> {
        let mut x = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        (0..32)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    };
    let mut client = RunnerClientChannel::new(&KEY, &entropy(33)).unwrap();
    let mut server = RunnerServerChannel::new(&KEY, &entropy(1_033)).unwrap();
    let hello = client.start().unwrap();
    let server_hello = sends_s(&server.receive(&hello, NOW).unwrap());
    let c_events = client.receive(&server_hello).unwrap();
    assert!(c_events.contains(&ClientEvent::Authenticated));
    let s_events = server.receive(&sends_c(&c_events), NOW).unwrap();
    assert_eq!(s_events, vec![ServerEvent::Authenticated]);
    let request = Request::Snapshot {
        include_screenshot: true,
    };
    let (id, bytes) = client.request(&request, FAR).unwrap();
    let events = server.receive(&bytes, NOW).unwrap();
    assert!(matches!(events.as_slice(), [ServerEvent::Request { .. }]));
    let out = server
        .reply(
            id,
            &Reply::Snapshot(Box::new(snapshot(
                vec![],
                Some(shot(MAX_SCREENSHOT_BYTES + 4096)),
            ))),
        )
        .unwrap();
    let events = client.receive(&out).unwrap();
    let [ClientEvent::Reply {
        id: got,
        reply: Reply::Snapshot(back),
    }] = events.as_slice()
    else {
        panic!("not a snapshot reply: {events:?}")
    };
    assert_eq!(*got, id);
    assert!(back.screenshot.is_none());
    assert_eq!(back.root, tree());
    assert_eq!(back.notes, vec![SCREENSHOT_TOO_LARGE_NOTE.to_owned()]);
    assert!(client.is_open() && !client.has_pending_request());
}
