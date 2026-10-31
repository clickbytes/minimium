#[cfg(windows)]
pub(super) fn schedule(id: u64, delay_seconds: f32) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs_f32(delay_seconds.max(0.0)));
        let sound = thunder_wave(id);
        // SND_MEMORY | SND_NODEFAULT; a worker owns the buffer until playback ends.
        unsafe {
            PlaySoundW(sound.as_ptr(), std::ptr::null_mut(), 0x0004 | 0x0002);
        }
    });
}

#[cfg(not(windows))]
pub(super) fn schedule(_id: u64, _delay_seconds: f32) {}

#[cfg(windows)]
fn thunder_wave(id: u64) -> Vec<u16> {
    const RATE: u32 = 22_050;
    const SAMPLES: usize = RATE as usize * 2;
    let mut bytes = Vec::with_capacity(44 + SAMPLES * 2);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + SAMPLES as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(SAMPLES as u32 * 2).to_le_bytes());
    let mut random = id | 1;
    let mut low = 0.0f32;
    for index in 0..SAMPLES {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let noise = ((random >> 32) as u32 as f32 / u32::MAX as f32) * 2.0 - 1.0;
        low = low * 0.965 + noise * 0.035;
        let time = index as f32 / RATE as f32;
        let attack = (time * 30.0).min(1.0);
        let decay = (-time * 1.8).exp();
        let roll = 0.65 + 0.35 * (time * 13.0).sin().abs();
        let sample = (low * 5.0 * attack * decay * roll).clamp(-1.0, 1.0);
        bytes.extend_from_slice(&((sample * i16::MAX as f32) as i16).to_le_bytes());
    }
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}

#[cfg(windows)]
#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundW(sound: *const u16, module: *mut std::ffi::c_void, flags: u32) -> i32;
}
