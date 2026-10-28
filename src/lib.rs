#[cfg(target_os = "linux")]
mod allocator;
#[cfg(target_os = "linux")]
pub use allocator::{alloc, dealloc, hugepage_size};

#[cfg(target_os = "linux")]
mod boxed;
#[cfg(target_os = "linux")]
pub use boxed::Box;
