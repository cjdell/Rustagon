use core::{
  alloc::{GlobalAlloc, Layout},
  sync::atomic::{AtomicUsize, Ordering},
};
use lol_alloc::{FreeListAllocator, LockedAllocator};

static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED: AtomicUsize = AtomicUsize::new(0);

pub struct TrackingAllocator<A> {
  inner: A,
}

impl<A> TrackingAllocator<A> {
  pub const fn new(inner: A) -> Self {
    Self { inner }
  }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for TrackingAllocator<A> {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    let ptr = unsafe { self.inner.alloc(layout) };
    if !ptr.is_null() {
      ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
    }
    ptr
  }

  unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
    unsafe { self.inner.dealloc(ptr, layout) };
    DEALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
  }

  unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
    let ptr = unsafe { self.inner.alloc_zeroed(layout) };
    if !ptr.is_null() {
      ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
    }
    ptr
  }

  unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    let new_ptr = unsafe { self.inner.realloc(ptr, layout, new_size) };
    if !new_ptr.is_null() {
      let old_size = layout.size();
      if new_size > old_size {
        ALLOCATED.fetch_add(new_size - old_size, Ordering::Relaxed);
      } else {
        DEALLOCATED.fetch_add(old_size - new_size, Ordering::Relaxed);
      }
    }
    new_ptr
  }
}

// Wrap your chosen allocator with tracking
// Comment out ALLOCATOR_INNER above and use this instead:
#[global_allocator]
static GLOBAL: TrackingAllocator<LockedAllocator<FreeListAllocator>> =
  TrackingAllocator::new(LockedAllocator::new(FreeListAllocator::new()));

/// Get current memory usage in bytes
#[inline]
pub fn get_current_memory_usage() -> usize {
  ALLOCATED.load(Ordering::Relaxed) - DEALLOCATED.load(Ordering::Relaxed)
}

/// Get total bytes allocated
#[inline]
pub fn get_total_allocated() -> usize {
  ALLOCATED.load(Ordering::Relaxed)
}

/// Get total bytes deallocated
#[inline]
pub fn get_total_deallocated() -> usize {
  DEALLOCATED.load(Ordering::Relaxed)
}

/// Reset counters
pub fn reset_memory_stats() {
  ALLOCATED.store(0, Ordering::Relaxed);
  DEALLOCATED.store(0, Ordering::Relaxed);
}

// WASM exports
#[unsafe(no_mangle)]
pub extern "C" fn get_memory_usage() -> usize {
  get_current_memory_usage()
}

#[unsafe(no_mangle)]
pub extern "C" fn get_memory_allocated() -> usize {
  get_total_allocated()
}

#[unsafe(no_mangle)]
pub extern "C" fn get_memory_deallocated() -> usize {
  get_total_deallocated()
}
