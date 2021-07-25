## hugepage-rs

hugepage-rs wrapped allocator for linux hugepage.

### Usage

#### HugePage Allocator

Hugepage allocator provides two interfaces for operation, **hugepage_rs::alloc** and **hugepage_rs::dealloc**, allocate and free hugepage memory.

The **hugepage_rs::alloc()** function allocates size bytes and returns a pointer to the allocated memory. *The memory is not initialized*. returns **std::ptr::null_mut()** if allocation fails, otherwise returns a pointer.

```rust
use hugepage_rs;

use std::alloc::Layout;
use std::{mem, ptr};

fn main() {
    let layout = Layout::array::<char>(2048).unwrap();
    let dst = hugepage_rs::alloc(layout);

    let src = String::from("hello");
    let len = src.len();
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst, len);
        let s = String::from_raw_parts(dst, len, len);
        assert_eq!(s, src);
        mem::forget(s);
    }

    hugepage_rs::dealloc(dst, layout);
}
```

#### HugePage Boxed

Simple Box<T> implementation with ownership, data on hugepage allocated memory, released directly after going out of scope, memory allocator using hugepage allocator.

```
use hugepage_rs;

fn main() {
    let mut v = hugepage_rs::Box::new(5);
    *v += 42;
    assert_eq!(*v, 47);
}
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


