#[cfg(target_os = "linux")]
pub(super) fn read_proc(path: &str) -> Option<String> {
    use std::io::Read;
    const LIMIT: usize = 64 * 1024;
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take((LIMIT + 1) as u64)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() <= LIMIT).then_some(text)
}

#[derive(Clone, Copy)]
pub(super) struct ProcessMemory {
    pub(super) rss_bytes: u64,
    pub(super) private_bytes: Option<u64>,
}

#[cfg(target_os = "linux")]
pub(super) fn process_memory() -> Option<ProcessMemory> {
    fn kibibytes_field(source: &str, field: &str) -> Option<u64> {
        let line = source.lines().find(|line| line.starts_with(field))?;
        let mut values = line[field.len()..].split_whitespace();
        let value = values.next()?.parse::<u64>().ok()?;
        match values.next()? {
            "kB" => value.checked_mul(1024),
            "B" => Some(value),
            _ => None,
        }
    }

    let status = read_proc("/proc/self/status")?;
    Some(ProcessMemory {
        rss_bytes: kibibytes_field(&status, "VmRSS:")?,
        private_bytes: None,
    })
}

#[cfg(windows)]
pub(super) fn process_memory() -> Option<ProcessMemory> {
    use std::{ffi::c_void, mem::size_of};

    #[repr(C)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut ProcessMemoryCountersEx,
            size: u32,
        ) -> i32;
    }

    let size = u32::try_from(size_of::<ProcessMemoryCountersEx>()).ok()?;
    let mut counters = ProcessMemoryCountersEx {
        cb: size,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
        private_usage: 0,
    };
    // SAFETY: the pseudo-handle is borrowed; a valid initialized repr(C) output
    // structure and its exact byte size are passed for the duration of the call.
    let succeeded =
        unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) } != 0;
    succeeded.then_some(ProcessMemory {
        rss_bytes: counters.working_set_size as u64,
        private_bytes: Some(counters.private_usage as u64),
    })
}

#[cfg(not(any(target_os = "linux", windows)))]
pub(super) fn process_memory() -> Option<ProcessMemory> {
    None
}
