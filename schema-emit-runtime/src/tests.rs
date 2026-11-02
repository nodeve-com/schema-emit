use super::*;

#[test]
fn multiples() {
    assert!(multiple_of(10.0, 2.0));
    assert!(!multiple_of(7.0, 2.0));
    assert!(multiple_of(0.0075, 0.0001));
    assert!(!multiple_of(1e308, 0.123456789));
    assert!(multiple_of(-4.0, 2.0));
}

#[test]
fn failures_cap_and_count() {
    let mut f = Failures::<1>::default();
    f.fail("A.b", &[], Reason::Range);
    f.fail("A.c[]", &[3], Reason::Absent);
    assert_eq!(f.len(), 1);
    assert_eq!(f.dropped, 1);
    assert_eq!(f.iter().next().map(|x| x.path), Some("A.b"));
}

#[test]
fn under_keeps_the_first_failure_on_its_path() {
    let mut u = Under::new("A.xs");
    u.fail("A.xsy", &[], Reason::Cast);
    u.fail("A.b", &[], Reason::Absent);
    assert_eq!(u.reason, None);
    u.fail("A.xs[].v", &[0], Reason::Range);
    u.fail("A.xs", &[], Reason::Cast);
    assert_eq!(u.reason, Some(Reason::Range));
}

#[test]
fn holds_until_a_failure() {
    let mut h = Holds::default();
    assert!(h.held());
    h.fail("A.b", &[], Reason::Range);
    assert!(!h.held());
}
