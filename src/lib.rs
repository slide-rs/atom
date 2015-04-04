
use std::sync::atomic::AtomicPtr;
use std::sync::Arc;
use std::mem;
use std::ptr;
use std::ops::Deref;
use std::marker::PhantomData;

pub use std::sync::atomic::Ordering;

unsafe impl<T, P> Send for Atom<T, P> where P: IntoRawPtr<T> + FromRawPtr<T> {}

/// An Atom wraps an AtomicPtr, it allows for safe mutation of an atomic
/// into common Rust Types.
pub struct Atom<T, P> where P: IntoRawPtr<T> + FromRawPtr<T> {
    inner: AtomicPtr<T>,
    data: PhantomData<P>
}


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
    /// Returning the contents. If the contents was a `null` pointer the
    /// result will be `None`.
    pub fn take(&self, order: Ordering) -> Option<P> {
        let old = self.inner.swap(ptr::null_mut(), order);
        if !old.is_null() {
            Some(unsafe { FromRawPtr::from_raw(old) })
        } else {
            None
        }
    }

    /// This will do a `CAS` setting the value only if it is NULL
    /// this will return `OK(())` if the value was written,
    /// otherwise a `Err(P)` will be returned, where the value was
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

/// Transforms lifetime of the second pointer to match the first.
#[inline]
unsafe fn copy_lifetime<'a, S: ?Sized, T: ?Sized + 'a>(_ptr: &'a S, 
                                                       ptr: &T) -> &'a T {
    mem::transmute(ptr)
}


/// This is a restricted version of the Atom. It allows for onlu
/// `set_if_none` to be called. Since the value cannot be modified via `take`
/// or via `swap` we know that as long as the `AtomSetOnce` is alive so is
/// its data. This allows for traits such as `get`
pub struct AtomSetOnce<T, P> where P: IntoRawPtr<T> + FromRawPtr<T> {
    inner: Atom<T, P>
}

impl<T, P> AtomSetOnce<T, P>
    where P: IntoRawPtr<T> + FromRawPtr<T> + Deref<Target=T> {

    /// Create a empty AtomSetOnce
    pub fn empty() -> AtomSetOnce<T, P> {
        AtomSetOnce { inner: Atom::empty() }
    }

    /// Create a new AtomSetOnce from Pointer P
    pub fn new(value: P) -> AtomSetOnce<T, P> {
        AtomSetOnce { inner: Atom::new(value) }
    }

    /// This will do a `CAS` setting the value only if it is NULL
    /// this will return `OK(())` if the value was written,
    /// otherwise a `Err(P)` will be returned, where the value was
    /// the same value that you passed into this function
    pub fn set_if_none(&self, v: P, order: Ordering) -> Result<(), P> {
        self.inner.set_if_none(v, order)
    }

    /// If the Atom is set, get the value
    pub fn get<'a>(&'a self, order: Ordering) -> Option<&'a T> {
        let ptr = self.inner.inner.load(order);
        if ptr.is_null() {
            None
        } else {
            unsafe {
                // This is safe since ptr cannot be changed once it is set
                // which means that this is now a Arc or a Box.
                let v: P = FromRawPtr::from_raw(ptr);
                let out = copy_lifetime(self, v.deref());
                mem::forget(v);
                Some(out)
            }
        }
    }
}