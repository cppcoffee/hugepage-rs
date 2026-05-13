use crate::default_allocator;

use std::alloc::{GlobalAlloc, Layout};
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};

/// A pointer type for hugepage allocation.
pub struct Box<T> {
    data: NonNull<T>,
}

unsafe impl<T: Send> Send for Box<T> {}
unsafe impl<T: Sync> Sync for Box<T> {}

impl<T> Box<T> {
    pub fn new(data: T) -> Box<T> {
        let layout = Layout::new::<T>();
        unsafe {
            let p = default_allocator().alloc(layout) as *mut T;
            let mut nn = NonNull::new(p).expect("hugepage allocation failed");
            ptr::write(nn.as_mut(), data);
            Self { data: nn }
        }
    }

    /// # Safety
    ///
    /// `raw` must be a non-null pointer previously obtained from `Box::into_raw`.
    pub unsafe fn from_raw(raw: *mut T) -> Self {
        Self {
            data: NonNull::new(raw).expect("Box::from_raw received null pointer"),
        }
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
            ptr::drop_in_place(self.data.as_ptr());
            default_allocator().dealloc(self.data.as_ptr() as *mut u8, Layout::new::<T>());
        }
    }
}

impl<T> Deref for Box<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.data.as_ref() }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { self.data.as_mut() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boxed() {
        {
            let mut v = Box::new(5);
            *v += 42;
            assert_eq!(*v, 47);
        }

        {
            let src: [u32; 4] = [1, 2, 3, 4];
            let mut v = Box::new(src);
            assert_eq!(&*v, &src);

            v[0] = 2;
            assert_ne!(&*v, &src);
            assert_eq!(&*v, &[2, 2, 3, 4]);
        }
    }

    #[test]
    fn test_boxed_drop_inner() {
        // Verify that inner values with Drop are properly dropped
        let s = String::from("hello hugepage");
        let b = Box::new(s.clone());
        assert_eq!(&*b, &s);
        drop(b); // should not leak
    }
}
