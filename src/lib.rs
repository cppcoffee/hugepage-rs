#[cfg(target_os = "linux")]
#[macro_use]
extern crate lazy_static;

#[cfg(target_os = "linux")]
use std::alloc::{GlobalAlloc, Layout};

#[cfg(target_os = "linux")]
mod allocator;
#[cfg(target_os = "linux")]
use allocator::HugePageAllocator;

#[cfg(target_os = "linux")]
mod boxed;
#[cfg(target_os = "linux")]
pub use boxed::Box;

#[cfg(target_os = "linux")]
lazy_static! {
    static ref HUGEPAGE_ALLOCATOR: HugePageAllocator = HugePageAllocator;
}

#[cfg(target_os = "linux")]
pub(crate) fn default_allocator() -> &'static HugePageAllocator {
    &HUGEPAGE_ALLOCATOR
}

/// Allocate memory with the hugepage allocator.
///
/// This function forwards calls to the HugePageAllocator.alloc() function.
///
/// # Safety
///
/// See [`GlobalAlloc::alloc`].
///
/// # Examples
///
/// ```
/// use std::alloc::Layout;
/// use hugepage_rs::{alloc, dealloc};
///
/// unsafe {
///     let layout = Layout::new::<u16>();
///     let ptr = alloc(layout);
///
///     *(ptr as *mut u16) = 42;
///     assert_eq!(*(ptr as *mut u16), 42);
///
///     dealloc(ptr, layout);
/// }
/// ```
#[cfg(target_os = "linux")]
pub fn alloc(layout: Layout) -> *mut u8 {
    unsafe { HUGEPAGE_ALLOCATOR.alloc(layout) }
}

/// Deallocate memory with the hugepage allocator.
///
/// This function forwards calls to the HugePageAllocator.dealloc() function.
///
/// # Safety
///
/// This function is unsafe because undefined behavior can result if the caller does not ensure all of the following:
///
/// - ptr must denote a block of memory currently allocated via this allocator,
/// - layout must be the same layout that was used to allocate that block of memory.
#[cfg(target_os = "linux")]
pub fn dealloc(p: *mut u8, layout: Layout) {
    unsafe { HUGEPAGE_ALLOCATOR.dealloc(p, layout) }
}
