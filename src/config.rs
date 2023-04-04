use std::fs::{self};

use anyhow::{anyhow, Context};
use configparser::ini::Ini;

#[derive(Debug, Clone)]
pub struct TtsConfig {
    voice: String,
    volume: f64,
    rate: f64,
}

impl TtsConfig {
    pub fn set_voice(&mut self, voice: String) {
        self.voice = voice;
    }

    pub fn set_volume(&mut self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.volume = volume;
    }

    pub fn set_rate(&mut self, rate: f64) {
        let rate = rate.clamp(0.2, 2.0);
        let rate: u64 = (rate * 10.0).round() as u64;
        let rate = rate as f64 / 10.0;

        self.rate = rate;
    }

    pub fn new(voice: String, volume: f64, rate: f64) -> Self {
        let mut result: Self = Default::default();

        result.set_rate(rate);
        result.set_volume(volume);
        result.set_voice(voice);

        result
    }

    pub fn voice(&self) -> &str {
        self.voice.as_ref()
    }

    pub fn volume(&self) -> f64 {
        self.volume
    }

    pub fn rate(&self) -> f64 {
        self.rate
    }
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            voice: "default".into(),
            volume: 1.0,
            rate: 1.0,
        }
    }
}

const FOLDER_NAME: &str = ".qtts";
const FILE_NAME: &str = "config.ini";

const VOICE_KEY: &str = "voice";
const VOLUME_KEY: &str = "volume";
const RATE_KEY: &str = "rate";

const DEFAULT: &str = "default";

pub fn load_config() -> anyhow::Result<TtsConfig> {
    let path = dirs::config_dir().context("Config dir not found")?;
    let path = path.join(FOLDER_NAME).join(FILE_NAME);

    if !path.is_file() {
        anyhow::bail!("File does not exist!");
    }

    let mut config = Ini::new();
    let _map = config.load(path).map_err(|e| anyhow!(e))?;

    let voice = config.get(DEFAULT, VOICE_KEY).unwrap_or("default".into());
    let volume = config
        .getfloat(DEFAULT, VOLUME_KEY)
        .ok()
        .flatten()
        .unwrap_or(1.0);
    let rate = config
        .getfloat(DEFAULT, RATE_KEY)
        .ok()
        .flatten()
        .unwrap_or(1.0);

    Ok(TtsConfig::new(voice, volume, rate))
}

pub fn save_config(
    TtsConfig {
        voice,
        volume,
        rate,
    }: TtsConfig,
) -> anyhow::Result<()> {
    let folder_path = dirs::config_dir().context("Config dir not found")?;
    let folder_path = folder_path.join(FOLDER_NAME);

    fs::create_dir_all(&folder_path)?;

    let file_path = folder_path.join(FILE_NAME);

    let mut config = Ini::new();

    config.set(DEFAULT, VOICE_KEY, Some(voice));
    config.set(DEFAULT, RATE_KEY, Some(rate.to_string()));
    config.set(DEFAULT, VOLUME_KEY, Some(volume.to_string()));

    config.write(file_path)?;

    Ok(())
}
