use libc::{
    self, c_void, MAP_ANONYMOUS, MAP_FAILED, MAP_HUGETLB, MAP_PRIVATE, PROT_READ, PROT_WRITE,
};
use std::{
    alloc::{GlobalAlloc, Layout},
    fs::File,
    io::Read,
    ptr::null_mut,
    sync::LazyLock,
};

// https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt
const MEMINFO_PATH: &str = "/proc/meminfo";
const TOKEN: &str = "Hugepagesize:";

pub(crate) static HUGEPAGE_SIZE: LazyLock<usize> = LazyLock::new(|| {
    let mut buf = String::new();
    if let Ok(mut f) = File::open(MEMINFO_PATH) {
        let _ = f.read_to_string(&mut buf);
    }
    parse_hugepage_size(&buf).expect("failed to parse hugepage size from /proc/meminfo")
});

fn parse_hugepage_size(s: &str) -> Option<usize> {
    for line in s.lines() {
        if line.starts_with(TOKEN) {
            let mut parts = line[TOKEN.len()..].split_whitespace();

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

pub(crate) fn aligned_size(layout: Layout) -> usize {
    align_to(layout.size(), *HUGEPAGE_SIZE)
}

// hugepage allocator.
pub(crate) struct HugePageAllocator;

unsafe impl GlobalAlloc for HugePageAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let len = aligned_size(layout);
        let p = libc::mmap(
            null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS | MAP_HUGETLB,
            -1,
            0,
        );

        if p == MAP_FAILED {
            return null_mut();
        }

        p as *mut u8
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        let len = aligned_size(layout);
        libc::munmap(p as *mut c_void, len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{mem, ptr};

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
    fn test_allocator() {
        let hugepage_alloc = HugePageAllocator;

        unsafe {
            let layout = Layout::new::<u16>();
            let p = hugepage_alloc.alloc(layout);
            assert_ne!(p, null_mut());
            *p = 20;
            assert_eq!(*p, 20);
            hugepage_alloc.dealloc(p, layout);
        }

        unsafe {
            let layout = Layout::array::<char>(2048).unwrap();
            let dst = hugepage_alloc.alloc(layout);
            assert_ne!(dst, null_mut());

            let src = String::from("hello rust");
            let len = src.len();
            ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
            let s = String::from_raw_parts(dst, len, len);
            assert_eq!(s, src);
            mem::forget(s);

            hugepage_alloc.dealloc(dst, layout);
        }
    }
}
