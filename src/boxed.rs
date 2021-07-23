use crate::default_allocator;

use std::alloc::{GlobalAlloc, Layout};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

/// A pointer type for hugepage allocation.
///
/// Allocates memory on the hugepage and then places x into it.
pub struct Box<T> {
    data: NonNull<T>,
}

impl<T> Box<T> {
    pub fn new(data: T) -> Box<T> {
        let layout = Layout::new::<T>();
        unsafe {
            let mut p = NonNull::new(default_allocator().alloc(layout) as *mut T).unwrap();
            *(p.as_mut()) = data;
            Self { data: p }
        }
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
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
        // variable.
        {
            let mut v = Box::new(5);
            *v += 42;
            assert_eq!(*v, 47);
        }

        // array.
        {
            let src: [u32; 4] = [1, 2, 3, 4];
            let mut v = Box::new(src);
            assert_eq!(&*v, &src);

            v[0] = 2;
            assert_ne!(&*v, &src);
            assert_eq!(&*v, &[2, 2, 3, 4]);
        }
    }
}
