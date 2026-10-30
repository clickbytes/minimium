use super::{atomic_write_file, read_bounded_file};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary_directory() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "minimium-platform-files-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    directory
}

#[test]
fn bounded_reads_accept_the_limit_and_reject_one_byte_over() {
    let directory = temporary_directory();
    let path = directory.join("snapshot");
    fs::write(&path, b"1234").unwrap();
    assert_eq!(read_bounded_file(&path, 4).unwrap(), b"1234");
    fs::write(&path, b"12345").unwrap();
    assert_eq!(
        read_bounded_file(&path, 4).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn atomic_write_replaces_successfully_and_preserves_old_data_on_failure() {
    let directory = temporary_directory();
    let destination = directory.join("snapshot");
    let temporary = directory.join("snapshot.tmp");
    fs::write(&destination, b"old snapshot").unwrap();
    let missing_parent = directory.join("missing").join("snapshot.tmp");
    assert!(atomic_write_file(&missing_parent, &destination, b"new snapshot").is_err());
    assert_eq!(fs::read(&destination).unwrap(), b"old snapshot");

    atomic_write_file(&temporary, &destination, b"new snapshot").unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"new snapshot");
    assert!(!temporary.exists());
    fs::remove_dir_all(directory).unwrap();
}
