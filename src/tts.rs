use anyhow::{anyhow, Result};

use windows::{
    core::ComInterface,
    Foundation::TypedEventHandler,
    Media::{
        Core::{MediaSource, SpeechCue},
        Playback::{
            MediaPlaybackItem, MediaPlayer, MediaPlayerAudioCategory,
            TimedMetadataTrackPresentationMode,
        },
        SpeechSynthesis::SpeechSynthesizer,
    },
};

pub use windows::Media::SpeechSynthesis::VoiceInformation;

pub struct Tts {
    synth: SpeechSynthesizer,
    player: MediaPlayer,
    playback_rate: f64,
}

impl Tts {
    pub fn new(callback_end: Option<impl Fn() + Send + 'static>) -> Result<Self> {
        let synth = SpeechSynthesizer::new()?;

        let player = MediaPlayer::new()?;
        player.SetRealTimePlayback(true)?;
        player.SetAudioCategory(MediaPlayerAudioCategory::Speech)?;

        let options = synth.Options()?;
        options.SetIncludeWordBoundaryMetadata(true)?;
        options.SetIncludeSentenceBoundaryMetadata(false)?;

        if let Some(callback) = callback_end {
            player.MediaEnded(&TypedEventHandler::new(move |_, _| {
                callback();
                Ok(())
            }))?;
        }

        Ok(Tts {
            synth,
            player,
            playback_rate: 1.0,
        })
    }

    pub fn voice(&self) -> Result<VoiceInformation> {
        self.synth.Voice().map_err(|e| anyhow!(e))
    }

    pub fn change_voice(&mut self, voice: &VoiceInformation) -> Result<()> {
        self.synth.SetVoice(voice)?;
        Ok(())
    }

    pub fn speak(
        &mut self,
        text: String,
        callback_word_boundary: Option<impl Fn() + Send + Sync + 'static>,
    ) -> anyhow::Result<Vec<String>> {

        self.stop()?;

        let stream = self
            .synth
            .SynthesizeTextToStreamAsync(&text.into())?
            .get()?;

        let cues = stream.TimedMetadataTracks()?.GetAt(0)?.Cues()?;
        let cue_len = cues.Size()?;

        let word_vec: Vec<String> = (0..cue_len)
            .filter_map(|i| {
                let cue = cues.GetAt(i).unwrap();
                let cue: Option<SpeechCue> = cue.cast().ok();
                cue.and_then(|cue| cue.Text().ok()).map(|t| t.to_string())
            })
            .collect();

        let content_type = stream.ContentType()?;
        let media_source = MediaSource::CreateFromStream(&stream, &content_type)?;
        let playback = MediaPlaybackItem::Create(&media_source)?;

        if let Some(callback) = callback_word_boundary {
            let metadata = playback.TimedMetadataTracks()?;
            metadata
                .SetPresentationMode(0, TimedMetadataTrackPresentationMode::ApplicationPresented)?;
            let track = metadata.GetAt(0)?;

            track.CueEntered(&TypedEventHandler::new(move |_, _| {
                callback();
                Ok(())
            }))?;
        }

        self.player.SetSource(&playback)?;
        self.set_rate(self.playback_rate)?;

        self.player.Play()?;

        Ok(word_vec)
    }

    pub fn stop(&mut self) -> Result<()> {
        self.pause()?;
        self.player.SetSource(None)?;

        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        self.player.Pause()?;

        Ok(())
    }

    pub fn list_voices(&self) -> Result<Vec<VoiceInformation>> {
        let mut result: Vec<(VoiceInformation, String)> = SpeechSynthesizer::AllVoices()?
            .into_iter()
            .filter_map(|v| v.Language().ok().map(|l| (v, l.to_string())))
            .collect();

        result.sort_by_cached_key(|(_, l)| l.clone());

        let result = result.into_iter().map(|(v, _)| v).collect();

        Ok(result)
    }

    pub fn resume(&mut self) -> Result<()> {
        self.player.Play()?;

        Ok(())
    }

    pub fn set_volume(&mut self, volume: f64) -> Result<()> {
        self.player.SetVolume(volume)?;

        Ok(())
    }

    pub fn set_rate(&mut self, rate: f64) -> Result<()> {
        self.playback_rate = rate;
        self.player.PlaybackSession()?.SetPlaybackRate(rate)?;

        Ok(())
    }
}
