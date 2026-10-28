## hugepage-rs

hugepage-rs wrapped allocator for linux hugepage.

### Usage

#### Hugepage Size

`hugepage_rs::hugepage_size()` returns the system's default hugepage size in bytes, as used by this allocator. Every call reads `/proc/meminfo` again without caching the result. I/O errors are returned if `/proc/meminfo` cannot be read; missing or invalid hugepage size information returns `InvalidData`.

```rust
fn main() -> std::io::Result<()> {
    let size = hugepage_rs::hugepage_size()?;
    println!("Default hugepage size: {size} bytes");
    Ok(())
}
```

This queries the hugepage size, not the regular OS page size or the number of available hugepages. It works even when no hugepages have been reserved.

#### HugePage Allocator

Use **hugepage_rs::alloc** to allocate hugepage memory and **hugepage_rs::try_dealloc** to free it with system error reporting. **hugepage_rs::dealloc** remains available for callers that cannot report unmapping errors, including destructors during unwinding.

The **hugepage_rs::alloc()** function allocates size bytes and returns a pointer to the allocated memory. *The memory is not initialized*. returns **std::ptr::null_mut()** if allocation fails, otherwise returns a pointer. Calling it is unsafe and requires a non-zero layout size.

Alignments larger than the system's default hugepage size are unsupported and return a null pointer.

If the allocator cannot read the default hugepage size, allocation returns a null pointer. Later allocation attempts can retry the read. Successful allocator initialization is retained so freeing memory does not depend on reading `/proc/meminfo` again.

```rust
use hugepage_rs;

use std::alloc::Layout;
use std::{ptr, slice, str};

fn main() {
    let layout = Layout::array::<char>(2048).unwrap();
    // SAFETY: the layout has non-zero size.
    let dst = unsafe { hugepage_rs::alloc(layout) };
    assert!(!dst.is_null(), "hugepage allocation failed");

    let src = String::from("hello");
    let len = src.len();
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
        let s = str::from_utf8(slice::from_raw_parts(dst, len)).unwrap();
        assert_eq!(s, src);
    }

    // SAFETY: dst is still allocated and layout matches the allocation.
    unsafe { hugepage_rs::dealloc(dst, layout) };
}
```

#### HugePage Boxed

Simple Box<T> implementation with ownership, data on hugepage allocated memory, released directly after going out of scope, memory allocator using hugepage allocator.

Zero-sized types (such as `()`) do not allocate hugepage memory. Their destructors still run when the box is dropped.

For nonzero-sized types, deallocation is attempted even if the value's destructor panics and unwinds. System unmapping errors do not cause another panic during cleanup.

```
use hugepage_rs;

fn main() {
    let mut v = hugepage_rs::Box::new(5);
    *v += 42;
    assert_eq!(*v, 47);
}
```

`Box::into_raw` transfers ownership to a raw pointer without dropping the value or freeing its memory. Reconstruct it exactly once with this crate's `Box::from_raw` to resume automatic cleanup:

```rust
let raw = hugepage_rs::Box::into_raw(hugepage_rs::Box::new(42u64));
// SAFETY: raw came from this box type, is still valid, and is reclaimed once.
let value = unsafe { hugepage_rs::Box::from_raw(raw) };
assert_eq!(*value, 42);
```


### Notes

- System need to enable hugepage.


### Reference

[Huge pages part 1 (Introduction)](https://lwn.net/Articles/374424/)

[Huge pages part 2: Interfaces](https://lwn.net/Articles/375096/)

[Huge pages part 3: Administration](https://lwn.net/Articles/376606/)

[Huge pages part 4: benchmarking with huge pages](https://lwn.net/Articles/378641/)

[Huge pages part 5: A deeper look at TLBs and costs](https://lwn.net/Articles/379748/)

[https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt](https://www.kernel.org/doc/Documentation/vm/hugetlbpage.txt)

[https://man7.org/linux/man-pages/man2/mmap.2.html](https://man7.org/linux/man-pages/man2/mmap.2.html)


