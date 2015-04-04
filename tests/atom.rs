extern crate atom;

use std::thread;
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

#[test]
fn ensure_send() {
    let atom = Arc::new(Atom::empty());
    let wait = Arc::new(Barrier::new(2));

    let w = wait.clone();
    let a = atom.clone();
    thread::spawn(move || {
        a.swap(Box::new(7u8), Ordering::SeqCst);
        w.wait();
    });

    wait.wait();
    assert_eq!(atom.take(Ordering::SeqCst), Some(Box::new(7u8)));
}

#[test]
fn get() {
    let atom = Arc::new(AtomSetOnce::empty());
    assert_eq!(atom.get(Ordering::Relaxed), None);
    assert_eq!(atom.set_if_none(Box::new(8u8), Ordering::SeqCst), Ok(()));
    assert_eq!(atom.get(Ordering::Relaxed), Some(&8u8));
}