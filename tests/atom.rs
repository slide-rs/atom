extern crate atom;

use std::sync::*;
use std::sync::atomic::AtomicUsize;
use atom::*;

#[test]
fn swap() {
    let a = Atom::empty();
    assert_eq!(a.swap(Box::new(1u8), Ordering::Relaxed), None);
    assert_eq!(a.swap(Box::new(2u8), Ordering::Relaxed), Some(Box::new(1u8)));
    assert_eq!(a.swap(Box::new(3u8), Ordering::Relaxed), Some(Box::new(2u8)));
}

#[test]
fn take() {
    let a = Atom::new(Box::new(7u8));
    assert_eq!(a.take(Ordering::Relaxed), Some(Box::new(7)));
    assert_eq!(a.take(Ordering::Relaxed), None);
}

#[test]
fn set_if_none() {
    let a = Atom::empty();
    assert_eq!(a.set_if_none(Box::new(7u8), Ordering::Relaxed), Ok(()));
    assert_eq!(a.set_if_none(Box::new(8u8), Ordering::Relaxed), Err(Box::new(8u8)));
}

struct Canary(Arc<AtomicUsize>);

impl Drop for Canary {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn ensure_drop() {
    let v = Arc::new(AtomicUsize::new(0));
    let a = Box::new(Canary(v.clone()));
    let a = Atom::new(a);
    assert_eq!(v.load(Ordering::SeqCst), 0);
    drop(a);
    assert_eq!(v.load(Ordering::SeqCst), 1);
}

#[test]
fn ensure_drop_arc() {
    let v = Arc::new(AtomicUsize::new(0));
    let a = Arc::new(Canary(v.clone()));
    let a = Atom::new(a);
    assert_eq!(v.load(Ordering::SeqCst), 0);
    drop(a);
    assert_eq!(v.load(Ordering::SeqCst), 1);
}
