use super::run::RunDirectory;
use minimium_diagnostics::{DiagnosticEvent, DomainId, Level, MAX_EVENT_BYTES};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const MAX_SEGMENT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PREVIOUS_SEGMENTS: usize = 2;
const MAX_RECORD_BYTES: usize = MAX_EVENT_BYTES;

struct DomainFile {
    base_path: PathBuf,
    file: Option<File>,
    bytes: u64,
}

pub(super) struct RunWriter {
    run: RunDirectory,
    started: Instant,
    sequence: u64,
    files: HashMap<DomainId, DomainFile>,
}

impl RunWriter {
    pub fn new(run: RunDirectory) -> Self {
        Self {
            run,
            started: Instant::now(),
            sequence: 0,
            files: HashMap::new(),
        }
    }

    pub fn write_event(&mut self, event: &DiagnosticEvent) -> io::Result<()> {
        let fields = event
            .fields()
            .iter()
            .map(|field| (field.key.as_str(), field.value.clone()))
            .collect::<Vec<_>>();
        self.write_record(
            event.domain(),
            event.level(),
            event.code().as_str(),
            fields,
            event.truncated(),
        )
    }

    pub fn write_loss_summary(&mut self, normal: u64, priority: u64) -> io::Result<()> {
        self.write_record(
            DomainId::Platform,
            Level::Warn,
            "telemetry.records_dropped",
            vec![
                ("normal_dropped", normal.to_string()),
                ("priority_dropped", priority.to_string()),
            ],
            false,
        )
    }

    pub fn flush(&mut self) -> io::Result<()> {
        for domain_file in self.files.values_mut() {
            if let Some(file) = domain_file.file.as_mut() {
                file.flush()?;
            }
        }
        Ok(())
    }

    fn write_record(
        &mut self,
        domain: DomainId,
        level: Level,
        code: &str,
        fields: Vec<(&str, String)>,
        event_truncated: bool,
    ) -> io::Result<()> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("diagnostic event sequence exhausted"))?;
        let now_ms = super::unix_time_ms()?;
        let elapsed_ms = self.started.elapsed().as_millis();
        let (year, month, day, hour, minute, second, millis) = utc_parts(now_ms);
        let timestamp =
            format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z");
        let mut line = format!(
            "{{\"timestamp_utc\":{},\"elapsed_ms\":{},\"event_seq\":{},\"domain\":{},\"level\":{},\"code\":{},\"process_id\":{},\"run_id\":{},\"parent_run_id\":{},\"fields\":{{",
            json_string(&timestamp),
            elapsed_ms,
            self.sequence,
            json_string(domain.as_str()),
            json_string(level.as_str()),
            json_string(code),
            std::process::id(),
            json_string(&self.run.run_id),
            self.run
                .parent_run_id
                .as_deref()
                .map(json_string)
                .unwrap_or_else(|| "null".to_owned()),
        );
        let mut truncated = event_truncated;
        let mut first = true;
        for (key, value) in fields {
            let separator_bytes = usize::from(!first);
            let fragment_bytes = separator_bytes
                .saturating_add(json_string_encoded_len(key))
                .saturating_add(1)
                .saturating_add(json_string_encoded_len(&value));
            if line.len().saturating_add(fragment_bytes).saturating_add(32) > MAX_RECORD_BYTES {
                truncated = true;
                break;
            }
            let fragment = format!(
                "{}{}:{}",
                if first { "" } else { "," },
                json_string(key),
                json_string(&value)
            );
            line.push_str(&fragment);
            first = false;
        }
        line.push_str(if truncated {
            "},\"truncated\":true}\n"
        } else {
            "},\"truncated\":false}\n"
        });
        if line.len() > MAX_RECORD_BYTES {
            return Err(io::Error::other("diagnostic JSON record exceeded 8 KiB"));
        }
        self.write_domain_line(domain, line.as_bytes())
    }

    fn write_domain_line(&mut self, domain: DomainId, line: &[u8]) -> io::Result<()> {
        if !self.files.contains_key(&domain) {
            let path = self.run.path.join(format!("{}.log", domain.as_str()));
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            let bytes = file.metadata()?.len();
            self.files.insert(
                domain,
                DomainFile {
                    base_path: path,
                    file: Some(file),
                    bytes,
                },
            );
        }
        let file = self.files.get_mut(&domain).expect("domain file inserted");
        if file.bytes.saturating_add(line.len() as u64) > MAX_SEGMENT_BYTES {
            rotate(file)?;
            self.run.cleanup()?;
        }
        file.file
            .as_mut()
            .expect("domain log file is open")
            .write_all(line)?;
        file.bytes = file.bytes.saturating_add(line.len() as u64);
        Ok(())
    }
}

fn rotate(domain_file: &mut DomainFile) -> io::Result<()> {
    if let Some(mut file) = domain_file.file.take() {
        file.flush()?;
    }
    let oldest = segment_path(&domain_file.base_path, MAX_PREVIOUS_SEGMENTS);
    if oldest.exists() {
        fs::remove_file(&oldest)?;
    }
    let first = segment_path(&domain_file.base_path, 1);
    if first.exists() {
        fs::rename(&first, &oldest)?;
    }
    fs::rename(&domain_file.base_path, &first)?;
    domain_file.file = Some(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&domain_file.base_path)?,
    );
    domain_file.bytes = 0;
    Ok(())
}

fn segment_path(base: &Path, index: usize) -> PathBuf {
    let name = base
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    base.with_file_name(format!("{name}.{index}"))
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len().min(MAX_RECORD_BYTES));
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character <= '\u{1f}' => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn json_string_encoded_len(value: &str) -> usize {
    value.chars().fold(2_usize, |length, character| {
        length.saturating_add(match character {
            '"' | '\\' | '\n' | '\r' | '\t' => 2,
            character if character <= '\u{1f}' => 6,
            character => character.len_utf8(),
        })
    })
}

fn utc_parts(milliseconds: u128) -> (i64, u32, u32, u32, u32, u32, u32) {
    let days = (milliseconds / 86_400_000) as i64;
    let daytime = (milliseconds % 86_400_000) as u64;
    let (year, month, day) = super::run::civil_from_days_public(days);
    (
        year,
        month,
        day,
        (daytime / 3_600_000) as u32,
        ((daytime / 60_000) % 60) as u32,
        ((daytime / 1_000) % 60) as u32,
        (daytime % 1_000) as u32,
    )
}
