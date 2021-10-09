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
fn compare_and_swap_basics() {
    cas_test_basics_helper(|a, cas_val, next_val| {
        a.compare_and_swap(cas_val, next_val, Ordering::SeqCst)
    });
}

#[test]
fn compare_exchange_basics() {
    cas_test_basics_helper(|a, cas_val, next_val| {
        a.compare_exchange(cas_val, next_val, Ordering::SeqCst, Ordering::SeqCst)
    });
}

#[test]
fn compare_exchange_weak_basics() {
    cas_test_basics_helper(|a, cas_val, next_val| {
        a.compare_exchange_weak(cas_val, next_val, Ordering::SeqCst, Ordering::SeqCst)
    });
}

#[test]
fn compare_and_swap_threads() {
    cas_test_threads_helper(|a, cas_val, next_val| {
        a.compare_and_swap(cas_val, next_val, Ordering::SeqCst)
    });
}

#[test]
fn compare_exchange_threads() {
    cas_test_threads_helper(|a, cas_val, next_val| {
        a.compare_exchange(cas_val, next_val, Ordering::SeqCst, Ordering::SeqCst)
    });
}

#[test]
fn compare_exchange_weak_threads() {
    cas_test_threads_helper(|a, cas_val, next_val| {
        a.compare_exchange_weak(cas_val, next_val, Ordering::SeqCst, Ordering::SeqCst)
    });
}

type TestCASFn = fn(&Atom<Arc<String>>, Option<&Arc<String>>, Option<Arc<String>>)
    -> Result<Option<Arc<String>>, (Option<Arc<String>>, *mut Arc<String>)>;

fn cas_test_basics_helper(cas: TestCASFn) {
    let cur_val = Arc::new("123".to_owned());
    let mut next_val = Arc::new("456".to_owned());
    let other_val = Arc::new("1927447".to_owned());

    let a = Atom::new(cur_val.clone());

    let pcur = IntoRawPtr::into_raw(cur_val.clone());
    let pnext = IntoRawPtr::into_raw(next_val.clone());

    for attempt in vec![None, Some(&other_val), Some(&Arc::new("wow".to_owned()))] {
        let res = cas(&a, attempt, Some(next_val.clone())).unwrap_err();
        next_val = res.0.unwrap();
        assert_eq!(res.1, pcur as *mut _);
    }

    let res = cas(&a, Some(&cur_val), Some(next_val.clone()));
    assert_eq!(res, Ok(Some(cur_val)));

    for attempt in vec![None, Some(&other_val), Some(&Arc::new("wow".to_owned()))] {
        let res = cas(&a, attempt, None).unwrap_err();
        assert_eq!(res, (None, pnext as *mut _));
    }
}

fn cas_test_threads_helper(cas: TestCASFn) {
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

#[test]
fn borrow() {
    let a = Atom::new(&5);
    assert_eq!(a.swap(&7, Ordering::Relaxed), Some(&5));
    assert_eq!(a.take(Ordering::Relaxed), Some(&7));
}
