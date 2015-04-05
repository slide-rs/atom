Atom
----

This is a small library around `AtomicPtr`. This offers safe abstraction for Rust's managed pointers when moving then into and out of a Atom.

An Atom can be used in place of a Mutex in some cases. If the data is immutable, you can swap the contents of the pointers in and out capturing the Boxed data.

There are some additional restrictions, A basic Atom cannot be `deref` since the ownership can be taken at any time. Before you can read the contents, you must `take` or `swap` the atom which transfers the ownership to the caller.

In addition to the basic `Atom` this library also provides `AtomSetOnce`. This only allows for the `Atom` to be set. Because of this restriction `deref` can be used, since the content will will be owned by the `AtomSetOnce`. The only way this type can be unset is if you have an mutable reference to the `AtomSetOnce`. In which case Rust guarantees that there is no outstanding references, making it safe to take or unset the content.

How _safe_ is this?
===================

I have tried not to expose any access patterns that I know to be unsafe. But there may be edge cases that I am unaware of. So right now, assume that this will eat your kittens if you are not careful.

Even if there is no hidden bug in the library, this only does Atomic operations on a pointer. This is insufficient to guarantee that your program will work on all architectures do to out-of-order read/writes of the contents of the boxes. So be careful when choosing which `Ordering` flag.
