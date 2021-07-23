use libc::{
    self, c_void, MAP_ANONYMOUS, MAP_FAILED, MAP_HUGETLB, MAP_PRIVATE, PROT_READ, PROT_WRITE,
};
use std::{
    alloc::{GlobalAlloc, Layout},
    fs::File,
    io::Read,
    ptr::null_mut,
};

// https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt
//
// The output of "cat /proc/meminfo" will include lines like:
// ...
// HugePages_Total: uuu
// HugePages_Free:  vvv
// HugePages_Rsvd:  www
// HugePages_Surp:  xxx
// Hugepagesize:    yyy kB
// Hugetlb:         zzz kB

// constant.
const MEMINFO_PATH: &str = "/proc/meminfo";
const TOKEN: &str = "Hugepagesize:";

lazy_static! {
    static ref HUGEPAGE_SIZE: isize = {
        let buf = File::open(MEMINFO_PATH).map_or("".to_owned(), |mut f| {
            let mut s = String::new();
            let _ = f.read_to_string(&mut s);
            s
        });
        parse_hugepage_size(&buf)
    };
}

fn parse_hugepage_size(s: &str) -> isize {
    for line in s.lines() {
        if line.starts_with(TOKEN) {
            let mut parts = line[TOKEN.len()..].split_whitespace();

            let p = parts.next().unwrap_or("0");
            let mut hugepage_size = p.parse::<isize>().unwrap_or(-1);

            hugepage_size *= parts.next().map_or(1, |x| match x {
                "kB" => 1024,
                _ => 1,
            });

            return hugepage_size;
        }
    }

    return -1;
}

fn align_to(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

// hugepage allocator.
pub(crate) struct HugePageAllocator;

unsafe impl GlobalAlloc for HugePageAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let len = align_to(layout.size(), *HUGEPAGE_SIZE as usize);
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
        libc::munmap(p as *mut c_void, layout.size());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hugepage_size() {
        // correct.
        assert_eq!(parse_hugepage_size("Hugepagesize:1024"), 1024);
        assert_eq!(parse_hugepage_size("Hugepagesize: 2 kB"), 2048);

        // wrong.
        assert_eq!(parse_hugepage_size("Hugepagesize:1kB"), -1);
        assert_eq!(parse_hugepage_size("Hugepagesize: 2kB"), -1);
    }

    #[test]
    fn test_align_to() {
        assert_eq!(align_to(8, 4), 8);
        assert_eq!(align_to(8, 16), 16);
    }

    #[test]
    fn test_basic() {
        let hugepage_alloc = HugePageAllocator;
        let layout = Layout::from_size_align(16, 8).unwrap();
        let p = unsafe { hugepage_alloc.alloc(layout) };
        assert_ne!(p, null_mut());
    }
}
