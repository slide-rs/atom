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

use std::cell::UnsafeCell;
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ptr;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// An Atom wraps an AtomicPtr, it allows for safe mutation of an atomic
/// into common Rust Types.
///
/// All `Ordering` is `AcqRel`.
///
/// ```
/// // Create an empty atom
/// use std::sync::Arc;
/// use atom::Atom;
///
/// let shared_atom = Arc::new(Atom::empty());
///
/// shared_atom.set_if_none(Box::new(42));
/// let old_value = shared_atom.swap(Box::new(75));
///
/// assert_eq!(old_value, Some(Box::new(42)));
/// ```
pub struct Atom<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    inner: AtomicPtr<()>,
    data: PhantomData<UnsafeCell<P>>,
}

impl<P> Debug for Atom<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "atom({:?})", self.inner.load(Ordering::Relaxed))
    }
}

impl<P> Atom<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    /// Create a empty Atom
    pub fn empty() -> Atom<P> {
        Atom {
            inner: AtomicPtr::new(ptr::null_mut()),
            data: PhantomData,
        }
    }

    /// Create a new Atomic from Pointer P
    pub fn new(value: P) -> Atom<P> {
        Atom {
            inner: AtomicPtr::new(value.into_raw()),
            data: PhantomData,
        }
    }

    /// Swap a new value into the Atom, This will try multiple
    /// times until it succeeds. The old value will be returned.
    pub fn swap(&self, v: P) -> Option<P> {
        let new = v.into_raw();
        let old = self.inner.swap(new, Ordering::AcqRel);
        unsafe { Self::inner_from_raw(old) }
    }

    /// Take the value of the Atom replacing it with null pointer
    /// Returning the contents. If the contents was a `null` pointer the
    /// result will be `None`.
    pub fn take(&self) -> Option<P> {
        let old = self.inner.swap(ptr::null_mut(), Ordering::AcqRel);
        unsafe { Self::inner_from_raw(old) }
    }

    /// This will do a `CAS` setting the value only if it is NULL
    /// this will return `None` if the value was written,
    /// otherwise a `Some(v)` will be returned, where the value was
    /// the same value that you passed into this function
    pub fn set_if_none(&self, v: P) -> Option<P> {
        let new = v.into_raw();
        let result = self.inner.compare_exchange(ptr::null_mut(), new, Ordering::AcqRel, Ordering::Acquire);
        if result.is_err() {
            Some(unsafe { FromRawPtr::from_raw(new) })
        } else {
            None
        }
    }

    /// Take the current content, write it into P then do a CAS to extent this
    /// Atom with the previous contents. This can be used to create a LIFO
    ///
    /// Returns true if this set this migrated the Atom from null.
    pub fn replace_and_set_next(
        &self,
        mut value: P,
    ) -> bool
    where
        P: GetNextMut<NextPtr = Option<P>>,
    {
        let next = value.get_next() as *mut Option<P>;
        let raw = value.into_raw();
        // If next was set to Some(P) we want to
        // assert that it was droppeds
        unsafe { ptr::drop_in_place(next) };
        loop {
            let pcurrent = self.inner.load(Ordering::Acquire);
            let current = unsafe { Self::inner_from_raw(pcurrent) };
            unsafe { ptr::write(next, current) };
            let result = self.inner.compare_exchange(pcurrent, raw, Ordering::AcqRel, Ordering::Acquire);
            match result {
                Ok(replaced_ptr) => return replaced_ptr.is_null(),
                _ => {}
            }
        }
    }

    /// Check to see if an atom is None
    ///
    /// This only means that the contents was None when it was measured
    pub fn is_none(&self, order: Ordering) -> bool {
        self.inner.load(order).is_null()
    }

    #[inline]
    fn inner_into_raw(val: Option<P>) -> *mut () {
        match val {
            Some(val) => val.into_raw(),
            None => ptr::null_mut(),
        }
    }

    #[inline]
    unsafe fn inner_from_raw(ptr: *mut ()) -> Option<P> {
        if !ptr.is_null() {
            Some(FromRawPtr::from_raw(ptr))
        } else {
            None
        }
    }
}

impl<P, T> Atom<P>
where
    P: IntoRawPtr + FromRawPtr + Deref<Target = T>,
{
    /// Stores `new` in the Atom if `current` has the same raw pointer
    /// representation as the currently stored value.
    ///
    /// On success, the Atom's previous value is returned. On failure, `new` is
    /// returned together with a raw pointer to the Atom's current unchanged
    /// value, which is **not safe to dereference**, especially if the Atom is
    /// accessed from multiple threads.
    pub fn compare_and_swap(
        &self,
        current: Option<&P>,
        new: Option<P>,
    ) -> Result<Option<P>, (Option<P>, *mut P)> {
        let pcurrent = Self::inner_as_ptr(current);
        let pnew = Self::inner_into_raw(new);
        let pprev = self.inner.compare_exchange(pcurrent, pnew, Ordering::AcqRel, Ordering::Acquire);
        match pprev {
            Ok(pprev) => Ok(unsafe { Self::inner_from_raw(pprev) }),
            Err(pprev) => Err((unsafe { Self::inner_from_raw(pnew) }, pprev as *mut P))
        }
    }

    /// Stores a value into the pointer if the current value is the same as the
    /// `current` value.
    ///
    /// The return value is a result indicating whether the new value was
    /// written and containing the previous value. On success this value is
    /// guaranteed to be equal to `current`.
    pub fn compare_exchange(
        &self,
        current: Option<&P>,
        new: Option<P>,
    ) -> Result<Option<P>, (Option<P>, *mut P)> {
        let pnew = Self::inner_into_raw(new);
        self.inner
            .compare_exchange(Self::inner_as_ptr(current), pnew, Ordering::AcqRel, Ordering::Acquire)
            .map(|pprev| unsafe { Self::inner_from_raw(pprev) })
            .map_err(|pprev| (unsafe { Self::inner_from_raw(pnew) }, pprev as *mut P))
    }

    /// Stores a value into the pointer if the current value is the same as the
    /// `current` value.
    ///
    /// Unlike `compare_exchange`, this function is allowed to spuriously fail
    /// even when the comparison succeeds, which can result in more efficient
    /// code on some platforms. The return value is a result indicating whether
    /// the new value was written and containing the previous value.
    pub fn compare_exchange_weak(
        &self,
        current: Option<&P>,
        new: Option<P>,
    ) -> Result<Option<P>, (Option<P>, *mut P)> {
        let pnew = Self::inner_into_raw(new);
        self.inner
            .compare_exchange_weak(Self::inner_as_ptr(current), pnew, Ordering::AcqRel, Ordering::Acquire)
            .map(|pprev| unsafe { Self::inner_from_raw(pprev) })
            .map_err(|pprev| (unsafe { Self::inner_from_raw(pnew) }, pprev as *mut P))
    }

    #[inline]
    fn inner_as_ptr(val: Option<&P>) -> *mut () {
        match val {
            Some(val) => &**val as *const _ as *mut (),
            None => ptr::null_mut(),
        }
    }
}

impl<P> Drop for Atom<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    fn drop(&mut self) {
        self.take();
    }
}

unsafe impl<P> Send for Atom<P>
where
    P: IntoRawPtr + FromRawPtr + Send,
{
}
unsafe impl<P> Sync for Atom<P>
where
    P: IntoRawPtr + FromRawPtr + Send,
{
}

/// Convert from into a raw pointer
pub trait IntoRawPtr {
    fn into_raw(self) -> *mut ();
}

/// Convert from a raw ptr into a pointer
pub trait FromRawPtr {
    unsafe fn from_raw(ptr: *mut ()) -> Self;
}

impl<T> IntoRawPtr for Box<T> {
    #[inline]
    fn into_raw(self) -> *mut () {
        Box::into_raw(self) as *mut ()
    }
}

impl<T> FromRawPtr for Box<T> {
    #[inline]
    unsafe fn from_raw(ptr: *mut ()) -> Box<T> {
        Box::from_raw(ptr as *mut T)
    }
}

impl<T> IntoRawPtr for Arc<T> {
    #[inline]
    fn into_raw(self) -> *mut () {
        Arc::into_raw(self) as *mut T as *mut ()
    }
}

impl<T> FromRawPtr for Arc<T> {
    #[inline]
    unsafe fn from_raw(ptr: *mut ()) -> Arc<T> {
        Arc::from_raw(ptr as *const () as *const T)
    }
}

// This impl can be useful for stack-allocated and 'static values.
impl<'a, T> IntoRawPtr for &'a T {
    #[inline]
    fn into_raw(self) -> *mut () {
        self as *const _ as *mut ()
    }
}

impl<'a, T> FromRawPtr for &'a T {
    #[inline]
    unsafe fn from_raw(ptr: *mut ()) -> &'a T {
        &*(ptr as *mut T)
    }
}

/// Transforms lifetime of the second pointer to match the first.
#[inline]
unsafe fn copy_lifetime<'a, S: ?Sized, T: ?Sized + 'a>(_ptr: &'a S, ptr: &T) -> &'a T {
    &*(ptr as *const T)
}

/// Transforms lifetime of the second pointer to match the first.
#[inline]
#[allow(unknown_lints, mut_from_ref)]
unsafe fn copy_mut_lifetime<'a, S: ?Sized, T: ?Sized + 'a>(_ptr: &'a S, ptr: &mut T) -> &'a mut T {
    &mut *(ptr as *mut T)
}

/// This is a restricted version of the Atom. It allows for only
/// `set_if_none` to be called.
///
/// `swap` and `take` can be used only with a mutable reference. Meaning
/// that AtomSetOnce is not usable as a
#[derive(Debug)]
pub struct AtomSetOnce<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    inner: Atom<P>,
}

impl<P> AtomSetOnce<P>
where
    P: IntoRawPtr + FromRawPtr,
{
    /// Create an empty `AtomSetOnce`
    pub fn empty() -> AtomSetOnce<P> {
        AtomSetOnce {
            inner: Atom::empty(),
        }
    }

    /// Create a new `AtomSetOnce` from Pointer P
    pub fn new(value: P) -> AtomSetOnce<P> {
        AtomSetOnce {
            inner: Atom::new(value),
        }
    }

    /// This will do a `CAS` setting the value only if it is NULL
    /// this will return `OK(())` if the value was written,
    /// otherwise a `Err(P)` will be returned, where the value was
    /// the same value that you passed into this function
    pub fn set_if_none(&self, v: P) -> Option<P> {
        self.inner.set_if_none(v)
    }

    /// Convert an `AtomSetOnce` into an `Atom`
    pub fn into_atom(self) -> Atom<P> {
        self.inner
    }

    /// Allow access to the atom if exclusive access is granted
    pub fn atom(&mut self) -> &mut Atom<P> {
        &mut self.inner
    }

    /// Check to see if an atom is None
    ///
    /// This only means that the contents was None when it was measured
    pub fn is_none(&self, order: Ordering) -> bool {
        self.inner.is_none(order)
    }
}

impl<T, P> AtomSetOnce<P>
where
    P: IntoRawPtr + FromRawPtr + Deref<Target = T>,
{
    /// If the Atom is set, get the value
    pub fn get(&self) -> Option<&T> {
        let ptr = self.inner.inner.load(Ordering::Acquire);
        let val = unsafe { Atom::inner_from_raw(ptr) };
        val.map(|v: P| {
            // This is safe since ptr cannot be changed once it is set
            // which means that this is now a Arc or a Box.
            let out = unsafe { copy_lifetime(self, &*v) };
            mem::forget(v);
            out
        })
    }
}

impl<T> AtomSetOnce<Box<T>> {
    /// If the Atom is set, get the value
    pub fn get_mut(&mut self) -> Option<&mut T> {
        let ptr = self.inner.inner.load(Ordering::Acquire);
        let val = unsafe { Atom::inner_from_raw(ptr) };
        val.map(move |mut v: Box<T>| {
            // This is safe since ptr cannot be changed once it is set
            // which means that this is now a Arc or a Box.
            let out = unsafe { copy_mut_lifetime(self, &mut *v) };
            mem::forget(v);
            out
        })
    }
}

impl<T> AtomSetOnce<T>
where
    T: Clone + IntoRawPtr + FromRawPtr,
{
    /// Duplicate the inner pointer if it is set
    pub fn dup(&self) -> Option<T> {
        let ptr = self.inner.inner.load(Ordering::Acquire);
        let val = unsafe { Atom::inner_from_raw(ptr) };
        val.map(|v: T| {
            let out = v.clone();
            mem::forget(v);
            out
        })
    }
}

/// This is a utility Trait that fetches the next ptr from
/// an object.
pub trait GetNextMut {
    type NextPtr;
    fn get_next(&mut self) -> &mut Self::NextPtr;
}
