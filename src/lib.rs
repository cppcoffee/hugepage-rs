#[macro_use]
extern crate lazy_static;

use std::alloc::{GlobalAlloc, Layout};

mod allocator;
use allocator::HugePageAllocator;

lazy_static! {
    static ref HUGEPAGE_ALLOCATOR: HugePageAllocator = HugePageAllocator;
}

pub fn alloc(layout: Layout) -> *mut u8 {
    unsafe { HUGEPAGE_ALLOCATOR.alloc(layout) }
}

#[cfg(linux)]
pub fn dealloc(p: *mut u8, layout: Layout) {
    unsafe { HUGEPAGE_ALLOCATOR.dealloc(p, layout) }
}
