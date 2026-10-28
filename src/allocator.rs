use libc::{
    self, c_void, MAP_ANONYMOUS, MAP_FAILED, MAP_HUGETLB, MAP_PRIVATE, PROT_READ, PROT_WRITE,
};
use std::{alloc::Layout, fs, io, ptr::null_mut, sync::OnceLock};

// https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt
const MEMINFO_PATH: &str = "/proc/meminfo";
const TOKEN: &str = "Hugepagesize:";

// Only successful allocator initialization is retained, so deallocation never
// needs to read /proc. The public query below always reads the current value.
static HUGEPAGE_SIZE: OnceLock<usize> = OnceLock::new();

#[cfg(test)]
std::thread_local! {
    pub(crate) static DEALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Returns the system's default hugepage size in bytes.
///
/// This is the page size used by [`alloc`] and [`dealloc`], rather than the
/// regular OS page size. Every call reads `/proc/meminfo` again; query results
/// are not cached.
///
/// Querying the size does not require any hugepages to have been reserved and
/// does not indicate whether any are available for allocation.
///
/// # Errors
///
/// Returns an I/O error if `/proc/meminfo` cannot be read, or
/// [`io::ErrorKind::InvalidData`] if its hugepage size is missing or invalid.
///
/// # Examples
///
/// ```
/// # fn main() -> std::io::Result<()> {
/// let size = hugepage_rs::hugepage_size()?;
/// println!("Default hugepage size: {size} bytes");
/// # Ok(())
/// # }
/// ```
pub fn hugepage_size() -> io::Result<usize> {
    let buf = fs::read_to_string(MEMINFO_PATH)?;
    parse_hugepage_size(&buf).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid hugepage size in /proc/meminfo",
        )
    })
}

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

            let size = size.checked_mul(multiplier)?;
            return size.is_power_of_two().then_some(size);
        }
    }

    None
}

fn initialize_hugepage_size(
    cache: &OnceLock<usize>,
    read_size: impl FnOnce() -> io::Result<usize>,
) -> io::Result<usize> {
    if let Some(&size) = cache.get() {
        return Ok(size);
    }
    let size = read_size()?;
    Ok(*cache.get_or_init(|| size))
}

fn align_to(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

/// Allocate memory with the hugepage allocator.
///
/// Returns null if allocation fails, the default hugepage size cannot be read,
/// or the requested alignment exceeds that size. A failed page-size read can
/// be retried by calling this function again.
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
    let Ok(hugepage_size) = initialize_hugepage_size(&HUGEPAGE_SIZE, hugepage_size) else {
        return null_mut();
    };
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
    // A valid allocation has already initialized the page size successfully.
    let hugepage_size = *HUGEPAGE_SIZE
        .get()
        .expect("hugepage size must be initialized before deallocation");
    let len = align_to(layout.size(), hugepage_size);
    unsafe { libc::munmap(p as *mut c_void, len) };
    #[cfg(test)]
    DEALLOCATIONS.with(|count| count.set(count.get() + 1));
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
    fn test_parse_invalid_hugepage_size() {
        assert_eq!(parse_hugepage_size("Hugepagesize: 0 kB"), None);
        assert_eq!(parse_hugepage_size("Hugepagesize: 3 kB"), None);
        assert_eq!(parse_hugepage_size("Hugepagesize: 2048 MB"), None);
        assert_eq!(
            parse_hugepage_size(&format!("Hugepagesize: {} kB", usize::MAX)),
            None
        );
    }

    #[test]
    fn test_hugepage_size() {
        let size = crate::hugepage_size().unwrap();
        assert!(size.is_power_of_two());
        assert_eq!(crate::hugepage_size().unwrap(), size);
    }

    #[test]
    fn test_initialization_failure_can_be_retried() {
        let cache = OnceLock::new();
        let error = initialize_hugepage_size(&cache, || {
            Err(io::Error::new(io::ErrorKind::NotFound, "missing meminfo"))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(error.to_string(), "missing meminfo");
        assert_eq!(cache.get(), None);

        let size = 2 * 1024 * 1024;
        assert_eq!(initialize_hugepage_size(&cache, || Ok(size)).unwrap(), size);
        assert_eq!(
            initialize_hugepage_size(&cache, || panic!("already initialized")).unwrap(),
            size
        );
    }

    #[test]
    fn test_concurrent_initialization_uses_one_page_size() {
        let cache = OnceLock::new();
        let barrier = std::sync::Barrier::new(4);
        let sizes = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|index| {
                    let cache = &cache;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        initialize_hugepage_size(cache, || {
                            barrier.wait();
                            Ok(4096 << index)
                        })
                        .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        let size = *cache.get().unwrap();
        assert!(sizes.iter().all(|&result| result == size));
    }

    #[test]
    fn test_align_to() {
        assert_eq!(align_to(8, 4), 8);
        assert_eq!(align_to(8, 16), 16);
    }

    #[test]
    fn test_unsupported_alignment() {
        let layout = Layout::from_size_align(1, hugepage_size().unwrap() * 2).unwrap();
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
