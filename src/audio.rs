use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
use std::fs::File;
use std::io::Cursor;
use std::path::PathBuf;

const EMBEDDED_TICK_WAV: &[u8] = include_bytes!("../tick.wav");
const EMBEDDED_START_TICK_WAV: &[u8] = include_bytes!("../start_tick.wav");

pub struct AudioManager {
    device_sink: Option<MixerDeviceSink>,
    player: Option<Player>,
    tick_bytes: Option<Vec<u8>>,
    start_tick_bytes: Option<Vec<u8>>,
    current_mp3_path: Option<PathBuf>,
}

impl AudioManager {
    pub fn new() -> Self {
        let (device_sink, player) = match DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let player = Player::connect_new(&sink.mixer());
                (Some(sink), Some(player))
            }
            Err(e) => {
                eprintln!("Warning: Could not open default audio sink: {}", e);
                (None, None)
            }
        };

        let mut mgr = Self {
            device_sink,
            player,
            tick_bytes: None,
            start_tick_bytes: None,
            current_mp3_path: None,
        };
        mgr.load_default_sounds();
        mgr
    }

    pub fn load_default_sounds(&mut self) {
        let mut search_dirs = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            search_dirs.push(cwd);
        }
        if let Ok(exe_path) = std::env::current_exe() {
            let mut curr = exe_path.parent();
            while let Some(parent) = curr {
                search_dirs.push(parent.to_path_buf());
                curr = parent.parent();
            }
        }

        for dir in &search_dirs {
            let tick_path = dir.join("tick.wav");
            let start_tick_path = dir.join("start_tick.wav");

            if self.tick_bytes.is_none() && tick_path.exists() {
                if let Ok(bytes) = std::fs::read(&tick_path) {
                    println!("Loaded tick sound from {:?}", tick_path);
                    self.tick_bytes = Some(bytes);
                }
            }

            if self.start_tick_bytes.is_none() && start_tick_path.exists() {
                if let Ok(bytes) = std::fs::read(&start_tick_path) {
                    println!("Loaded start tick sound from {:?}", start_tick_path);
                    self.start_tick_bytes = Some(bytes);
                }
            }
        }

        if self.tick_bytes.is_none() {
            println!("Using embedded fallback for tick sound");
            self.tick_bytes = Some(EMBEDDED_TICK_WAV.to_vec());
        }

        if self.start_tick_bytes.is_none() {
            println!("Using embedded fallback for start tick sound");
            self.start_tick_bytes = Some(EMBEDDED_START_TICK_WAV.to_vec());
        }
    }

    pub fn play_tick(&self) {
        if let (Some(sink), Some(bytes)) = (&self.device_sink, &self.tick_bytes) {
            if let Ok(decoder) = Decoder::try_from(Cursor::new(bytes.clone())) {
                sink.mixer().add(decoder);
            }
        }
    }

    pub fn play_start_tick(&self) {
        if let (Some(sink), Some(bytes)) = (&self.device_sink, &self.start_tick_bytes) {
            if let Ok(decoder) = Decoder::try_from(Cursor::new(bytes.clone())) {
                sink.mixer().add(decoder);
            }
        }
    }

    pub fn set_mp3_path(&mut self, path: Option<PathBuf>) {
        self.stop_mp3();
        self.current_mp3_path = path;
    }

    pub fn current_mp3_path(&self) -> Option<&PathBuf> {
        self.current_mp3_path.as_ref()
    }

    pub fn play_mp3(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
            player.clear();
            if let Some(path) = &self.current_mp3_path {
                match File::open(path) {
                    Ok(file) => match Decoder::try_from(file) {
                        Ok(decoder) => {
                            player.append(decoder);
                            player.play();
                            println!("Playing MP3: {:?}", path);
                        }
                        Err(e) => eprintln!("Error decoding MP3 {:?}: {}", path, e),
                    },
                    Err(e) => eprintln!("Error opening MP3 {:?}: {}", path, e),
                }
            }
        }
    }

    pub fn pause_mp3(&self) {
        if let Some(player) = &self.player {
            player.pause();
        }
    }

    pub fn resume_mp3(&self) {
        if let Some(player) = &self.player {
            player.play();
        }
    }

    pub fn stop_mp3(&mut self) {
        if let Some(player) = &self.player {
            player.stop();
            player.clear();
        }
    }

    pub fn seek_mp3(&mut self, pos: std::time::Duration) {
        if let Some(player) = &self.player {
            if player.empty() {
                // If player is currently empty but we have an MP3 path, load it first
                if let Some(path) = &self.current_mp3_path {
                    if let Ok(file) = File::open(path) {
                        if let Ok(decoder) = Decoder::try_from(file) {
                            player.append(decoder);
                            player.pause();
                        }
                    }
                }
            }
            if let Err(e) = player.try_seek(pos) {
                eprintln!("Error seeking MP3 to {:?}: {:?}", pos, e);
            } else {
                println!("Seeked MP3 to {:?}", pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_embedded_and_loaded_sounds() {
        let mgr = AudioManager::new();
        assert!(mgr.tick_bytes.is_some(), "tick_bytes must be loaded");
        assert!(mgr.start_tick_bytes.is_some(), "start_tick_bytes must be loaded");

        // Verify that embedded sound bytes can be decoded properly
        let tick_dec = Decoder::try_from(Cursor::new(EMBEDDED_TICK_WAV.to_vec()));
        assert!(tick_dec.is_ok(), "Embedded tick WAV must be decodable");

        let start_dec = Decoder::try_from(Cursor::new(EMBEDDED_START_TICK_WAV.to_vec()));
        assert!(start_dec.is_ok(), "Embedded start tick WAV must be decodable");
    }

    #[test]
    fn test_mp3_seek() {
        if let Ok(file) = File::open("Free.mp3") {
            if let Ok(decoder) = Decoder::try_from(file) {
                let mut handle = DeviceSinkBuilder::open_default_sink().unwrap();
                handle.log_on_drop(false);
                let player = Player::connect_new(&handle.mixer());
                player.append(decoder);
                player.pause();
                let res = player.try_seek(Duration::from_secs(15));
                println!("Seek result: {:?}", res);
                assert!(res.is_ok());
            }
        }
    }
}

