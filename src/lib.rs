
use std::sync::atomic::AtomicPtr;
use std::sync::Arc;
use std::mem;
use std::ptr;
use std::marker::PhantomData;

/// An Atom wraps an AtomicPtr, it allows for safe mutation of an atomic
/// into common Rust Types.
pub struct Atom<T, P> where P: IntoRawPtr<T> + FromRawPtr<T> {
    inner: AtomicPtr<T>,
    data: PhantomData<P>
}

pub use std::sync::atomic::Ordering;

impl<T, P> Atom<T, P> where P: IntoRawPtr<T> + FromRawPtr<T> {
    /// Create a empty Atom
    pub fn empty() -> Atom<T, P> {
        Atom {
            inner: AtomicPtr::new(ptr::null_mut()),
            data: PhantomData
        }
    }

    /// Create a new Atomic from Pointer P
    pub fn new(value: P) -> Atom<T, P> {
        Atom {
            inner: AtomicPtr::new(unsafe { value.into_raw() }),
            data: PhantomData
        }
    }

    /// Swap a new value into the Atom, This will try multiple
    /// times until it succeeds. The old value will be returned.
    pub fn swap(&self, v: P, order: Ordering) -> Option<P> {
        let new = unsafe { v.into_raw() };
        let old = self.inner.swap(new, order);
        if !old.is_null() {
            Some(unsafe { FromRawPtr::from_raw(old) })
        } else {
            None
        }
    }

    /// Take the value of the Atom replacing it with null pointer
    /// Returning the contents. If the contents was a Null pointer the
    /// result will be None.
    pub fn take(&self, order: Ordering) -> Option<P> {
        let old = self.inner.swap(ptr::null_mut(), order);
        if !old.is_null() {
            Some(unsafe { FromRawPtr::from_raw(old) })
        } else {
            None
        }
    }

    /// This will do a CAS setting the value only if it is NULL
    /// this will return OK(()) if the value was written,
    /// otherwise a Err(Box<T>) will be returned, where the value was
    /// the same value that you passed into this function
    pub fn set_if_none(&self, v: P, order: Ordering) -> Result<(), P> {
        let new = unsafe { v.into_raw() };
        let old = self.inner.compare_and_swap(ptr::null_mut(), new, order);
        if !old.is_null() {
            Err(unsafe { FromRawPtr::from_raw(new) })
        } else {
            Ok(())
        }
    }
}

impl<T, P> Drop for Atom<T, P> where P: IntoRawPtr<T> + FromRawPtr<T>  {
    fn drop(&mut self) {
        // this is probably paranoid
        // TODO: Acquire?
        self.take(Ordering::SeqCst);
    }
}

/// Convert from into a raw pointer
pub trait IntoRawPtr<T> {
    unsafe fn into_raw(self) -> *mut T;
}

/// Convert from a raw ptr into a pointer
pub trait FromRawPtr<T> {
    unsafe fn from_raw(ptr: *mut T) -> Self;
}

impl<T> IntoRawPtr<T> for Box<T> {
    unsafe fn into_raw(self) -> *mut T {
        mem::transmute(self)
    }
}

impl<T> FromRawPtr<T> for Box<T> {
    unsafe fn from_raw(ptr: *mut T) -> Box<T> {
        mem::transmute(ptr)
    }
}

impl<T> IntoRawPtr<T> for Arc<T> {
    unsafe fn into_raw(self) -> *mut T {
        mem::transmute(self)
    }
}

impl<T> FromRawPtr<T> for Arc<T> {
    unsafe fn from_raw(ptr: *mut T) -> Arc<T> {
        mem::transmute(ptr)
    }
}
