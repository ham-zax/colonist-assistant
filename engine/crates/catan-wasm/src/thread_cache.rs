//! Per-thread size-class cache in front of a shared allocator.
//!
//! The threaded WASM build uses std's single `dlmalloc` heap behind one global
//! lock. Search cells allocate small vectors and maps at a high rate, so Rayon
//! workers serialize on that lock and extra browser threads make search slower.
//! Small blocks freed on a thread are kept in that thread's free lists and
//! reused without touching the shared lock. Blocks may migrate between threads
//! (freed where they die); every block lives in the one shared heap, so that is
//! sound. Each list is capped, bounding the memory a thread can hold back.

use std::alloc::{GlobalAlloc, Layout};
use std::cell::UnsafeCell;
use std::ptr;

const MIN_SHIFT: u32 = 4;
const CLASSES: usize = 8; // 16 B ..= 2 KiB
const MAX_CLASS_SIZE: usize = 1 << (MIN_SHIFT as usize + CLASSES - 1);
const CLASS_ALIGN: usize = 1 << MIN_SHIFT;
const MAX_CACHED: u32 = 512;

struct FreeLists {
    heads: [*mut u8; CLASSES],
    counts: [u32; CLASSES],
}

thread_local! {
    // Const-initialized without drop glue: access never allocates or registers
    // a destructor, so it is safe from inside the allocator.
    static LISTS: UnsafeCell<FreeLists> = const {
        UnsafeCell::new(FreeLists { heads: [ptr::null_mut(); CLASSES], counts: [0; CLASSES] })
    };
}

#[inline]
fn class_of(layout: Layout) -> Option<usize> {
    if layout.size() > MAX_CLASS_SIZE || layout.align() > CLASS_ALIGN {
        return None;
    }
    let size = layout.size().max(CLASS_ALIGN).next_power_of_two();
    Some((size.trailing_zeros() - MIN_SHIFT) as usize)
}

#[inline]
fn class_layout(class: usize) -> Layout {
    // SAFETY: power-of-two size >= alignment, both nonzero and small.
    unsafe { Layout::from_size_align_unchecked(CLASS_ALIGN << class, CLASS_ALIGN) }
}

pub struct ThreadCache<A>(pub A);

unsafe impl<A: GlobalAlloc> GlobalAlloc for ThreadCache<A> {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(class) = class_of(layout) else {
            return unsafe { self.0.alloc(layout) };
        };
        let cached = LISTS.with(|lists| {
            // SAFETY: thread-local, and nothing below re-enters this thread's lists.
            let lists = unsafe { &mut *lists.get() };
            let head = lists.heads[class];
            if !head.is_null() {
                // SAFETY: a cached block's first word stores the next free block.
                lists.heads[class] = unsafe { head.cast::<*mut u8>().read() };
                lists.counts[class] -= 1;
            }
            head
        });
        if cached.is_null() {
            unsafe { self.0.alloc(class_layout(class)) }
        } else {
            cached
        }
    }

    #[inline]
    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        let Some(class) = class_of(layout) else {
            return unsafe { self.0.dealloc(block, layout) };
        };
        let kept = LISTS.with(|lists| {
            // SAFETY: as in `alloc`.
            let lists = unsafe { &mut *lists.get() };
            if lists.counts[class] >= MAX_CACHED {
                return false;
            }
            // SAFETY: every class block is at least 16 aligned bytes.
            unsafe { block.cast::<*mut u8>().write(lists.heads[class]) };
            lists.heads[class] = block;
            lists.counts[class] += 1;
            true
        });
        if !kept {
            unsafe { self.0.dealloc(block, class_layout(class)) }
        }
    }

    #[inline]
    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: GlobalAlloc guarantees new_size rounded to align is valid.
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        match (class_of(layout), class_of(new_layout)) {
            (None, None) => unsafe { self.0.realloc(block, layout, new_size) },
            (Some(old), Some(new)) if old == new => block,
            _ => {
                let moved = unsafe { self.alloc(new_layout) };
                if !moved.is_null() {
                    unsafe {
                        ptr::copy_nonoverlapping(block, moved, layout.size().min(new_size));
                        self.dealloc(block, layout);
                    }
                }
                moved
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::System;

    static CACHE: ThreadCache<System> = ThreadCache(System);

    #[test]
    fn small_blocks_are_reused_on_the_freeing_thread() {
        let layout = Layout::from_size_align(40, 8).unwrap();
        unsafe {
            let first = CACHE.alloc(layout);
            CACHE.dealloc(first, layout);
            // Same 64-byte class, different request size.
            let second = CACHE.alloc(Layout::from_size_align(60, 4).unwrap());
            assert_eq!(first, second);
            CACHE.dealloc(second, Layout::from_size_align(60, 4).unwrap());
        }
    }

    #[test]
    fn blocks_can_migrate_to_another_threads_cache() {
        let layout = Layout::from_size_align(40, 8).unwrap();
        let address = unsafe {
            let block = CACHE.alloc(layout);
            assert!(!block.is_null());
            block.write_bytes(0x5a, layout.size());
            block as usize
        };
        std::thread::spawn(move || unsafe {
            let block = address as *mut u8;
            for offset in 0..layout.size() {
                assert_eq!(block.add(offset).read(), 0x5a);
            }
            CACHE.dealloc(block, layout);
            let reused = CACHE.alloc(layout);
            assert_eq!(reused, block);
            assert_eq!(reused as usize % layout.align(), 0);
            CACHE.dealloc(reused, layout);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn realloc_preserves_contents_across_classes_and_large_blocks() {
        let mut layout = Layout::from_size_align(8, 8).unwrap();
        unsafe {
            let mut block = CACHE.alloc(layout);
            for (index, byte) in (0..8u8).enumerate() {
                block.add(index).write(byte);
            }
            for new_size in [16, 100, 3000, 70_000, 12] {
                block = CACHE.realloc(block, layout, new_size);
                assert!(!block.is_null());
                layout = Layout::from_size_align(new_size, 8).unwrap();
                for index in 0..8usize {
                    assert_eq!(block.add(index).read(), index as u8);
                }
            }
            CACHE.dealloc(block, layout);
        }
    }

    #[test]
    fn over_aligned_and_large_layouts_bypass_the_cache() {
        assert_eq!(class_of(Layout::from_size_align(8, 32).unwrap()), None);
        assert_eq!(
            class_of(Layout::from_size_align(MAX_CLASS_SIZE + 1, 8).unwrap()),
            None
        );
        assert_eq!(class_of(Layout::from_size_align(1, 1).unwrap()), Some(0));
        assert_eq!(
            class_of(Layout::from_size_align(MAX_CLASS_SIZE, 16).unwrap()),
            Some(CLASSES - 1)
        );
    }

    #[test]
    fn cache_depth_is_bounded() {
        let layout = Layout::from_size_align(128, 8).unwrap();
        let blocks: Vec<_> = (0..MAX_CACHED + 16)
            .map(|_| unsafe { CACHE.alloc(layout) })
            .collect();
        for &block in &blocks {
            unsafe { CACHE.dealloc(block, layout) };
        }
        let count = LISTS.with(|lists| unsafe { (*lists.get()).counts[class_of(layout).unwrap()] });
        assert_eq!(count, MAX_CACHED);
    }
}
