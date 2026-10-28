use libc::{
    self, c_void, MAP_ANONYMOUS, MAP_FAILED, MAP_HUGETLB, MAP_PRIVATE, PROT_READ, PROT_WRITE,
};
use std::{alloc::Layout, fs::File, io::Read, ptr::null_mut, sync::LazyLock};

// https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt
const MEMINFO_PATH: &str = "/proc/meminfo";
const TOKEN: &str = "Hugepagesize:";

static HUGEPAGE_SIZE: LazyLock<usize> = LazyLock::new(|| {
    let mut buf = String::new();
    if let Ok(mut f) = File::open(MEMINFO_PATH) {
        let _ = f.read_to_string(&mut buf);
    }
    parse_hugepage_size(&buf).expect("failed to parse hugepage size from /proc/meminfo")
});

fn parse_hugepage_size(s: &str) -> Option<usize> {
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(TOKEN) {
            let mut parts = rest.split_whitespace();

            let size: usize = parts.next()?.parse().ok()?;

            let multiplier = match parts.next() {
                None => 1,
                Some("kB") => 1024,
                Some(_) => return None,
            };

            return Some(size * multiplier);
        }
    }

    None
}

fn align_to(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

/// Allocate memory with the hugepage allocator.
///
/// Returns null if allocation fails or the requested alignment exceeds the
/// system's default hugepage size.
///
/// # Safety
///
/// `layout` must have non-zero size. See [`std::alloc::GlobalAlloc::alloc`].
///
/// Calling this function requires an unsafe block:
///
/// ```compile_fail,E0133
/// use std::alloc::Layout;
///
/// let _ = hugepage_rs::alloc(Layout::new::<u64>());
/// ```
pub unsafe fn alloc(layout: Layout) -> *mut u8 {
    let hugepage_size = *HUGEPAGE_SIZE;
    // mmap guarantees hugepage alignment, but not larger alignments.
    if layout.align() > hugepage_size {
        return null_mut();
    }
    let len = align_to(layout.size(), hugepage_size);
    let p = unsafe {
        libc::mmap(
            null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_HUGETLB,
            -1,
            0,
        )
    };

    if p == MAP_FAILED {
        return null_mut();
    }

    p as *mut u8
}

/// Deallocate memory with the hugepage allocator.
///
/// # Safety
///
/// - `ptr` must denote a block of memory currently allocated via this allocator.
/// - `layout` must be the same layout that was used to allocate that block of memory.
pub unsafe fn dealloc(p: *mut u8, layout: Layout) {
    let len = align_to(layout.size(), *HUGEPAGE_SIZE);
    unsafe { libc::munmap(p as *mut c_void, len) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ptr, slice, str};

    #[test]
    fn test_parse_hugepage_size() {
        assert_eq!(
            parse_hugepage_size("Hugepagesize: 2048 kB"),
            Some(2048 * 1024)
        );
        assert_eq!(parse_hugepage_size("Hugepagesize: 1024"), Some(1024));
        assert_eq!(parse_hugepage_size("Hugepagesize:"), None);
        assert_eq!(parse_hugepage_size("Hugepagesize: abc kB"), None);
        assert_eq!(parse_hugepage_size(""), None);
    }

    #[test]
    fn test_align_to() {
        assert_eq!(align_to(8, 4), 8);
        assert_eq!(align_to(8, 16), 16);
    }

    #[test]
    fn test_unsupported_alignment() {
        let layout = Layout::from_size_align(1, *HUGEPAGE_SIZE * 2).unwrap();
        assert!(unsafe { alloc(layout) }.is_null());
    }

    #[test]
    fn test_allocator() {
        unsafe {
            let layout = Layout::new::<u16>();
            let p = alloc(layout);
            assert_ne!(p, null_mut());
            *p = 20;
            assert_eq!(*p, 20);
            dealloc(p, layout);
        }

        unsafe {
            let layout = Layout::array::<char>(2048).unwrap();
            let dst = alloc(layout);
            assert_ne!(dst, null_mut());

            let src = String::from("hello rust");
            let len = src.len();
            ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
            let s = str::from_utf8(slice::from_raw_parts(dst, len)).unwrap();
            assert_eq!(s, src);

            dealloc(dst, layout);
        }
    }
}
