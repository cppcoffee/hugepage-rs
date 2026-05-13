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
static HUGEPAGE_ALLOCATOR: HugePageAllocator = HugePageAllocator;

#[cfg(target_os = "linux")]
pub(crate) fn default_allocator() -> &'static HugePageAllocator {
    &HUGEPAGE_ALLOCATOR
}

/// Allocate memory with the hugepage allocator.
///
/// # Safety
///
/// See [`GlobalAlloc::alloc`].
#[cfg(target_os = "linux")]
pub fn alloc(layout: Layout) -> *mut u8 {
    unsafe { HUGEPAGE_ALLOCATOR.alloc(layout) }
}

/// Deallocate memory with the hugepage allocator.
///
/// # Safety
///
/// - `ptr` must denote a block of memory currently allocated via this allocator.
/// - `layout` must be the same layout that was used to allocate that block of memory.
#[cfg(target_os = "linux")]
pub unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    HUGEPAGE_ALLOCATOR.dealloc(ptr, layout)
}
