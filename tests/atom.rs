//   Copyright 2015 Colin Sherratt
//
//   Licensed under the Apache License, Version 2.0 (the "License");
//   you may not use this file except in compliance with the License.
//   You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
//   Unless required by applicable law or agreed to in writing, software
//   distributed under the License is distributed on an "AS IS" BASIS,
//   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//   See the License for the specific language governing permissions and
//   limitations under the License.

extern crate atom;

use atom::*;
use std::collections::HashSet;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::*;
use std::thread;

#[test]
fn swap() {
    let a = Atom::empty();
    assert_eq!(a.swap(Box::new(1u8), Ordering::AcqRel), None);
    assert_eq!(a.swap(Box::new(2u8), Ordering::AcqRel), Some(Box::new(1u8)));
    assert_eq!(a.swap(Box::new(3u8), Ordering::AcqRel), Some(Box::new(2u8)));
}

#[test]
fn take() {
    let a = Atom::new(Box::new(7u8));
    assert_eq!(a.take(Ordering::Acquire), Some(Box::new(7)));
    assert_eq!(a.take(Ordering::Acquire), None);
}

#[test]
fn set_if_none() {
    let a = Atom::empty();
    assert_eq!(a.set_if_none(Box::new(7u8), Ordering::Release), None);
    assert_eq!(
        a.set_if_none(Box::new(8u8), Ordering::Release),
        Some(Box::new(8u8))
    );
}

#[test]
fn compare_and_swap() {
    cas_test_helper(|a, cas_val, next_val| a.compare_and_swap(cas_val, next_val, Ordering::SeqCst));
}

#[test]
fn compare_exchange() {
    cas_test_helper(|a, cas_val, next_val| {
        a.compare_exchange(cas_val, next_val, Ordering::SeqCst, Ordering::SeqCst)
    });
}

#[test]
fn compare_exchange_weak() {
    cas_test_helper(|a, cas_val, next_val| {
        a.compare_exchange_weak(cas_val, next_val, Ordering::SeqCst, Ordering::SeqCst)
    });
}

fn cas_test_helper(
    cas: fn(&Atom<Arc<String>>, Option<&Arc<String>>, Option<Arc<String>>)
        -> Result<Option<Arc<String>>, (Option<Arc<String>>, *mut Arc<String>)>,
) {
    let cur_val = Arc::new("current".to_owned());
    let next_val = Arc::new("next".to_owned());
    let other_val = Arc::new("other".to_owned());

    let a = Arc::new(Atom::new(cur_val.clone()));

    let num_threads = 10;
    let cas_thread = num_threads / 2;
    let pprevs: Vec<Result<usize, usize>> = (0..num_threads)
        .map(|i| {
            let a = a.clone();
            let cur_val = cur_val.clone();
            let next_val = next_val.clone();
            let other_val = other_val.clone();
            thread::spawn(move || {
                let cas_val = Some(if i == cas_thread {
                    &cur_val
                } else {
                    &other_val
                });
                match cas(&a, cas_val, Some(next_val.clone())) {
                    Ok(prev) => {
                        let prev = prev.unwrap();
                        assert!(Arc::ptr_eq(&prev, &cur_val));
                        assert!(!Arc::ptr_eq(&prev, &next_val));
                        Ok(prev.into_raw() as usize)
                    }
                    Err((_, pprev)) => Err(pprev as usize),
                }
            })
        })
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(pprevs.iter().filter(|pprev| pprev.is_ok()).count(), 1);
    let uniq_pprevs: HashSet<_> = pprevs
        .into_iter()
        .map(|pprev| pprev.unwrap_or_else(|pprev| pprev) as *mut _)
        .collect();
    assert!(uniq_pprevs.contains(&cur_val.into_raw()));
    assert!(!uniq_pprevs.contains(&other_val.into_raw()));
    assert_eq!(a.take(Ordering::Relaxed), Some(next_val));
}

#[derive(Clone)]
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
        a.swap(Box::new(7u8), Ordering::AcqRel);
        w.wait();
    });

    wait.wait();
    assert_eq!(atom.take(Ordering::Acquire), Some(Box::new(7u8)));
}

#[test]
fn get() {
    let atom = Arc::new(AtomSetOnce::empty());
    assert_eq!(atom.get(Ordering::Acquire), None);
    assert_eq!(atom.set_if_none(Box::new(8u8), Ordering::Release), None);
    assert_eq!(atom.get(Ordering::Acquire), Some(&8u8));
}

#[test]
fn get_arc() {
    let atom = Arc::new(AtomSetOnce::empty());
    assert_eq!(atom.get(Ordering::Acquire), None);
    assert_eq!(atom.set_if_none(Arc::new(8u8), Ordering::Release), None);
    assert_eq!(atom.get(Ordering::Acquire), Some(&8u8));

    let v = Arc::new(AtomicUsize::new(0));
    let atom = Arc::new(AtomSetOnce::empty());
    atom.get(Ordering::Acquire);
    atom.set_if_none(Arc::new(Canary(v.clone())), Ordering::Release);
    atom.get(Ordering::Acquire);
    drop(atom);

    assert_eq!(v.load(Ordering::SeqCst), 1);
}

#[derive(Debug)]
struct Link {
    next: Option<Box<Link>>,
    value: u32,
}

impl Link {
    fn new(v: u32) -> Box<Link> {
        Box::new(Link {
            next: None,
            value: v,
        })
    }
}

impl GetNextMut for Box<Link> {
    type NextPtr = Option<Box<Link>>;
    fn get_next(&mut self) -> &mut Option<Box<Link>> {
        &mut self.next
    }
}

#[test]
fn lifo() {
    let atom = Atom::empty();
    for i in 0..100 {
        let x = atom.replace_and_set_next(Link::new(99 - i), Ordering::Relaxed, Ordering::AcqRel);
        assert_eq!(x, i == 0);
    }

    let expected: Vec<u32> = (0..100).collect();
    let mut found = Vec::new();
    let mut chain = atom.take(Ordering::Acquire);
    while let Some(v) = chain {
        found.push(v.value);
        chain = v.next;
    }
    assert_eq!(expected, found);
}

#[allow(dead_code)]
struct LinkCanary {
    next: Option<Box<LinkCanary>>,
    value: Canary,
}

impl LinkCanary {
    fn new(v: Canary) -> Box<LinkCanary> {
        Box::new(LinkCanary {
            next: None,
            value: v,
        })
    }
}

impl GetNextMut for Box<LinkCanary> {
    type NextPtr = Option<Box<LinkCanary>>;
    fn get_next(&mut self) -> &mut Option<Box<LinkCanary>> {
        &mut self.next
    }
}

#[test]
fn lifo_drop() {
    let v = Arc::new(AtomicUsize::new(0));
    let canary = Canary(v.clone());
    let mut link = LinkCanary::new(canary.clone());
    link.next = Some(LinkCanary::new(canary.clone()));

    let atom = Atom::empty();
    atom.replace_and_set_next(link, Ordering::Relaxed, Ordering::AcqRel);
    assert_eq!(1, v.load(Ordering::SeqCst));
    drop(atom);
    assert_eq!(2, v.load(Ordering::SeqCst));
}
