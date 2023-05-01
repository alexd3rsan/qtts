#![windows_subsystem = "windows"]

mod config;
mod tts;

use std::{path::PathBuf, str::FromStr};

use clipboard_win::{formats, get_clipboard};
use config::{load_config, save_config, TtsConfig};
use fltk::{
    app::{self, TimeoutHandle},
    button::{Button, ToggleButton},
    enums::{Color, Event, FrameType, Shortcut},
    frame::Frame,
    group::Pack,
    menu::Choice,
    prelude::*,
    valuator::HorValueSlider,
    window::{RawHandle, Window},
};
use fltk_theme::{ThemeType, WidgetTheme};
use tts::{Tts, VoiceInformation};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTCOLOR, WM_SETICON,
        },
    },
};

const CHOOSER_HEIGHT: i32 = 50;

const WINDOW_W: i32 = 300;
const WINDOW_H: i32 = 450;

const BTN_PAD_X: i32 = 20;
const WIDGET_W: i32 = WINDOW_W - BTN_PAD_X;

const PAUSE_STR: &str = "Pause";
const RESUME_STR: &str = "Resume";
const PLAY_STR: &str = "Start";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TTSCommand {
    TogglePlayPause,
    Stop,
    VolumeChanged,
    PlaybackRateChanged,
    PlaybackRateSet,
    SpeakerChanged,
    WordBoundaryAdvanced,
    DraggedFile,
}

fn main() -> anyhow::Result<()> {
    let app = app::App::default();

    let widget_theme = WidgetTheme::new(ThemeType::Metro);
    widget_theme.apply();

    app::set_visible_focus(true);

    let mut wind = Window::default()
        .with_label("Quick Text-To-Speech")
        .with_size(WINDOW_W, WINDOW_H)
        .center_screen();

    wind.make_resizable(false);

    let (s, r) = app::channel::<TTSCommand>();

    let mut tts = Tts::new(Some(move || {
        s.send(TTSCommand::Stop);
    }))
    .map_err(|e| {
        fltk::dialog::alert_default(&e.to_string());
        e
    })?;

    let speakers = tts.list_voices().map_err(|e| {
        fltk::dialog::alert_default("Failed trying to read installed voices.");
        e
    })?;

    let mut config = load_config().ok().unwrap_or_default();

    let (voice_labels, last_voice_index) = load_voice_labels(&speakers, &config);

    tts.set_rate(config.rate())?;
    tts.set_volume(config.volume())?;
    if let Some(voice) = speakers.get(last_voice_index.unwrap_or(0)) {
        tts.change_voice(voice)?;
    };

    let mut vpack = Pack::default()
        .with_size(WIDGET_W, WINDOW_H - BTN_PAD_X)
        .center_of_parent();

    let mut speaker_chooser = init_speaker_chooser(&voice_labels, last_voice_index)?;
    let mut word_display = init_word_display();
    let mut play_pause_btn = init_play_pause_btn();
    let mut stop_btn = init_stop_btn();
    let mut vol_sld = init_volume_sld(&config);
    let mut rate_sld = init_rate_sld(&config);

    vpack.end();
    vpack.set_spacing(20);

    wind.end();
    wind.show();

    wind.handle(move |_, e| match e {
        Event::DndEnter => true,
        Event::DndLeave => true,
        Event::DndRelease => true,
        Event::DndDrag => true,
        Event::Paste => {
            s.send(TTSCommand::DraggedFile);
            true
        }
        _ => false,
    });

    // must be called after wind.show() to work!
    init_icons(wind.raw_handle())?;

    vol_sld.emit(s, TTSCommand::VolumeChanged);
    rate_sld.emit(s, TTSCommand::PlaybackRateChanged);
    play_pause_btn.emit(s, TTSCommand::TogglePlayPause);
    stop_btn.emit(s, TTSCommand::Stop);
    speaker_chooser.emit(s, TTSCommand::SpeakerChanged);

    let mut reached_end = true;
    let mut word_cnt = 0;
    let mut word_vec: Vec<String> = Vec::new();

    let mut handle: Option<TimeoutHandle> = None;

    while app.wait() {
        if let Some(val) = r.recv() {
            match val {
                TTSCommand::TogglePlayPause => {
                    let was_playing = play_pause_btn.is_toggled();

                    if was_playing {
                        let text: String = get_clipboard(formats::Unicode).unwrap_or_default();

                        if text.is_empty() {
                            play_pause_btn.set(false);
                            continue;
                        }

                        if reached_end {
                            reached_end = false;

                            let callback = move || {
                                s.send(TTSCommand::WordBoundaryAdvanced);
                            };
                            if let Ok(words) = tts.speak(text, Some(callback)) {
                                if words.is_empty() {
                                    continue;
                                }
                                word_vec = words;
                                let label = word_vec.get(0).map(|s| s.as_str()).unwrap_or("");
                                word_display.set_label(label);
                            }
                        } else {
                            tts.resume().ok();
                        }

                        stop_btn.activate();
                        play_pause_btn.set_label(PAUSE_STR);
                        speaker_chooser.deactivate();
                    } else {
                        tts.pause().ok();
                        play_pause_btn.set_label(RESUME_STR)
                    }

                    wind.redraw();
                }

                TTSCommand::DraggedFile => {
                    let text: String = read_files_from_drop().unwrap_or_default();

                    if text.is_empty() {
                        continue;
                    }

                    let callback = move || {
                        s.send(TTSCommand::WordBoundaryAdvanced);
                    };
                    if let Ok(words) = tts.speak(text, Some(callback)) {
                        if words.is_empty() {
                            continue;
                        }

                        word_cnt = 0;
                        play_pause_btn.set(true);
                        reached_end = false;

                        word_vec.clear();
                        word_vec = words;

                        let label = word_vec.get(0).map(|s| s.as_str()).unwrap_or("");
                        word_display.set_label(label);

                        wind.redraw();
                    }

                    stop_btn.activate();
                    play_pause_btn.set_label(PAUSE_STR);
                    speaker_chooser.deactivate();
                }
                TTSCommand::SpeakerChanged => {
                    if let Some(sp) = speakers.get(speaker_chooser.value() as usize) {
                        tts.change_voice(sp).ok();

                        if let Ok(name) = sp.DisplayName() {
                            config.set_voice(name.to_string());
                        }
                    };
                }
                TTSCommand::Stop => {
                    if tts.stop().is_ok() {
                        word_vec.clear();
                        word_cnt = 0;
                        reached_end = true;
                        play_pause_btn.set(false);
                        play_pause_btn.set_label(PLAY_STR);
                        speaker_chooser.activate();
                        stop_btn.deactivate();
                        word_display.set_label("");
                    }
                }
                TTSCommand::WordBoundaryAdvanced => {
                    if word_cnt < word_vec.len() {
                        word_display.set_label(&word_vec[word_cnt]);
                        word_display.redraw();
                        word_cnt += 1;
                    }
                }
                TTSCommand::VolumeChanged => {
                    let volume = vol_sld.value();
                    tts.set_volume(volume).ok();
                    config.set_volume(volume);
                }
                TTSCommand::PlaybackRateChanged => {
                    if let Some(old_handle) = handle {
                        fltk::app::remove_timeout3(old_handle);
                    };

                    handle.replace(fltk::app::add_timeout3(0.5, move |_| {
                        s.send(TTSCommand::PlaybackRateSet);
                    }));
                }
                TTSCommand::PlaybackRateSet => {
                    let rate = rate_sld.value();
                    tts.set_rate(rate).ok();
                    config.set_rate(rate);
                }
            };
        }
    }

    save_config(config).ok();

    Ok(())
}

fn read_files_from_drop() -> anyhow::Result<String> {
    const MAX_FILE_LEN: u64 = 10_485_760; // 10 MiB limit per file seems reasonable

    let text = app::event_text();

    use std::fs::*;

    let file_contents: Vec<String> = text
        .split('\n')
        .filter_map(|s| PathBuf::from_str(s).ok())
        .filter(|p| metadata(p).map_or(false, |m| m.is_file() && m.len() < MAX_FILE_LEN))
        .filter_map(|p| read_to_string(p).ok())
        .collect();

    let content = file_contents.join("\n\n\n");

    Ok(content)
}

/**
 * Extracts a vec of voice chooser labels given a slice of VoiceInformation.
 * Also returns an index into the slice that points to the same voice used in the last session,
 *  if a match based on its name provided in the TtsConfig could be made.
 */
fn load_voice_labels(
    speakers: &[VoiceInformation],
    config: &TtsConfig,
) -> (Vec<String>, Option<usize>) {
    let mut index = None;

    let labels = speakers
        .iter()
        .enumerate()
        .filter_map(|(i, sp)| {
            let name = sp.DisplayName().ok()?;
            let lang = sp.Language().ok()?;

            let label = format!("{name} ({lang})");

            if name == config.voice() {
                index.replace(i);
            }

            Some(label)
        })
        .collect::<Vec<String>>();

    (labels, index)
}

fn init_rate_sld(config: &TtsConfig) -> HorValueSlider {
    let mut s = HorValueSlider::default().with_size(WIDGET_W, 25);
    s.set_label("Speed");
    s.set_bounds(0.2, 2.0);
    s.set_value(config.rate());
    s.set_slider_size(0.1);
    s.set_precision(1);

    s
}

fn init_volume_sld(config: &TtsConfig) -> HorValueSlider {
    let mut s = HorValueSlider::default().with_size(WIDGET_W, 25);
    s.set_label("Volume");
    s.set_value(config.volume());
    s.set_bounds(0.0, 1.0);
    s.set_slider_size(0.1);

    s
}

fn init_stop_btn() -> Button {
    let mut b = Button::default().with_size(WIDGET_W, 50);
    b.set_label("Stop");
    b.set_label_size(20);
    b.deactivate();

    b
}

fn init_play_pause_btn() -> ToggleButton {
    let mut b = ToggleButton::default().with_size(WIDGET_W, 100);
    b.set_label(PLAY_STR);
    b.set_label_size(20);
    b.set_tooltip("Start/Pause/Resume reading text from clipboard");
    b.set_shortcut(Shortcut::from_char('s'));

    b
}

fn init_word_display() -> Frame {
    let mut f = Frame::default().with_size(WIDGET_W, 60);

    f.set_color(Color::White);
    f.set_frame(FrameType::BorderBox);
    f.set_label_size(20);
    f.set_tooltip("Displays the current word");

    f
}

fn init_speaker_chooser(
    labels: &[String],
    last_voice_index: Option<usize>,
) -> anyhow::Result<Choice> {
    let mut chooser = fltk::menu::Choice::default().with_size(WIDGET_W, CHOOSER_HEIGHT);
    chooser.set_text_size(20);

    chooser.set_frame(FrameType::BorderBox);
    chooser.set_tooltip("Voice selector");
    chooser.set_color(Color::White);

    for label in labels {
        chooser.add_choice(label);
    }

    if let Some(index) = last_voice_index {
        chooser.set_value(index as i32);
    }

    Ok(chooser)
}

/**
 * Loads icon from executable and sets them as taskbar/window icons.
 * Must be called after wind.show() in order for it to work!
*/
fn init_icons(w_handle: RawHandle) -> anyhow::Result<()> {
    let hinstance = unsafe { GetModuleHandleW(None)? };
    let icon_name_w = PCWSTR(1 as *mut u16);
    let p_icon = unsafe {
        LoadImageW(
            hinstance,
            icon_name_w,
            IMAGE_ICON,
            512,
            512,
            LR_DEFAULTCOLOR,
        )?
    };

    unsafe {
        SendMessageW(
            HWND(w_handle as isize),
            WM_SETICON,
            WPARAM(ICON_SMALL as usize),
            LPARAM(p_icon.0),
        );
        SendMessageW(
            HWND(w_handle as isize),
            WM_SETICON,
            WPARAM(ICON_BIG as usize),
            LPARAM(p_icon.0),
        );
    };

    Ok(())
}
