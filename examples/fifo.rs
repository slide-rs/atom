
extern crate atom;

use std::thread;
use std::sync::{Arc, Barrier};
use atom::*;

struct Link {
    next: AtomSetOnce<Link, Arc<Link>>
}

fn main() {
    let b = Arc::new(Barrier::new(11));

    let head = Arc::new(Link{next: AtomSetOnce::empty()});

    for _ in (0..10) {
        let b = b.clone();
        let head = head.clone();
        thread::spawn(move || {
            let mut hptr = &*head;

            for _ in (0..100) {
                let mut my_awesome_node = Arc::new(Link {
                    next: AtomSetOnce::empty()
                });


                loop {
                    while let Some(h) = hptr.next.get(Ordering::SeqCst) {
                        hptr = h;
                    }

                    my_awesome_node = match hptr.next.set_if_none(my_awesome_node, Ordering::SeqCst) {
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
    while let Some(h) = hptr.next.get(Ordering::SeqCst) {
        hptr = h;
        count += 1;
    }
    println!("Using {} threads we wrote {} links at the same time!", 10, count);
}