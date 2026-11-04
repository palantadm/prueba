//! Audio I/O: device selection, channel routing, sound playback, LTC and audio-reactive analysis.
//!
//! * [`dsp`]   – level / bass-mid-treble / beat + tempo from an input channel (pure).
//! * [`ltc`]   – SMPTE LTC encoder and decoder (pure).
//! * [`mixer`] – output matrix: sounds, timeline audio and LTC → device channels (pure).
//! * [`io`]    – shared state + input processing (always) and the device layer (cpal) / file decoding (symphonia) with the `audio` feature.
//!
//! The engine owns it (`engine/audio.rs`): commands, runtime state, timecode source `ltc`, bindings from the
//! reactive values and the voice-follow microphone feed.

pub mod dsp;
pub mod ltc;
pub mod mixer;

pub mod io;

/// Whether this build can open audio devices.
pub const AVAILABLE: bool = cfg!(feature = "audio");

/// Audio file extensions the sound library lists.
pub const SOUND_EXTS: &[&str] = &["wav", "mp3", "flac", "ogg", "m4a", "aac", "aif", "aiff"];

pub fn is_sound(name: &str) -> bool {
    std::path::Path::new(name).extension().and_then(|e| e.to_str()).map_or(false, |e| SOUND_EXTS.contains(&e.to_lowercase().as_str()))
}

/// Sound files (names only) in a media folder, sorted.
pub fn list_sounds(media: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(media).map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| is_sound(n)).collect()).unwrap_or_default();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_files_are_recognised() {
        assert!(is_sound("gong.WAV") && is_sound("a.b.mp3") && !is_sound("plano.pdf") && !is_sound("wav"));
        let d = std::env::temp_dir().join(format!("lumos-snd-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for f in ["b.mp3", "a.wav", "x.png"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        assert_eq!(list_sounds(&d), vec!["a.wav", "b.mp3"]);
    }
}
