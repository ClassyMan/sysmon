mod app;
mod collector;
mod theme;
mod ui;

use std::io;
use std::panic;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::collector::FetchState;

#[derive(Parser)]
#[command(
    name = "astro",
    about = "NASA Astronomy Picture of the Day TUI viewer",
    version
)]
struct Cli {
    /// NASA API key
    #[arg(long, default_value = "2hDbCHnvukvQdR356ptvqGRJ1C0ud5S5TxUOO4Pr")]
    api_key: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    let result = run(&mut terminal, &cli.api_key);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, api_key: &str) -> Result<()> {
    let refresh_ms = 3_600_000;
    let shared = Arc::new(Mutex::new(FetchState::new(refresh_ms, api_key.to_string())));
    let _fetcher = collector::spawn_fetcher(shared.clone());
    let mut app = App::new(shared);

    let ui_refresh = Duration::from_millis(500);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        let timeout = ui_refresh
            .checked_sub(last_tick.elapsed())
            .unwrap_or_default();

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                    KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                    KeyCode::Char('v') => app.toggle_view(),
                    KeyCode::Char('J') => app.scroll_down(),
                    KeyCode::Char('K') => app.scroll_up(),
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= ui_refresh {
            app.tick();
            last_tick = Instant::now();
        }

        if app.should_quit {
            let mut state = app.shared.lock().unwrap();
            state.should_stop = true;
            break;
        }
    }

    Ok(())
}
