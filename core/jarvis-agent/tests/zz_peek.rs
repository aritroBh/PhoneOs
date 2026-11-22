use jarvis_proto::fixtures::*;
#[test]
fn peek() {
    for s in [settings_screen(), login_screen(), compose_screen()] {
        let c = jarvis_observe::compact(&s);
        println!("{}\n", jarvis_observe::render(&c, 2000));
    }
}
