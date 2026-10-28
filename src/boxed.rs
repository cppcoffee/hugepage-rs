use crate::{alloc, dealloc};

use std::alloc::Layout;
use std::mem::ManuallyDrop;
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

    /// Consumes the box and transfers ownership to a raw pointer.
    ///
    /// The pointer is non-null and properly aligned, including for zero-sized
    /// types. Neither the value nor its allocation is dropped by this function.
    /// The caller becomes responsible for destruction and deallocation; the
    /// simplest way to do that is to reconstruct this box with [`Self::from_raw`].
    ///
    /// # Examples
    ///
    /// ```
    /// let raw = hugepage_rs::Box::into_raw(hugepage_rs::Box::new(String::from("hello")));
    /// // SAFETY: raw came from this box type and ownership has not been reclaimed.
    /// let value = unsafe { hugepage_rs::Box::from_raw(raw) };
    /// assert_eq!(&**value, "hello");
    /// ```
    #[must_use = "losing the pointer leaks the value and its allocation"]
    pub fn into_raw(this: Self) -> *mut T {
        let this = ManuallyDrop::new(this);
        this.data.as_ptr()
    }

    /// Reconstructs a box and takes ownership of its value and allocation.
    ///
    /// # Safety
    ///
    /// - `raw` must have been returned by this crate's [`Self::into_raw`] for
    ///   the same type `T`, and ownership must not have been reclaimed already.
    /// - It must be non-null, properly aligned, and point to a live, initialized
    ///   value of `T`. For nonzero-sized types, the original hugepage allocation
    ///   must still be allocated with the layout for `T`.
    /// - The caller must transfer exclusive ownership. No other owner may drop
    ///   the value or free its allocation, and existing references must not be
    ///   used in conflict with ownership by the returned box.
    ///
    /// For zero-sized types no allocation is required, but the pointer and
    /// ownership requirements still apply. Dropping the returned box drops `T`.
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
    // Match issue #9's nested Box<String> payload intentionally.
    #[allow(clippy::box_collection)]
    fn test_boxed_construction_does_not_drop_uninitialized_value() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        struct DropCount {
            value: std::boxed::Box<String>,
        }

        impl Drop for DropCount {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let value = Box::new(DropCount {
            value: std::boxed::Box::new(String::from("hello")),
        });
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        assert_eq!(value.value.as_str(), "hello");
        drop(value);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_boxed_raw_roundtrip_transfers_ownership_once() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        struct DropCount {
            value: u64,
        }

        impl Drop for DropCount {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let value = Box::new(DropCount { value: 42 });
        let before = crate::allocator::DEALLOCATIONS.with(|count| count.get());
        let raw = Box::into_raw(value);
        assert!(!raw.is_null());
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        assert_eq!(
            crate::allocator::DEALLOCATIONS.with(|count| count.get()),
            before
        );

        // SAFETY: raw came from this box type and still has exclusive ownership.
        let value = unsafe { Box::from_raw(raw) };
        assert_eq!(value.value, 42);
        drop(value);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
        assert_eq!(
            crate::allocator::DEALLOCATIONS.with(|count| count.get()),
            before + 1
        );
    }

    #[test]
    fn test_boxed_aligned_zst_raw_roundtrip() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        #[repr(align(4096))]
        struct AlignedZst;

        impl Drop for AlignedZst {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let before = crate::allocator::DEALLOCATIONS.with(|count| count.get());
        let raw = Box::into_raw(Box::new(AlignedZst));
        assert!(!raw.is_null());
        assert_eq!(raw as usize % std::mem::align_of::<AlignedZst>(), 0);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        // SAFETY: the aligned ZST pointer came from into_raw and is reclaimed once.
        drop(unsafe { Box::from_raw(raw) });
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
        assert_eq!(
            crate::allocator::DEALLOCATIONS.with(|count| count.get()),
            before
        );
    }

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
