//! Headless checks that every screen renders and that key handling wires the
//! machine to the UI correctly.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use renigma::app::{App, Screen};
use renigma::config::Config;
use renigma::ui;
use std::path::PathBuf;

fn app() -> App {
    App::new(Config::default(), PathBuf::from("/tmp/renigma-unused.toml"))
}

fn key(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

#[test]
fn renders_every_screen_without_panicking() {
    for (w, h) in [(80, 24), (100, 30), (132, 40)] {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = app();

        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();

        // Configuration, every tab.
        app.screen = Screen::Config;
        for tab in 0..6 {
            app.tab = tab;
            app.row = 0;
            terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
        }

        // Help and the custom-rotor form.
        app.under = Screen::Config;
        app.screen = Screen::Help;
        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();

        app.screen = Screen::RotorForm;
        app.form = Some(renigma::app::RotorForm::new(None, None));
        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();

        // A four-rotor machine too.
        app.form = None;
        app.screen = Screen::Config;
        app.tab = 0;
        app.row = 0;
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
        assert_eq!(app.machine.rotors.len(), 4);
    }
}

#[test]
fn typing_encrypts_and_backspace_rewinds() {
    let mut app = app();
    for _ in 0..5 {
        key(&mut app, KeyCode::Char('a'));
    }
    assert_eq!(app.output, "BDZGO");
    assert_eq!(app.machine.positions(), vec!['A', 'A', 'F']);

    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.output, "BDZG");
    assert_eq!(app.machine.positions(), vec!['A', 'A', 'E']);

    ctrl(&mut app, 'r');
    assert_eq!(app.output, "");
    assert_eq!(app.machine.positions(), vec!['A', 'A', 'A']);
}

#[test]
fn tab_switches_to_configuration_and_back() {
    let mut app = app();
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.screen, Screen::Config);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Encrypt);
}

#[test]
fn count_toggle_builds_a_valid_m4() {
    let mut app = app();
    app.screen = Screen::Config;
    app.tab = 0;
    app.row = 0;
    key(&mut app, KeyCode::Right);
    assert_eq!(app.cfg.rotor_count(), 4);
    assert_eq!(app.machine.rotors.len(), 4);
    assert!(app.machine.reflector.is_thin());
    assert!(app.cfg.rotors.order.iter().any(|n| n == "Beta"));
    assert_eq!(app.cfg.rotors.order.len(), 4);

    // And back down again.
    key(&mut app, KeyCode::Left);
    assert_eq!(app.cfg.rotor_count(), 3);
    assert!(!app.machine.reflector.is_thin());
}

#[test]
fn plugboard_pairing_round_trips() {
    let mut app = app();
    app.screen = Screen::Config;
    app.tab = 3;
    app.plug_cursor = 0; // A
    key(&mut app, KeyCode::Enter);
    app.plug_cursor = 2; // C
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.cfg.plugboard, vec!["AC".to_string()]);
    assert_eq!(app.machine.plug_for(0), Some(2));

    // Entering a plugged letter removes its pair.
    app.plug_cursor = 0;
    key(&mut app, KeyCode::Enter);
    assert!(app.cfg.plugboard.is_empty());
}

#[test]
fn theme_selection_applies_live() {
    let mut app = app();
    app.screen = Screen::Config;
    app.tab = 4;
    app.row = 3; // Dracula
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.cfg.theme, "dracula");
    assert_eq!(app.theme.name, "dracula");
}

#[test]
fn custom_rotor_can_be_created_and_used() {
    let mut app = app();
    app.screen = Screen::Config;
    app.tab = 5;
    key(&mut app, KeyCode::Char('n'));
    assert_eq!(app.screen, Screen::RotorForm);

    for c in "TEST".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Tab);
    for c in "ZYXWVUTSRQPONMLKJIHGFEDCBA".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Char('M'));
    key(&mut app, KeyCode::Enter);

    assert_eq!(app.screen, Screen::Config);
    assert_eq!(app.cfg.custom_rotors.len(), 1);
    assert_eq!(app.cfg.custom_rotors[0].name, "TEST");

    // Place it in the machine and encrypt with it.
    app.slot = 2;
    key(&mut app, KeyCode::Enter);
    assert!(app.cfg.rotors.order.contains(&"TEST".to_string()));
    app.screen = Screen::Encrypt;
    key(&mut app, KeyCode::Char('A'));
    assert_eq!(app.output.len(), 1);
}

#[test]
fn invalid_custom_rotor_is_rejected() {
    let mut app = app();
    app.screen = Screen::Config;
    app.tab = 5;
    key(&mut app, KeyCode::Char('n'));
    for c in "BAD".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Tab);
    for c in "ABC".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::RotorForm);
    assert!(app.form.as_ref().unwrap().error.is_some());
    assert!(app.cfg.custom_rotors.is_empty());
}

#[test]
fn quit_and_save_shortcuts() {
    let mut app = app();
    ctrl(&mut app, 'q');
    assert!(app.should_quit);
}

#[test]
fn stepping_rotor_flashes_in_lamp_colour() {
    let mut app = app();
    app.flash[2] = Some(std::time::Instant::now());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let lamp = Some(app.theme.lamp);
    let cells = terminal.backend().buffer().content();
    assert!(
        cells.iter().any(|cell| cell.style().bg == lamp),
        "stepped rotor should be painted with the lamp colour"
    );
}
