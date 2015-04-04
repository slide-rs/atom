
extern crate atom;

use std::thread;
use std::mem;
use std::sync::{Arc, Barrier};
use atom::*;

#[derive(Debug)]
struct Link {
    next: AtomSetOnce<Link, Box<Link>>
}

impl Drop for Link {
    fn drop(&mut self) {
        while let Some(mut h) = self.next.atom().take(Ordering::Relaxed) {
           self.next = mem::replace(&mut h.next, AtomSetOnce::empty());
        }
    }
}

fn main() {
    let b = Arc::new(Barrier::new(101));

    let head = Arc::new(Link{next: AtomSetOnce::empty()});

    for _ in (0..100) {
        let b = b.clone();
        let head = head.clone();
        thread::spawn(move || {
            let mut hptr = &*head;

            for _ in (0..10_000) {
                let mut my_awesome_node = Box::new(Link {
                    next: AtomSetOnce::empty()
                });


                loop {
                    while let Some(h) = hptr.next.get(Ordering::Relaxed) {
                        hptr = h;
                    }

                    my_awesome_node = match hptr.next.set_if_none(my_awesome_node, Ordering::Relaxed) {
                        Some(v) => v,
                        None => break
                    };
                }
            }
            b.wait();
        });
    }

    b.wait();

    let mut hptr = &*head;
    let mut count = 0;
    while let Some(h) = hptr.next.get(Ordering::Relaxed) {
        hptr = h;
        count += 1;
    }
    println!("Using {} threads we wrote {} links at the same time!", 10, count);
}