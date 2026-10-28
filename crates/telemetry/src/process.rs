use std::time::{Duration, Instant};

/// Cumulative process CPU time (all OS threads) and instantaneous process resources.
#[derive(Clone, Copy, Default)]
pub struct ProcessSample {
    pub rss_bytes: Option<u64>,
    pub private_bytes: Option<u64>,
    pub cpu_time: Option<Duration>,
    pub threads: Option<u32>,
}

#[derive(Default)]
pub struct ProcessSampler {
    sampled_at: Option<Instant>,
    sample: ProcessSample,
}

impl ProcessSampler {
    /// Cached for one second; failed measurements remain explicitly unavailable.
    pub fn sample(&mut self, now: Instant) -> ProcessSample {
        if self
            .sampled_at
            .is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_secs(1))
        {
            self.sampled_at = Some(now);
            let memory = crate::memory::process_memory();
            let (cpu_time, threads) = process_execution();
            self.sample = ProcessSample {
                rss_bytes: memory.map(|value| value.rss_bytes),
                private_bytes: memory.and_then(|value| value.private_bytes),
                cpu_time,
                threads,
            };
        }
        self.sample
    }
}

#[cfg(target_os = "linux")]
fn process_execution() -> (Option<Duration>, Option<u32>) {
    use std::os::raw::{c_int, c_long};
    unsafe extern "C" {
        fn sysconf(name: c_int) -> c_long;
    }
    // Linux _SC_CLK_TCK. sysconf has no pointer arguments or ownership requirements.
    let hz = unsafe { sysconf(2) };
    let cpu = (|| {
        let stat = crate::memory::read_proc("/proc/self/stat")?;
        // comm (field 2) may contain spaces/parentheses; split after its final ')'.
        let mut fields = stat.rsplit_once(')')?.1.split_whitespace();
        let user = fields.nth(11)?.parse::<u64>().ok()?; // field 14
        let kernel = fields.next()?.parse::<u64>().ok()?; // field 15
        let hz = u64::try_from(hz).ok().filter(|hz| *hz > 0)?;
        let ticks = user.checked_add(kernel)?;
        Some(Duration::new(
            ticks / hz,
            ((ticks % hz) as u128 * 1_000_000_000 / hz as u128) as u32,
        ))
    })();
    let threads = crate::memory::read_proc("/proc/self/status").and_then(|status| {
        status
            .lines()
            .find_map(|line| line.strip_prefix("Threads:")?.trim().parse().ok())
    });
    (cpu, threads)
}

#[cfg(windows)]
fn process_execution() -> (Option<Duration>, Option<u32>) {
    use std::{ffi::c_void, mem::size_of};
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    impl FileTime {
        fn ticks(&self) -> u64 {
            (u64::from(self.high) << 32) | u64::from(self.low)
        }
    }
    #[repr(C)]
    struct ProcessEntry {
        size: u32,
        usage: u32,
        process_id: u32,
        default_heap: usize,
        module_id: u32,
        threads: u32,
        parent_id: u32,
        priority: i32,
        flags: u32,
        executable: [u16; 260],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessTimes(
            process: *mut c_void,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> *mut c_void;
        fn Process32FirstW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
        fn Process32NextW(snapshot: *mut c_void, entry: *mut ProcessEntry) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    let (mut creation, mut exit, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    // SAFETY: repr(C) FILETIME buffers are writable, disjoint, and live for the call.
    // GetCurrentProcess returns a borrowed pseudo-handle which must not be closed.
    let success = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } != 0;
    let cpu = success.then(|| {
        Duration::from_nanos(
            kernel
                .ticks()
                .saturating_add(user.ticks())
                .saturating_mul(100),
        )
    });
    // TH32CS_SNAPPROCESS; successful snapshot handles are closed exactly once below.
    let snapshot = unsafe { CreateToolhelp32Snapshot(2, 0) };
    if snapshot as isize == -1 {
        return (cpu, None);
    }
    let mut entry = ProcessEntry {
        size: size_of::<ProcessEntry>() as u32,
        usage: 0,
        process_id: 0,
        default_heap: 0,
        module_id: 0,
        threads: 0,
        parent_id: 0,
        priority: 0,
        flags: 0,
        executable: [0; 260],
    };
    let mut threads = None;
    // SAFETY: initialized correctly sized repr(C) PROCESSENTRY32W and valid snapshot.
    let mut available = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while available {
        if entry.process_id == std::process::id() {
            threads = Some(entry.threads);
            break;
        }
        available = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
    (cpu, threads)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn process_execution() -> (Option<Duration>, Option<u32>) {
    (None, None)
}
