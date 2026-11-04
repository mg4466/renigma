use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use renigma::app::App;
use renigma::config::{Config, default_path};
use renigma::ui;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const HELP: &str = "\
Renigma — a terminal Enigma machine

USAGE:
    renigma [OPTIONS]

OPTIONS:
    -c, --config <PATH>   Use a specific config file
                          (default: $XDG_CONFIG_HOME/renigma/config.toml)
    -h, --help            Print this help
    -V, --version         Print the version

KEYS:
    A–Z         encrypt             TAB     configure
    Backspace   undo                ?       help
    Ctrl-R      reset rotors        Ctrl-S  save
    Ctrl-Q      quit";
const TICK: Duration = Duration::from_millis(90);

fn main() -> ExitCode {
    let mut path: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => match args.next() {
                Some(p) => path = Some(PathBuf::from(p)),
                None => {
                    eprintln!("renigma: --config needs a path");
                    return ExitCode::FAILURE;
                }
            },
            "-h" | "--help" => {
                println!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("renigma {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("renigma: unknown argument '{other}' (try --help)");
                return ExitCode::FAILURE;
            }
        }
    }

    let path = path.unwrap_or_else(default_path);
    let (cfg, notes) = Config::load(&path);

    // First run: write a config file so the format is discoverable.
    let first_run = !path.exists();
    let mut app = App::new(cfg, path.clone());
    if first_run
        && let Err(e) = app.cfg.save(&path) {
            eprintln!("renigma: could not create {}: {e}", path.display());
        }
    if let Some(note) = notes.first() {
        app.toast(note.clone());
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();

    match result {
        Ok(()) => {
            match app.cfg.save(&path) {
                Ok(()) => println!("renigma: settings saved to {}", path.display()),
                Err(e) => eprintln!("renigma: could not save {}: {e}", path.display()),
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("renigma: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
                _ => {}
            }
        }
        app.tick();
        if app.should_quit {
            return Ok(());
        }
    }
}
