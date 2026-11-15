use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

const ACTIVE_LOCK: &str = ".active.lock";
const RUN_MARKER: &str = ".minimium-run";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const RETENTION_DAYS: i64 = 7;

pub(super) struct RunDirectory {
    log_root: PathBuf,
    start_day: i64,
    pub path: PathBuf,
    pub run_id: String,
    pub parent_run_id: Option<String>,
    _active_lock: File,
}

impl RunDirectory {
    pub fn create(root: &Path, parent_run_id: Option<String>) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let start_utc_ms = crate::logging::unix_time_ms()?;
        let start_day = (start_utc_ms / 86_400_000) as i64;
        if parent_run_id
            .as_deref()
            .is_some_and(|value| !valid_run_id(value))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "parent run identifier is not a valid diagnostics run id",
            ));
        }
        let (year, month, day) = civil_from_days_public(start_day);
        let date = format!("{year:04}-{month:02}-{day:02}");
        let date_path = root.join(&date);
        fs::create_dir_all(&date_path)?;

        let mut sequence = 1_u64;
        let (path, lock) = loop {
            let candidate = date_path.join(format!("{sequence:06}"));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    let lock = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .create_new(true)
                        .open(candidate.join(ACTIVE_LOCK))?;
                    lock.try_lock().map_err(|error| match error {
                        std::fs::TryLockError::WouldBlock => io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "new diagnostics run directory was unexpectedly locked",
                        ),
                        std::fs::TryLockError::Error(error) => error,
                    })?;
                    let marker_path = candidate.join(RUN_MARKER);
                    let mut marker = File::create(marker_path)?;
                    writeln!(marker, "minimium diagnostics run v1")?;
                    marker.sync_all()?;
                    break (candidate, lock);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    sequence = sequence
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("diagnostics run sequence exhausted"))?;
                }
                Err(error) => return Err(error),
            }
        };
        cleanup_completed_runs(root, start_day, &path)?;
        Ok(Self {
            log_root: root.to_path_buf(),
            start_day,
            run_id: format!("{date}/{sequence:06}"),
            path,
            parent_run_id,
            _active_lock: lock,
        })
    }

    pub fn cleanup(&self) -> io::Result<()> {
        cleanup_completed_runs(&self.log_root, self.start_day, &self.path)
    }
}

fn cleanup_completed_runs(root: &Path, today: i64, own_path: &Path) -> io::Result<()> {
    let mut total_size = directory_size(own_path)?;
    let mut removable = Vec::new();
    for date_entry in fs::read_dir(root)? {
        let date_entry = date_entry?;
        if !date_entry.file_type()?.is_dir() {
            continue;
        }
        let date_name = date_entry.file_name();
        let Some(date_text) = date_name.to_str() else {
            continue;
        };
        let Some(date_days) = parse_date(date_text) else {
            continue;
        };
        for run_entry in fs::read_dir(date_entry.path())? {
            let run_entry = run_entry?;
            if !run_entry.file_type()?.is_dir() {
                continue;
            }
            let run_path = run_entry.path();
            if run_path == own_path
                || !run_entry
                    .file_name()
                    .to_string_lossy()
                    .bytes()
                    .all(|b| b.is_ascii_digit())
            {
                continue;
            }
            if !run_path.join(RUN_MARKER).is_file() {
                continue;
            }
            let size = directory_size(&run_path)?;
            let Some(active) = try_acquire_run_lock(&run_path)? else {
                total_size = total_size.saturating_add(size);
                continue;
            };
            if date_days <= today - RETENTION_DAYS {
                fs::remove_dir_all(&run_path)?;
                drop(active);
            } else {
                total_size = total_size.saturating_add(size);
                removable.push((date_text.to_owned(), run_path, size, active));
            }
        }
    }
    removable.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    for (_, path, size, active) in removable {
        if total_size <= MAX_LOG_BYTES {
            break;
        }
        fs::remove_dir_all(path)?;
        drop(active);
        total_size = total_size.saturating_sub(size);
    }
    Ok(())
}

fn try_acquire_run_lock(run_path: &Path) -> io::Result<Option<File>> {
    let lock = match OpenOptions::new()
        .read(true)
        .write(true)
        .open(run_path.join(ACTIVE_LOCK))
    {
        Ok(lock) => lock,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    match lock.try_lock() {
        Ok(()) => Ok(Some(lock)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

fn directory_size(root: &Path) -> io::Result<u64> {
    let mut total = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            total = total.saturating_add(directory_size(&entry.path())?);
        } else if kind.is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}

fn parse_date(value: &str) -> Option<i64> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let result = days_from_civil(year, month, day);
    let (round_year, round_month, round_day) = civil_from_days_public(result);
    (year == round_year && month == i64::from(round_month) && day == i64::from(round_day))
        .then_some(result)
}

fn valid_run_id(value: &str) -> bool {
    value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-' || byte == b'/')
        && value.contains('/')
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let adjusted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub(super) fn civil_from_days_public(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}
