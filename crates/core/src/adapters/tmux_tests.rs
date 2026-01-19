use super::*;

#[test]
fn prefixed_name_adds_prefix() {
    let adapter = TmuxAdapter::new("oj-");
    assert_eq!(adapter.prefixed_name("test"), "oj-test");
    assert_eq!(adapter.prefixed_name("oj-test"), "oj-test");
}
