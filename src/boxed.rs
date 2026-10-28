use crate::{alloc, dealloc};

use std::alloc::Layout;
use std::ops::{Deref, DerefMut};
use std::ptr::{self, NonNull};

/// A pointer type for hugepage allocation.
///
/// Zero-sized types do not allocate memory, but their destructors still run.
pub struct Box<T> {
    data: NonNull<T>,
}

unsafe impl<T: Send> Send for Box<T> {}
unsafe impl<T: Sync> Sync for Box<T> {}

impl<T> Box<T> {
    pub fn new(data: T) -> Box<T> {
        let layout = Layout::new::<T>();
        unsafe {
            let nn = if layout.size() == 0 {
                // ZSTs only need a non-null, properly aligned pointer.
                NonNull::dangling()
            } else {
                let p = alloc(layout) as *mut T;
                NonNull::new(p).expect("hugepage allocation failed")
            };
            ptr::write(nn.as_ptr(), data);
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
        struct DeallocGuard(*mut u8, Layout);

        impl Drop for DeallocGuard {
            fn drop(&mut self) {
                if self.1.size() != 0 {
                    // SAFETY: the guard owns the original allocation and layout.
                    unsafe { dealloc(self.0, self.1) };
                }
            }
        }

        // Keep allocation cleanup active if T's destructor unwinds.
        let _guard = DeallocGuard(self.data.as_ptr().cast(), Layout::new::<T>());
        unsafe { ptr::drop_in_place(self.data.as_ptr()) };
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_boxed_zst() {
        let mut value = Box::new(());
        assert_eq!(*value, ());
        *value = ();

        let empty = Box::new([0u64; 0]);
        assert_eq!(*empty, []);
    }

    #[test]
    fn test_boxed_aligned_zst() {
        #[repr(align(4096))]
        struct AlignedZst;

        assert_eq!(std::mem::size_of::<AlignedZst>(), 0);
        let mut value = Box::new(AlignedZst);
        let alignment = std::mem::align_of::<AlignedZst>();
        let address = (&*value as *const AlignedZst) as usize;
        assert_ne!(address, 0);
        assert_eq!(address % alignment, 0);
        let address = (&mut *value as *mut AlignedZst) as usize;
        assert_eq!(address % alignment, 0);
    }

    #[test]
    fn test_boxed_zst_drop() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        struct ZstWithDrop;

        impl Drop for ZstWithDrop {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        assert_eq!(std::mem::size_of::<ZstWithDrop>(), 0);
        let value = Box::new(ZstWithDrop);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        drop(value);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_boxed_drop_panic_deallocates() {
        struct PanickingDrop {
            _value: u8,
        }

        impl Drop for PanickingDrop {
            fn drop(&mut self) {
                panic!("intentional destructor panic");
            }
        }

        let value = Box::new(PanickingDrop { _value: 1 });
        let before = crate::allocator::DEALLOCATIONS.with(|count| count.get());
        let result = std::panic::catch_unwind(|| drop(value));
        assert!(result.is_err());
        assert_eq!(
            crate::allocator::DEALLOCATIONS.with(|count| count.get()),
            before + 1
        );
    }

    #[test]
    fn test_boxed_zst_drop_panic_does_not_deallocate() {
        struct PanickingZst;

        impl Drop for PanickingZst {
            fn drop(&mut self) {
                panic!("intentional ZST destructor panic");
            }
        }

        let value = Box::new(PanickingZst);
        let before = crate::allocator::DEALLOCATIONS.with(|count| count.get());
        let result = std::panic::catch_unwind(|| drop(value));
        assert!(result.is_err());
        assert_eq!(
            crate::allocator::DEALLOCATIONS.with(|count| count.get()),
            before
        );
    }

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
