/// Phases are thread local; executor jobs inherit their submitting phase.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum Phase {
    Other,
    Network,
    Commands,
    Input,
    Simulation,
    Snapshot,
    Storage,
}
pub const PHASES: [Phase; 7] = [
    Phase::Other,
    Phase::Network,
    Phase::Commands,
    Phase::Input,
    Phase::Simulation,
    Phase::Snapshot,
    Phase::Storage,
];
impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::Network => "network",
            Self::Commands => "commands",
            Self::Input => "input",
            Self::Simulation => "simulation",
            Self::Snapshot => "snapshot",
            Self::Storage => "storage",
        }
    }
}
pub const ENABLED: bool = cfg!(feature = "allocation-metrics");
#[derive(Clone, Copy, Default)]
pub struct Counts {
    pub allocations: u64,
    pub reallocations: u64,
    pub bytes: u64,
}

#[cfg(feature = "allocation-metrics")]
mod enabled {
    use super::{Counts, Phase};
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        cell::Cell,
        sync::atomic::{AtomicU64, Ordering},
    };
    thread_local! { static PHASE: Cell<Phase> = const { Cell::new(Phase::Other) }; }
    struct Counters {
        allocations: AtomicU64,
        reallocations: AtomicU64,
        bytes: AtomicU64,
    }
    static COUNTERS: [Counters; 7] = [const {
        Counters {
            allocations: AtomicU64::new(0),
            reallocations: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }; 7];
    pub fn current_phase() -> Phase {
        PHASE.try_with(Cell::get).unwrap_or(Phase::Other)
    }
    pub struct PhaseGuard {
        previous: Phase,
    }
    pub fn enter(phase: Phase) -> PhaseGuard {
        let previous = PHASE
            .try_with(|slot| slot.replace(phase))
            .unwrap_or(Phase::Other);
        PhaseGuard { previous }
    }
    impl Drop for PhaseGuard {
        fn drop(&mut self) {
            let _ = PHASE.try_with(|slot| slot.set(self.previous));
        }
    }
    fn record(bytes: usize, reallocation: bool) {
        let counters = &COUNTERS[current_phase() as usize];
        if reallocation {
            counters.reallocations.fetch_add(1, Ordering::Relaxed);
        } else {
            counters.allocations.fetch_add(1, Ordering::Relaxed);
        }
        counters.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }
    pub fn snapshot() -> [Counts; 7] {
        std::array::from_fn(|i| Counts {
            allocations: COUNTERS[i].allocations.load(Ordering::Relaxed),
            reallocations: COUNTERS[i].reallocations.load(Ordering::Relaxed),
            bytes: COUNTERS[i].bytes.load(Ordering::Relaxed),
        })
    }
    pub struct CountingAllocator;
    // SAFETY: forwards the exact pointer/layout contract to System. Counters use only
    // const TLS Cell and relaxed atomics: no allocation, logging, mutex, or unwinding.
    // Only successful allocation/reallocation is counted; deallocation is unchanged.
    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let result = unsafe { System.alloc(layout) };
            if !result.is_null() {
                record(layout.size(), false);
            }
            result
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let result = unsafe { System.alloc_zeroed(layout) };
            if !result.is_null() {
                record(layout.size(), false);
            }
            result
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe {
                System.dealloc(pointer, layout);
            }
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let result = unsafe { System.realloc(pointer, layout, size) };
            if !result.is_null() {
                record(size, true);
            }
            result
        }
    }
}
#[cfg(feature = "allocation-metrics")]
pub use enabled::*;

// No TLS or atomic operations are compiled into the default variant.
#[cfg(not(feature = "allocation-metrics"))]
mod disabled {
    use super::{Counts, Phase};
    pub struct PhaseGuard;
    impl Drop for PhaseGuard {
        fn drop(&mut self) {}
    }
    #[inline(always)]
    pub fn current_phase() -> Phase {
        Phase::Other
    }
    #[inline(always)]
    pub fn enter(_: Phase) -> PhaseGuard {
        PhaseGuard
    }
    pub fn snapshot() -> [Counts; 7] {
        [Counts::default(); 7]
    }
}
#[cfg(not(feature = "allocation-metrics"))]
pub use disabled::*;
