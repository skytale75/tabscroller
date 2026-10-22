mod audio;
mod parser;

use audio::AudioManager;
use parser::{format_seconds_to_time, parse_song_content, parse_time_to_seconds, ScrollEvent};

use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CheckButton, CssProvider, DropDown,
    Entry, EventControllerKey, FileDialog, FileFilter, HeaderBar, Label, Orientation, PositionType,
    Scale, ScrolledWindow, Separator, TextBuffer, TextTag, TextView, WrapMode,
};
use regex::Regex;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone)]
pub struct Widgets {
    pub window: ApplicationWindow,
    pub text_view: TextView,
    pub text_buffer: TextBuffer,
    pub font_tag: TextTag,
    pub bracket_tag: TextTag,
    pub brace_tag: TextTag,
    pub start_stop_button: Button,
    pub pause_button: Button,
    pub rewind_button: Button,
    pub forward_button: Button,
    pub warmup_entry: Entry,
    pub metronome_check: CheckButton,
    pub bpm_entry: Entry,
    pub time_entry: Entry,
    pub total_time_entry: Entry,
    pub time_slider: Scale,
    pub mp3_check: CheckButton,
    pub mp3_path_entry: Entry,
    pub font_size_dropdown: DropDown,
}

pub struct AppState {
    pub current_file_path: Option<PathBuf>,
    pub is_running: bool,
    pub is_paused: bool,
    pub current_time_seconds: i32,
    pub total_time_seconds: i32,
    pub scroll_events: Vec<ScrollEvent>,
    pub audio: AudioManager,
    pub timer_source: Option<glib::SourceId>,
    pub metronome_source: Option<glib::SourceId>,
    pub warmup_source: Option<glib::SourceId>,
    pub warmup_remaining: u32,
}

const CSS_DATA: &str = "
window {
    background-color: #1e1e2e;
}

/* Enforce monospace font family across the entire text view */
textview, textview text, textview.view, textview.view text {
    background-color: #24273A;
    color: #A2DD9E;
    caret-color: #ffffff;
    font-family: 'Ubuntu Mono', 'Noto Sans Mono', 'Liberation Mono', 'DejaVu Sans Mono', monospace;
}

.running-btn {
    background-color: #2ec27e;
    color: #000000;
    font-weight: bold;
}

.running-btn:hover {
    background-color: #26a269;
    color: #000000;
}

/* Timeline scrubber slider styling */
scale.song-slider {
    margin-left: 8px;
    margin-right: 8px;
}

scale.song-slider trough {
    min-height: 8px;
    border-radius: 4px;
    background-color: #313244;
}

scale.song-slider highlight {
    min-height: 8px;
    border-radius: 4px;
    background-color: #a6da95;
}

scale.song-slider slider {
    min-width: 18px;
    min-height: 18px;
    border-radius: 9px;
    background-color: #eed49f;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
}

scale.song-slider slider:hover {
    background-color: #f5a97f;
}
";

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("com.mike.musicscroller")
        .build();

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    // 1. Setup CSS with USER priority to override any default system fonts
    let provider = CssProvider::new();
    provider.load_from_string(CSS_DATA);
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
    }

    // 2. Main Window
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Mikes Music Scroller")
        .default_width(980)
        .default_height(680)
        .build();

    // 3. HeaderBar
    let header_bar = HeaderBar::new();
    let open_btn = Button::builder().label("Open").tooltip_text("Open Song File (Ctrl+O)").build();
    let save_btn = Button::builder().label("Save").tooltip_text("Save File (Ctrl+S)").build();
    let header_reload_btn = Button::builder().label("Reload").tooltip_text("Reload File (Ctrl+R)").build();

    header_bar.pack_start(&open_btn);
    header_bar.pack_start(&save_btn);
    header_bar.pack_start(&header_reload_btn);
    window.set_titlebar(Some(&header_bar));

    // 4. Main vertical layout
    let root_box = GtkBox::new(Orientation::Vertical, 0);
    window.set_child(Some(&root_box));

    // --- Top Toolbar (Controls) ---
    let toolbar_box = GtkBox::new(Orientation::Horizontal, 6);
    toolbar_box.set_margin_start(8);
    toolbar_box.set_margin_end(8);
    toolbar_box.set_margin_top(6);
    toolbar_box.set_margin_bottom(4);

    let start_stop_button = Button::builder().label("Start/Stop").build();
    toolbar_box.append(&start_stop_button);

    let pause_button = Button::builder().label("Pause").build();
    toolbar_box.append(&pause_button);

    let rewind_button = Button::builder().label("<< -10s").tooltip_text("Rewind 10 seconds").build();
    toolbar_box.append(&rewind_button);

    let forward_button = Button::builder().label("+10s >>").tooltip_text("Fast Forward 10 seconds").build();
    toolbar_box.append(&forward_button);

    let sep_ctrl = Separator::new(Orientation::Vertical);
    toolbar_box.append(&sep_ctrl);

    let warmup_label = Label::new(Some("Warmup:"));
    toolbar_box.append(&warmup_label);

    let warmup_entry = Entry::builder().text("4").width_chars(4).build();
    toolbar_box.append(&warmup_entry);

    let sep1 = Separator::new(Orientation::Vertical);
    toolbar_box.append(&sep1);

    let metronome_check = CheckButton::builder().active(true).build();
    toolbar_box.append(&metronome_check);

    let bpm_label = Label::new(Some("BPM:"));
    toolbar_box.append(&bpm_label);

    let bpm_entry = Entry::builder().text("120").width_chars(5).build();
    toolbar_box.append(&bpm_entry);

    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    toolbar_box.append(&spacer);

    let font_size_label = Label::new(Some("Text Size:"));
    toolbar_box.append(&font_size_label);

    let font_sizes = ["small", "normal", "large", "extra large"];
    let font_size_dropdown = DropDown::from_strings(&font_sizes);
    font_size_dropdown.set_selected(1); // "normal"
    toolbar_box.append(&font_size_dropdown);

    root_box.append(&toolbar_box);

    // --- Timeline Scrubber Toolbar ---
    let scrubber_box = GtkBox::new(Orientation::Horizontal, 6);
    scrubber_box.set_margin_start(8);
    scrubber_box.set_margin_end(8);
    scrubber_box.set_margin_top(2);
    scrubber_box.set_margin_bottom(6);

    let time_label = Label::new(Some("Time:"));
    scrubber_box.append(&time_label);

    let time_entry = Entry::builder().text("0:00").width_chars(5).editable(false).build();
    scrubber_box.append(&time_entry);

    // Progress / Scrub Slider
    let time_slider = Scale::with_range(Orientation::Horizontal, 0.0, 180.0, 1.0);
    time_slider.set_hexpand(true);
    time_slider.add_css_class("song-slider");
    time_slider.set_draw_value(true);
    time_slider.set_value_pos(PositionType::Top);
    time_slider.set_format_value_func(|_scale, value| {
        format_seconds_to_time(value as i32)
    });
    scrubber_box.append(&time_slider);

    let total_time_label = Label::new(Some("Total Time:"));
    scrubber_box.append(&total_time_label);

    let total_time_entry = Entry::builder().text("3:00").width_chars(5).build();
    scrubber_box.append(&total_time_entry);

    root_box.append(&scrubber_box);

    // --- Center Text View ---
    let buffer = TextBuffer::new(None);
    let tag_table = buffer.tag_table();

    let font_tag = TextTag::builder()
        .name("font_size")
        .family("monospace")
        .size_points(14.0)
        .build();
    tag_table.add(&font_tag);

    let bracket_tag = TextTag::builder()
        .family("monospace")
        .foreground("#EBA0AC")
        .background("#000000")
        .build();
    tag_table.add(&bracket_tag);

    let brace_tag = TextTag::builder()
        .family("monospace")
        .foreground("#FAB387")
        .build();
    tag_table.add(&brace_tag);

    let text_view = TextView::builder()
        .buffer(&buffer)
        .wrap_mode(WrapMode::Word)
        .monospace(true)
        .left_margin(12)
        .right_margin(12)
        .top_margin(10)
        .bottom_margin(10)
        .vexpand(true)
        .hexpand(true)
        .build();

    let scrolled_window = ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .hexpand(true)
        .build();
    root_box.append(&scrolled_window);

    // --- Bottom MP3 Toolbar ---
    let mp3_box = GtkBox::new(Orientation::Horizontal, 6);
    mp3_box.set_margin_start(8);
    mp3_box.set_margin_end(8);
    mp3_box.set_margin_top(6);
    mp3_box.set_margin_bottom(6);

    let mp3_check = CheckButton::builder().label("Play MP3").active(true).build();
    mp3_box.append(&mp3_check);

    let load_mp3_button = Button::builder().label("Load MP3").build();
    mp3_box.append(&load_mp3_button);

    let mp3_path_entry = Entry::builder().placeholder_text("Path to MP3 file").hexpand(true).build();
    mp3_box.append(&mp3_path_entry);

    let reload_button = Button::builder().label("Reload").build();
    mp3_box.append(&reload_button);

    root_box.append(&mp3_box);

    // 5. App State and Widgets
    let widgets = Widgets {
        window: window.clone(),
        text_view: text_view.clone(),
        text_buffer: buffer.clone(),
        font_tag,
        bracket_tag,
        brace_tag,
        start_stop_button: start_stop_button.clone(),
        pause_button: pause_button.clone(),
        rewind_button: rewind_button.clone(),
        forward_button: forward_button.clone(),
        warmup_entry: warmup_entry.clone(),
        metronome_check: metronome_check.clone(),
        bpm_entry: bpm_entry.clone(),
        time_entry: time_entry.clone(),
        total_time_entry: total_time_entry.clone(),
        time_slider: time_slider.clone(),
        mp3_check: mp3_check.clone(),
        mp3_path_entry: mp3_path_entry.clone(),
        font_size_dropdown: font_size_dropdown.clone(),
    };

    let state = Rc::new(RefCell::new(AppState {
        current_file_path: None,
        is_running: false,
        is_paused: false,
        current_time_seconds: 0,
        total_time_seconds: 180,
        scroll_events: Vec::new(),
        audio: AudioManager::new(),
        timer_source: None,
        metronome_source: None,
        warmup_source: None,
        warmup_remaining: 0,
    }));

    // Flag to prevent recursive feedback loop when timer updates the slider
    let is_updating_slider = Rc::new(Cell::new(false));

    // 6. Connect Handlers

    // Start/Stop
    let state_ss = state.clone();
    let wid_ss = widgets.clone();
    let upd_ss = is_updating_slider.clone();
    start_stop_button.connect_clicked(move |_| {
        toggle_scrolling_metronome(&state_ss, &wid_ss, &upd_ss);
    });

    // Pause/Resume
    let state_pr = state.clone();
    let wid_pr = widgets.clone();
    let upd_pr = is_updating_slider.clone();
    pause_button.connect_clicked(move |_| {
        pause_resume(&state_pr, &wid_pr, &upd_pr);
    });

    // Slider scrub (rewind / fast-forward by dragging or clicking)
    let state_sl = state.clone();
    let wid_sl = widgets.clone();
    let upd_sl = is_updating_slider.clone();
    time_slider.connect_value_changed(move |scale| {
        if upd_sl.get() {
            return;
        }
        let target_sec = scale.value().round() as i32;
        seek_to_time(target_sec, &state_sl, &wid_sl, &upd_sl);
    });

    // Rewind -10s
    let state_rw = state.clone();
    let wid_rw = widgets.clone();
    let upd_rw = is_updating_slider.clone();
    rewind_button.connect_clicked(move |_| {
        let current = state_rw.borrow().current_time_seconds;
        let new_time = (current - 10).max(0);
        seek_to_time(new_time, &state_rw, &wid_rw, &upd_rw);
    });

    // Fast Forward +10s
    let state_fw = state.clone();
    let wid_fw = widgets.clone();
    let upd_fw = is_updating_slider.clone();
    forward_button.connect_clicked(move |_| {
        let total = state_fw.borrow().total_time_seconds;
        let current = state_fw.borrow().current_time_seconds;
        let new_time = (current + 10).min(total);
        seek_to_time(new_time, &state_fw, &wid_fw, &upd_fw);
    });

    // Total time entry change
    let state_tt = state.clone();
    let wid_tt = widgets.clone();
    total_time_entry.connect_changed(move |entry| {
        let sec = parse_time_to_seconds(&entry.text());
        if sec > 0 {
            state_tt.borrow_mut().total_time_seconds = sec;
            wid_tt.time_slider.set_range(0.0, sec as f64);
        }
    });

    // Metronome checkbox toggle
    let bpm_entry_clone = bpm_entry.clone();
    metronome_check.connect_toggled(move |btn| {
        bpm_entry_clone.set_sensitive(btn.is_active());
        if btn.is_active() {
            println!("Metronome enabled.");
        } else {
            println!("Metronome disabled.");
        }
    });

    // Font size dropdown
    let wid_font = widgets.clone();
    font_size_dropdown.connect_selected_notify(move |dd| {
        let size = match dd.selected() {
            0 => 11.0, // small
            1 => 14.0, // normal
            2 => 18.0, // large
            3 => 24.0, // extra large
            _ => 14.0,
        };
        wid_font.font_tag.set_property("size-points", size);
        apply_syntax_highlighting(&wid_font);
    });

    // Open file
    let state_open = state.clone();
    let wid_open = widgets.clone();
    let upd_open = is_updating_slider.clone();
    open_btn.connect_clicked(move |_| {
        open_file(&state_open, &wid_open, &upd_open);
    });

    // Save file
    let state_save = state.clone();
    let wid_save = widgets.clone();
    save_btn.connect_clicked(move |_| {
        save_file(&state_save, &wid_save);
    });

    // Reload file
    let state_rel = state.clone();
    let wid_rel = widgets.clone();
    let upd_rel = is_updating_slider.clone();
    reload_button.connect_clicked(move |_| {
        reload(&state_rel, &wid_rel, &upd_rel);
    });

    let state_rel2 = state.clone();
    let wid_rel2 = widgets.clone();
    let upd_rel2 = is_updating_slider.clone();
    header_reload_btn.connect_clicked(move |_| {
        reload(&state_rel2, &wid_rel2, &upd_rel2);
    });

    // Load MP3
    let state_mp3 = state.clone();
    let wid_mp3 = widgets.clone();
    load_mp3_button.connect_clicked(move |_| {
        load_mp3_dialog(&state_mp3, &wid_mp3);
    });

    // MP3 path manual entry change
    let state_path = state.clone();
    mp3_path_entry.connect_changed(move |entry| {
        let text = entry.text().trim().to_string();
        if text.is_empty() {
            state_path.borrow_mut().audio.set_mp3_path(None);
        } else {
            state_path.borrow_mut().audio.set_mp3_path(Some(PathBuf::from(text)));
        }
    });

    // Syntax highlighting on text change (with reentrancy protection)
    let is_highlighting = Rc::new(Cell::new(false));
    let is_high = is_highlighting.clone();
    let wid_hl = widgets.clone();
    buffer.connect_changed(move |_| {
        if is_high.get() {
            return;
        }
        is_high.set(true);
        apply_syntax_highlighting(&wid_hl);
        is_high.set(false);
    });

    // Keyboard shortcuts (Ctrl+S for save, Ctrl+O for open, Ctrl+R for reload, Left/Right arrow for rewind/forward)
    let state_key = state.clone();
    let wid_key = widgets.clone();
    let upd_key = is_updating_slider.clone();
    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed(move |_ctrl, keyval, _keycode, modifier| {
        if modifier.contains(gdk::ModifierType::CONTROL_MASK) {
            if keyval == gdk::Key::s || keyval == gdk::Key::S {
                save_file(&state_key, &wid_key);
                return glib::Propagation::Stop;
            } else if keyval == gdk::Key::o || keyval == gdk::Key::O {
                open_file(&state_key, &wid_key, &upd_key);
                return glib::Propagation::Stop;
            } else if keyval == gdk::Key::r || keyval == gdk::Key::R {
                reload(&state_key, &wid_key, &upd_key);
                return glib::Propagation::Stop;
            } else if keyval == gdk::Key::Left {
                let current = state_key.borrow().current_time_seconds;
                seek_to_time((current - 5).max(0), &state_key, &wid_key, &upd_key);
                return glib::Propagation::Stop;
            } else if keyval == gdk::Key::Right {
                let total = state_key.borrow().total_time_seconds;
                let current = state_key.borrow().current_time_seconds;
                seek_to_time((current + 5).min(total), &state_key, &wid_key, &upd_key);
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    // Initial startup file: check play.txt in app dir, current directory, or next to binary
    let startup_files = [
        get_app_dir().join("play.txt"),
        PathBuf::from("play.txt"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("play.txt")))
            .unwrap_or_else(|| PathBuf::from("play.txt")),
    ];

    for path in &startup_files {
        if path.exists() {
            load_file_content(path, &state, &widgets, &is_updating_slider);
            break;
        }
    }

    if state.borrow().current_file_path.is_none() {
        apply_syntax_highlighting(&widgets);
    }

    window.present();
}

fn apply_syntax_highlighting(widgets: &Widgets) {
    let buffer = &widgets.text_buffer;
    let start_iter = buffer.start_iter();
    let end_iter = buffer.end_iter();
    buffer.remove_all_tags(&start_iter, &end_iter);

    let text = buffer.text(&start_iter, &end_iter, false);
    if text.is_empty() {
        return;
    }

    buffer.apply_tag(&widgets.font_tag, &start_iter, &end_iter);

    // Build byte-index to char-index map for exact unicode placement
    let mut byte_to_char = Vec::with_capacity(text.len() + 1);
    let mut char_count: usize = 0;
    for (b_idx, _) in text.char_indices() {
        while byte_to_char.len() < b_idx {
            byte_to_char.push(char_count.saturating_sub(1));
        }
        byte_to_char.push(char_count);
        char_count += 1;
    }
    while byte_to_char.len() <= text.len() {
        byte_to_char.push(char_count);
    }

    let bracket_re = Regex::new(r"\[.*?\]").unwrap();
    for m in bracket_re.find_iter(&text) {
        let start_char = byte_to_char[m.start()] as i32;
        let end_char = byte_to_char[m.end()] as i32;
        let start = buffer.iter_at_offset(start_char);
        let end = buffer.iter_at_offset(end_char);
        buffer.apply_tag(&widgets.bracket_tag, &start, &end);
    }

    let brace_re = Regex::new(r"\{.*?\}").unwrap();
    for m in brace_re.find_iter(&text) {
        let start_char = byte_to_char[m.start()] as i32;
        let end_char = byte_to_char[m.end()] as i32;
        let start = buffer.iter_at_offset(start_char);
        let end = buffer.iter_at_offset(end_char);
        buffer.apply_tag(&widgets.brace_tag, &start, &end);
    }
}

fn prepare_scroll_data(state: &mut AppState, widgets: &Widgets) {
    let total_time_str = widgets.total_time_entry.text();
    state.total_time_seconds = parse_time_to_seconds(&total_time_str);
    if state.total_time_seconds <= 0 {
        eprintln!("Warning: Total time is zero or invalid. Scroller will not run.");
    } else {
        println!(
            "Total duration set to {} seconds ({}).",
            state.total_time_seconds, total_time_str
        );
        widgets.time_slider.set_range(0.0, state.total_time_seconds as f64);
    }

    let buffer = &widgets.text_buffer;
    let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    let parsed = parse_song_content(&text);
    state.scroll_events = parsed.scroll_events;
    println!("Found and sorted {} scroll events.", state.scroll_events.len());
}

fn toggle_scrolling_metronome(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    let is_running = state_rc.borrow().is_running;
    if is_running {
        stop_all(state_rc, widgets, is_updating_slider);
    } else {
        start_metronome_and_scrolling(state_rc, widgets, is_updating_slider);
    }
}

fn start_metronome_and_scrolling(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    stop_all(state_rc, widgets, is_updating_slider);

    {
        let mut state = state_rc.borrow_mut();
        prepare_scroll_data(&mut state, widgets);
        if state.total_time_seconds <= 0 {
            return;
        }
        state.is_running = true;
        state.is_paused = false;
        state.current_time_seconds = -1;
    }

    widgets.start_stop_button.add_css_class("running-btn");
    widgets.time_entry.set_text("0:00");
    is_updating_slider.set(true);
    widgets.time_slider.set_value(0.0);
    is_updating_slider.set(false);

    let bpm = widgets
        .bpm_entry
        .text()
        .parse::<u32>()
        .unwrap_or(120)
        .max(1);
    let warmup_ticks = widgets
        .warmup_entry
        .text()
        .parse::<u32>()
        .unwrap_or(4);

    let beat_interval = Duration::from_millis((60000.0 / bpm as f64).round() as u64);

    if warmup_ticks > 0 {
        state_rc.borrow_mut().warmup_remaining = warmup_ticks;
        if widgets.metronome_check.is_active() {
            state_rc.borrow().audio.play_tick();
        }

        let state_w = state_rc.clone();
        let wid_w = widgets.clone();
        let upd_w = is_updating_slider.clone();
        let warmup_source = glib::timeout_add_local(beat_interval, move || {
            let mut state = state_w.borrow_mut();
            if state.warmup_remaining > 1 {
                state.warmup_remaining -= 1;
                if wid_w.metronome_check.is_active() {
                    state.audio.play_tick();
                }
                glib::ControlFlow::Continue
            } else {
                state.warmup_remaining = 0;
                state.warmup_source = None;
                drop(state);
                start_main_process(&state_w, &wid_w, &upd_w, beat_interval);
                glib::ControlFlow::Break
            }
        });
        state_rc.borrow_mut().warmup_source = Some(warmup_source);
    } else {
        start_main_process(state_rc, widgets, is_updating_slider, beat_interval);
    }
}

fn start_main_process(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
    beat_interval: Duration,
) {
    {
        let mut state = state_rc.borrow_mut();
        if widgets.metronome_check.is_active() {
            state.audio.play_start_tick();
        }

        if widgets.mp3_check.is_active() && state.audio.current_mp3_path().is_some() {
            state.audio.play_mp3();
        }

        let state_met = state_rc.clone();
        let wid_met = widgets.clone();
        let met_source = glib::timeout_add_local(beat_interval, move || {
            let state = state_met.borrow();
            if wid_met.metronome_check.is_active() {
                state.audio.play_tick();
            }
            glib::ControlFlow::Continue
        });
        state.metronome_source = Some(met_source);
    }

    // Run first 1-sec tick
    update_timer(state_rc, widgets, is_updating_slider);

    // Schedule 1-sec recurring timer
    let state_timer = state_rc.clone();
    let wid_timer = widgets.clone();
    let upd_timer = is_updating_slider.clone();
    let timer_source = glib::timeout_add_local(Duration::from_millis(1000), move || {
        update_timer(&state_timer, &wid_timer, &upd_timer)
    });
    state_rc.borrow_mut().timer_source = Some(timer_source);
}

fn update_timer(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) -> glib::ControlFlow {
    let mut state = state_rc.borrow_mut();
    state.current_time_seconds += 1;
    let curr = state.current_time_seconds;
    let total = state.total_time_seconds;

    widgets.time_entry.set_text(&format_seconds_to_time(curr));

    is_updating_slider.set(true);
    widgets.time_slider.set_value(curr as f64);
    is_updating_slider.set(false);

    for event in &state.scroll_events {
        if event.time_seconds == curr {
            let line_number = event.line_index;
            println!(
                "Time {}: Scrolling to line {}.",
                format_seconds_to_time(curr),
                line_number + 1
            );
            let target_line = (line_number as i32).saturating_sub(6);
            let buffer = &widgets.text_buffer;
            let mut iter = buffer
                .iter_at_line(target_line)
                .unwrap_or_else(|| buffer.start_iter());
            widgets.text_view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.0);
        }
    }

    if curr >= total {
        println!("Total time reached. Stopping all.");
        drop(state);
        stop_all(state_rc, widgets, is_updating_slider);
        return glib::ControlFlow::Break;
    }

    glib::ControlFlow::Continue
}

fn seek_to_time(
    new_time: i32,
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    let mut state = state_rc.borrow_mut();
    let total = state.total_time_seconds;
    let clamped_time = new_time.clamp(0, total.max(0));
    state.current_time_seconds = clamped_time;

    widgets.time_entry.set_text(&format_seconds_to_time(clamped_time));

    is_updating_slider.set(true);
    widgets.time_slider.set_value(clamped_time as f64);
    is_updating_slider.set(false);

    scroll_to_time(clamped_time, &state, widgets);

    state.audio.seek_mp3(Duration::from_secs(clamped_time as u64));

    if state.is_running && !state.is_paused && widgets.mp3_check.is_active() {
        state.audio.resume_mp3();
    }
}

fn scroll_to_time(time_seconds: i32, state: &AppState, widgets: &Widgets) {
    let buffer = &widgets.text_buffer;
    let target_event = state
        .scroll_events
        .iter()
        .filter(|e| e.time_seconds <= time_seconds)
        .last();

    if let Some(event) = target_event {
        let line_number = event.line_index;
        let target_line = (line_number as i32).saturating_sub(6);
        let mut iter = buffer
            .iter_at_line(target_line)
            .unwrap_or_else(|| buffer.start_iter());
        widgets.text_view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.0);
    } else {
        let mut iter = buffer.start_iter();
        widgets.text_view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.0);
    }
}

fn pause_resume(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    let mut state = state_rc.borrow_mut();
    if !state.is_running {
        return;
    }

    state.is_paused = !state.is_paused;
    if state.is_paused {
        state.audio.pause_mp3();

        if let Some(id) = state.timer_source.take() {
            id.remove();
        }
        if let Some(id) = state.metronome_source.take() {
            id.remove();
        }
        if let Some(id) = state.warmup_source.take() {
            id.remove();
        }

        widgets.pause_button.set_label("Resume");
        println!("Paused.");
    } else {
        state.audio.resume_mp3();

        let bpm = widgets
            .bpm_entry
            .text()
            .parse::<u32>()
            .unwrap_or(120)
            .max(1);
        let beat_interval = Duration::from_millis((60000.0 / bpm as f64).round() as u64);

        let state_met = state_rc.clone();
        let wid_met = widgets.clone();
        let met_source = glib::timeout_add_local(beat_interval, move || {
            let state = state_met.borrow();
            if wid_met.metronome_check.is_active() {
                state.audio.play_tick();
            }
            glib::ControlFlow::Continue
        });
        state.metronome_source = Some(met_source);

        let state_timer = state_rc.clone();
        let wid_timer = widgets.clone();
        let upd_timer = is_updating_slider.clone();
        let timer_source = glib::timeout_add_local(Duration::from_millis(1000), move || {
            update_timer(&state_timer, &wid_timer, &upd_timer)
        });
        state.timer_source = Some(timer_source);

        widgets.pause_button.set_label("Pause");
        println!("Resumed.");
    }
}

fn stop_all(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    let mut state = state_rc.borrow_mut();
    state.is_running = false;
    state.is_paused = false;
    state.current_time_seconds = 0;

    state.audio.stop_mp3();

    if let Some(id) = state.warmup_source.take() {
        id.remove();
        println!("Warmup stopped.");
    }

    if let Some(id) = state.timer_source.take() {
        id.remove();
        println!("Main timer stopped.");
    }

    if let Some(id) = state.metronome_source.take() {
        id.remove();
        println!("Metronome stopped.");
    }

    widgets.start_stop_button.remove_css_class("running-btn");
    widgets.pause_button.set_label("Pause");
    widgets.time_entry.set_text("0:00");

    is_updating_slider.set(true);
    widgets.time_slider.set_value(0.0);
    is_updating_slider.set(false);

    let mut iter = widgets.text_buffer.start_iter();
    widgets.text_view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.0);
}

fn load_file_content(
    path: &Path,
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            stop_all(state_rc, widgets, is_updating_slider);

            widgets.text_buffer.set_text(&content);
            state_rc.borrow_mut().current_file_path = Some(path.to_path_buf());
            let file_name = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".to_string());
            widgets
                .window
                .set_title(Some(&format!("Mikes Music Scroller - {}", file_name)));

            println!("Loaded file: {:?}", path);

            let parsed = parse_song_content(&content);

            if let Some(bpm) = parsed.bpm {
                widgets.bpm_entry.set_text(&bpm.to_string());
                println!("BPM found in file and updated to: {}", bpm);
            }

            if let Some(total_time) = parsed.total_time_seconds {
                widgets
                    .total_time_entry
                    .set_text(&format_seconds_to_time(total_time));
                state_rc.borrow_mut().total_time_seconds = total_time;
                widgets.time_slider.set_range(0.0, total_time as f64);
                println!("Total Time found in file and updated to: {}", total_time);
            }

            if let Some(track) = parsed.track {
                if track.eq_ignore_ascii_case("none") {
                    println!("Track set to NONE. No MP3 will be loaded.");
                    widgets.mp3_path_entry.set_text("");
                    state_rc.borrow_mut().audio.set_mp3_path(None);
                } else {
                    let track_path = resolve_mp3_path(&track, path.parent());
                    widgets
                        .mp3_path_entry
                        .set_text(&track_path.to_string_lossy());
                    state_rc
                        .borrow_mut()
                        .audio
                        .set_mp3_path(Some(track_path.clone()));
                    println!("Loaded MP3 from track: {:?}", track_path);
                }
            } else {
                println!("No TRACK specified in the file.");
                widgets.mp3_path_entry.set_text("");
                state_rc.borrow_mut().audio.set_mp3_path(None);
            }

            apply_syntax_highlighting(widgets);

            // Re-parse scroll events for seeking
            let mut state = state_rc.borrow_mut();
            state.scroll_events = parsed.scroll_events;
        }
        Err(e) => eprintln!("Error loading file {:?}: {}", path, e),
    }
}

fn get_app_dir() -> PathBuf {
    // 1. Walk up from current executable to find the directory named "music_scroller" or containing songs/Cargo.toml
    if let Ok(exe_path) = std::env::current_exe() {
        let mut curr = exe_path.parent();
        while let Some(dir) = curr {
            if dir.file_name().map_or(false, |n| n == "music_scroller")
                || dir.join("Cargo.toml").exists()
                || dir.join("songs").exists()
            {
                return dir.to_path_buf();
            }
            curr = dir.parent();
        }
    }

    // 2. Check current working directory
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.file_name().map_or(false, |n| n == "music_scroller")
            || cwd.join("Cargo.toml").exists()
            || cwd.join("songs").exists()
        {
            return cwd;
        }
    }

    // 3. Check ~/music_scroller
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = Path::new(&home).join("music_scroller");
        if candidate.exists() {
            return candidate;
        }
    }

    // Fallback to current working directory or "."
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn resolve_mp3_path(track: &str, song_dir: Option<&Path>) -> PathBuf {
    let p = Path::new(track);
    if p.is_absolute() && p.exists() {
        return p.to_path_buf();
    }
    if let Some(dir) = song_dir {
        let in_song_dir = dir.join(p);
        if in_song_dir.exists() {
            return in_song_dir;
        }
    }
    let app_dir = get_app_dir();
    let in_app_dir = app_dir.join(p);
    if in_app_dir.exists() {
        return in_app_dir;
    }
    let in_app_songs = app_dir.join("songs").join(p);
    if in_app_songs.exists() {
        return in_app_songs;
    }
    if let Ok(cwd) = std::env::current_dir() {
        let in_cwd = cwd.join(p);
        if in_cwd.exists() {
            return in_cwd;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let in_music = Path::new(&home).join("Music").join(p);
        if in_music.exists() {
            return in_music;
        }
    }
    if let Some(dir) = song_dir {
        dir.join(p)
    } else {
        p.to_path_buf()
    }
}

fn open_file(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    let dialog = FileDialog::builder().title("Open Song File").build();
    let filter = FileFilter::new();
    filter.add_pattern("*.txt");
    filter.set_name(Some("Text Files (*.txt)"));
    let filters = gio::ListStore::new::<FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let songs_dir = get_app_dir().join("songs");
    let initial_dir = if songs_dir.exists() {
        songs_dir
    } else {
        get_app_dir()
    };
    dialog.set_initial_folder(Some(&gio::File::for_path(&initial_dir)));

    let state_c = state_rc.clone();
    let wid_c = widgets.clone();
    let upd_c = is_updating_slider.clone();
    dialog.open(Some(&widgets.window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                load_file_content(&path, &state_c, &wid_c, &upd_c);
            }
        }
    });
}

fn save_file(state_rc: &Rc<RefCell<AppState>>, widgets: &Widgets) {
    let current_path = state_rc.borrow().current_file_path.clone();
    if let Some(path) = current_path {
        let buffer = &widgets.text_buffer;
        let content = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        if let Err(e) = std::fs::write(&path, content) {
            eprintln!("Error saving file {:?}: {}", path, e);
        } else {
            println!("File saved: {:?}", path);
        }
    } else {
        save_file_as(state_rc, widgets);
    }
}

fn save_file_as(state_rc: &Rc<RefCell<AppState>>, widgets: &Widgets) {
    let dialog = FileDialog::builder().title("Save Song").build();
    let filter = FileFilter::new();
    filter.add_pattern("*.txt");
    filter.set_name(Some("Text Files (*.txt)"));
    let filters = gio::ListStore::new::<FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let songs_dir = get_app_dir().join("songs");
    let initial_dir = if songs_dir.exists() {
        songs_dir
    } else {
        get_app_dir()
    };
    dialog.set_initial_folder(Some(&gio::File::for_path(&initial_dir)));
    dialog.set_initial_name(Some("untitled.txt"));

    let state_c = state_rc.clone();
    let wid_c = widgets.clone();
    dialog.save(Some(&widgets.window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                let buffer = &wid_c.text_buffer;
                let content = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
                if let Err(e) = std::fs::write(&path, content) {
                    eprintln!("Error saving file {:?}: {}", path, e);
                } else {
                    println!("File saved: {:?}", path);
                    state_c.borrow_mut().current_file_path = Some(path.clone());
                    let file_name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".to_string());
                    wid_c
                        .window
                        .set_title(Some(&format!("Mikes Music Scroller - {}", file_name)));
                }
            }
        }
    });
}

fn reload(
    state_rc: &Rc<RefCell<AppState>>,
    widgets: &Widgets,
    is_updating_slider: &Rc<Cell<bool>>,
) {
    let current_path = state_rc.borrow().current_file_path.clone();
    if let Some(path) = current_path {
        println!("Reloading file: {:?}", path);
        load_file_content(&path, state_rc, widgets, is_updating_slider);
    } else {
        println!("No file is currently open to reload.");
    }
}

fn load_mp3_dialog(state_rc: &Rc<RefCell<AppState>>, widgets: &Widgets) {
    let dialog = FileDialog::builder().title("Select MP3 File").build();
    let filter = FileFilter::new();
    filter.add_pattern("*.mp3");
    filter.set_name(Some("MP3 Files (*.mp3)"));
    let filters = gio::ListStore::new::<FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let app_dir = get_app_dir();
    let current_mp3_entry = widgets.mp3_path_entry.text().trim().to_string();
    let initial_folder = if !current_mp3_entry.is_empty() {
        let p = Path::new(&current_mp3_entry);
        if p.exists() {
            if p.is_dir() {
                p.to_path_buf()
            } else {
                p.parent().map(|par| par.to_path_buf()).unwrap_or_else(|| app_dir.clone())
            }
        } else {
            app_dir.clone()
        }
    } else {
        app_dir.clone()
    };

    dialog.set_initial_folder(Some(&gio::File::for_path(&initial_folder)));

    let state_c = state_rc.clone();
    let wid_c = widgets.clone();
    dialog.open(Some(&widgets.window), gio::Cancellable::NONE, move |result| {
        if let Ok(file) = result {
            if let Some(path) = file.path() {
                wid_c.mp3_path_entry.set_text(&path.to_string_lossy());
                state_c.borrow_mut().audio.set_mp3_path(Some(path.clone()));
                println!("Loaded MP3: {:?}", path);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_app_dir() {
        let app_dir = get_app_dir();
        assert!(app_dir.exists());
        assert!(app_dir.join("Cargo.toml").exists() || app_dir.join("songs").exists());
    }

    #[test]
    fn test_resolve_mp3_path_in_app_dir() {
        let path = resolve_mp3_path("Free.mp3", None);
        assert!(path.exists());
        assert!(path.is_file());
    }
}

