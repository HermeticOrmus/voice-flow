mod status;
mod store;
mod theme;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib::{self, ControlFlow};
use gtk4::prelude::*;
use gtk4::{
    gio, Align, Application, ApplicationWindow, Box, Button, CssProvider, Entry, Label, ListBox,
    ListBoxRow, Orientation, PolicyType, ScrolledWindow, SelectionMode, Separator, Switch,
    TextView, ToggleButton, WrapMode,
};

use store::Express;

fn main() {
    let app = Application::builder()
        .application_id("solutions.ormus.VoiceFlowPanel")
        .build();
    app.connect_activate(build);
    app.run();
}

fn build(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Voice Flow")
        .default_width(920)
        .default_height(680)
        .build();

    let css = CssProvider::new();
    css.load_from_data(&theme::css(&theme::load()));
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("display"),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let root = Box::new(Orientation::Vertical, 12);
    root.set_margin_top(18);
    root.set_margin_bottom(18);
    root.set_margin_start(18);
    root.set_margin_end(18);

    let head = Box::new(Orientation::Horizontal, 12);
    let title = Label::new(Some("Voice Flow"));
    title.add_css_class("title");
    title.set_halign(Align::Start);
    title.set_hexpand(true);
    let state = Label::new(Some(status::read_state()));
    state.add_css_class("state");
    state.set_halign(Align::End);
    head.append(&title);
    head.append(&state);
    let note = Label::new(Some(
        "Dictionary and substitutions apply on the next Super+H. Terminals stay verbatim.",
    ));
    note.add_css_class("muted");
    note.set_halign(Align::Start);
    note.set_wrap(true);
    root.append(&head);
    root.append(&note);
    root.append(&numbers_row(&state));

    let express_row = Box::new(Orientation::Horizontal, 8);
    let express_label = Label::new(Some("EXPRESS"));
    express_label.add_css_class("section");
    let formal = ToggleButton::with_label("Formal");
    let casual = ToggleButton::with_label("Casual");
    let verbatim = ToggleButton::with_label("Verbatim");
    casual.set_group(Some(&formal));
    verbatim.set_group(Some(&formal));
    match store::load_express(&store::conf_path()) {
        Express::Formal => formal.set_active(true),
        Express::Casual => casual.set_active(true),
        Express::Verbatim => verbatim.set_active(true),
    }
    for (button, value) in [
        (&formal, Express::Formal),
        (&casual, Express::Casual),
        (&verbatim, Express::Verbatim),
    ] {
        button.connect_toggled(move |button| {
            if button.is_active() {
                let _ = store::save_express(&store::conf_path(), value);
            }
        });
    }
    express_row.append(&express_label);
    express_row.append(&formal);
    express_row.append(&casual);
    express_row.append(&verbatim);
    root.append(&express_row);
    root.append(&controls_row());
    root.append(&actions_row());

    root.append(&section("Last paste"));
    let paste_view = TextView::new();
    paste_view.set_editable(false);
    paste_view.set_cursor_visible(false);
    paste_view.set_wrap_mode(WrapMode::Word);
    paste_view.set_left_margin(10);
    paste_view.set_right_margin(10);
    paste_view.set_top_margin(8);
    paste_view.set_bottom_margin(8);
    let paste_buf = paste_view.buffer();
    paste_buf.set_text(&store::load_last_paste(&store::last_paste_path()));
    let paste_scroll = ScrolledWindow::builder()
        .child(&paste_view)
        .hscrollbar_policy(PolicyType::Never)
        .min_content_height(120)
        .vexpand(false)
        .build();
    root.append(&paste_scroll);
    let paste_path = store::last_paste_path();
    glib::timeout_add_local(Duration::from_millis(800), move || {
        let text = store::load_last_paste(&paste_path);
        let current = paste_buf.text(&paste_buf.start_iter(), &paste_buf.end_iter(), false);
        if current.as_str() != text {
            paste_buf.set_text(&text);
        }
        ControlFlow::Continue
    });

    let columns = Box::new(Orientation::Horizontal, 16);
    columns.set_vexpand(true);
    columns.append(&dictionary_column());
    columns.append(&Separator::new(Orientation::Vertical));
    columns.append(&substitution_column());
    root.append(&columns);

    window.set_child(Some(&root));
    window.present();
}

fn section(text: &str) -> Label {
    let label = Label::new(Some(text));
    label.add_css_class("section");
    label.set_halign(Align::Start);
    label
}

fn dictionary_column() -> Box {
    let column = Box::new(Orientation::Vertical, 8);
    column.set_hexpand(true);
    column.append(&section("Dictionary"));
    let hint = Label::new(Some("Words Whisper should prefer."));
    hint.add_css_class("muted");
    hint.set_halign(Align::Start);
    column.append(&hint);

    let terms = Rc::new(RefCell::new(store::load_vocab(&store::vocab_path())));
    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    let scroll = scrolled(&list);
    fill_labels(&list, &terms.borrow());
    column.append(&scroll);

    let add_row = Box::new(Orientation::Horizontal, 8);
    let entry = Entry::new();
    entry.set_placeholder_text(Some("Add a word"));
    entry.set_hexpand(true);
    let add = Button::with_label("Add");
    let remove = Button::with_label("Remove");
    add_row.append(&entry);
    add_row.append(&add);
    add_row.append(&remove);
    column.append(&add_row);

    let terms_add = Rc::clone(&terms);
    let list_add = list.clone();
    let entry_add = entry.clone();
    let commit: Rc<dyn Fn()> = Rc::new(move || {
        let term = entry_add.text().trim().to_string();
        if term.is_empty() {
            return;
        }
        let mut owned = terms_add.borrow_mut();
        if !owned.iter().any(|have| have == &term) {
            owned.push(term);
            let _ = store::save_vocab(&store::vocab_path(), &owned);
            fill_labels(&list_add, &owned);
        }
        entry_add.set_text("");
    });
    add.connect_clicked({
        let commit = Rc::clone(&commit);
        move |_| commit()
    });
    entry.connect_activate(move |_| commit());

    remove.connect_clicked(move |_| {
        let Some(row) = list.selected_row() else {
            return;
        };
        let Some(label) = row_label(&row) else {
            return;
        };
        let mut owned = terms.borrow_mut();
        owned.retain(|term| term != &label);
        let _ = store::save_vocab(&store::vocab_path(), &owned);
        fill_labels(&list, &owned);
    });

    column
}

fn substitution_column() -> Box {
    let column = Box::new(Orientation::Vertical, 8);
    column.set_hexpand(true);
    column.append(&section("Substitutions"));
    let hint = Label::new(Some("What was heard, and what you meant."));
    hint.add_css_class("muted");
    hint.set_halign(Align::Start);
    column.append(&hint);

    let rows = Rc::new(RefCell::new(store::load_substitutions(
        &store::substitutions_path(),
    )));
    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::Single);
    fill_subs(&list, &rows.borrow());
    column.append(&scrolled(&list));

    let heard = Entry::new();
    heard.set_placeholder_text(Some("Heard"));
    heard.set_hexpand(true);
    let meant = Entry::new();
    meant.set_placeholder_text(Some("Meant"));
    meant.set_hexpand(true);
    let add = Button::with_label("Add");
    let remove = Button::with_label("Remove");
    let fields = Box::new(Orientation::Horizontal, 8);
    fields.append(&heard);
    fields.append(&meant);
    fields.append(&add);
    fields.append(&remove);
    column.append(&fields);

    let rows_add = Rc::clone(&rows);
    let list_add = list.clone();
    let heard_add = heard.clone();
    let meant_add = meant.clone();
    let commit: Rc<dyn Fn()> = Rc::new(move || {
        let heard_text = heard_add.text().trim().to_string();
        let meant_text = meant_add.text().trim().to_string();
        if heard_text.is_empty() || meant_text.is_empty() {
            return;
        }
        let mut owned = rows_add.borrow_mut();
        owned.retain(|row| !row.heard.eq_ignore_ascii_case(&heard_text));
        owned.push(store::Substitution {
            heard: heard_text,
            canonical: meant_text,
        });
        let _ = store::save_substitutions(&store::substitutions_path(), &owned);
        fill_subs(&list_add, &owned);
        heard_add.set_text("");
        meant_add.set_text("");
    });
    add.connect_clicked({
        let commit = Rc::clone(&commit);
        move |_| commit()
    });
    meant.connect_activate(move |_| commit());

    remove.connect_clicked(move |_| {
        let Some(row) = list.selected_row() else {
            return;
        };
        let Some(index) = row.index().try_into().ok() else {
            return;
        };
        let mut owned = rows.borrow_mut();
        if index < owned.len() {
            owned.remove(index);
            let _ = store::save_substitutions(&store::substitutions_path(), &owned);
            fill_subs(&list, &owned);
        }
    });

    column
}

fn scrolled(list: &ListBox) -> ScrolledWindow {
    ScrolledWindow::builder()
        .child(list)
        .vexpand(true)
        .hscrollbar_policy(PolicyType::Never)
        .min_content_height(220)
        .build()
}

fn fill_labels(list: &ListBox, terms: &[String]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for term in terms {
        let label = Label::new(Some(term));
        label.set_halign(Align::Start);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_start(8);
        label.set_margin_end(8);
        let row = ListBoxRow::new();
        row.set_child(Some(&label));
        list.append(&row);
    }
}

fn fill_subs(list: &ListBox, rows: &[store::Substitution]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for row in rows {
        let label = Label::new(Some(&format!("{}   →   {}", row.heard, row.canonical)));
        label.set_halign(Align::Start);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_start(8);
        label.set_margin_end(8);
        let item = ListBoxRow::new();
        item.set_child(Some(&label));
        list.append(&item);
    }
}

fn row_label(row: &ListBoxRow) -> Option<String> {
    row.child()
        .and_then(|child| child.downcast::<Label>().ok())
        .map(|label| label.text().to_string())
}

/// Today's numbers and the engine, refreshed on timers: the state word
/// every 400 ms, the counts every 2 s, the host probe every 10 s off the
/// main thread (a dead host costs 300 ms).
fn numbers_row(state: &Label) -> Box {
    let row = Box::new(Orientation::Horizontal, 28);
    let (dictations, dictations_v) = number("dictations today");
    let (words, words_v) = number("words today");
    let (speed, speed_v) = number("avg transcribe");
    row.append(&dictations);
    row.append(&words);
    row.append(&speed);
    let engine = Label::new(Some("engine · —"));
    engine.add_css_class("muted");
    engine.set_halign(Align::End);
    engine.set_hexpand(true);
    engine.set_valign(Align::End);
    row.append(&engine);

    let history = store::data_dir().join("history.jsonl");
    let backend = store::data_dir().join("backend.state");
    let refresh = {
        let (dictations_v, words_v, speed_v) =
            (dictations_v.clone(), words_v.clone(), speed_v.clone());
        move || {
            let t = status::read_today(&history);
            dictations_v.set_text(&t.dictations.to_string());
            words_v.set_text(&t.words.to_string());
            speed_v.set_text(
                &t.avg_ms
                    .map(|ms| format!("{:.1} s", ms as f64 / 1000.0))
                    .unwrap_or_else(|| "—".into()),
            );
        }
    };
    refresh();
    glib::timeout_add_local(Duration::from_millis(2000), move || {
        refresh();
        ControlFlow::Continue
    });

    let state = state.clone();
    glib::timeout_add_local(Duration::from_millis(400), move || {
        let word = status::read_state();
        if state.text() != word {
            state.set_text(word);
        }
        ControlFlow::Continue
    });

    let probe = {
        let engine = engine.clone();
        move || {
            let last = std::fs::read_to_string(&backend).unwrap_or_default();
            let engine = engine.clone();
            glib::spawn_future_local(async move {
                let up = gio::spawn_blocking(status::reachable).await.ok().flatten();
                engine.set_text(&format!(
                    "engine · last {} · {}",
                    status::backend_label(last.trim()),
                    up.map(|n| format!("{n} answers"))
                        .unwrap_or_else(|| "no host answers".into())
                ));
            });
        }
    };
    probe();
    glib::timeout_add_local(Duration::from_millis(10_000), move || {
        probe();
        ControlFlow::Continue
    });
    row
}

fn number(caption: &str) -> (Box, Label) {
    let column = Box::new(Orientation::Vertical, 2);
    let value = Label::new(Some("—"));
    value.add_css_class("number");
    value.set_halign(Align::Start);
    let label = Label::new(Some(caption));
    label.add_css_class("muted");
    label.set_halign(Align::Start);
    column.append(&value);
    column.append(&label);
    (column, value)
}

/// The dictation settings voice-flow reads from the conf on every press.
/// Each control writes its one key in place; nothing restarts.
fn controls_row() -> Box {
    let conf = store::load_conf(&store::conf_path());
    let row = Box::new(Orientation::Horizontal, 8);

    let method_label = Label::new(Some("INJECT"));
    method_label.add_css_class("section");
    row.append(&method_label);
    let current = conf
        .get("VOICE_FLOW_METHOD")
        .map(String::as_str)
        .unwrap_or("auto");
    let mut group: Option<ToggleButton> = None;
    for value in ["auto", "type", "paste"] {
        let button = ToggleButton::with_label(value);
        if let Some(first) = &group {
            button.set_group(Some(first));
        }
        button.set_active(value == current);
        button.connect_toggled(move |b| {
            if b.is_active() {
                let _ = store::save_key(&store::conf_path(), "VOICE_FLOW_METHOD", value);
            }
        });
        row.append(&button);
        if group.is_none() {
            group = Some(button);
        }
    }

    for (caption, key, on, off) in [
        ("AI cleanup", "VOICE_FLOW_REWRITE", "on", "off"),
        ("Pill", "VOICE_FLOW_PILL", "1", "0"),
        ("Live words", "VOICE_FLOW_PARTIALS", "1", "0"),
        ("Keep text in history", "VOICE_FLOW_LOG_TEXT", "1", "0"),
    ] {
        let default = if key == "VOICE_FLOW_LOG_TEXT" {
            off
        } else {
            on
        };
        let active = conf.get(key).map(String::as_str).unwrap_or(default) == on;
        let switch = Switch::new();
        switch.set_active(active);
        switch.set_valign(Align::Center);
        switch.connect_state_set(move |_, state| {
            let _ = store::save_key(&store::conf_path(), key, if state { on } else { off });
            glib::Propagation::Proceed
        });
        let caption = Label::new(Some(caption));
        caption.add_css_class("muted");
        let pair = Box::new(Orientation::Horizontal, 6);
        pair.set_margin_start(10);
        pair.append(&switch);
        pair.append(&caption);
        row.append(&pair);
    }
    row
}

/// Dictate and Agent run voice-flow as the key bind does; Analytics opens
/// the web page; App profiles opens the terminal list in the editor.
fn actions_row() -> Box {
    let row = Box::new(Orientation::Horizontal, 8);
    for (caption, command) in [
        ("Dictate · Super+H", vec!["voice-flow"]),
        ("Agent", vec!["voice-flow", "agent"]),
        ("Analytics", vec!["voice-flow-web"]),
    ] {
        let button = Button::with_label(caption);
        button.connect_clicked(move |_| {
            let _ = std::process::Command::new(command[0])
                .args(&command[1..])
                .spawn();
        });
        row.append(&button);
    }
    let profiles = Button::with_label("App profiles");
    profiles.connect_clicked(|_| {
        let path = store::config_dir().join("app-profiles.conf");
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    });
    row.append(&profiles);
    row
}
