mod app;
mod collector;
mod ui;

use std::io;
use std::panic;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::collector::FetchState;

#[derive(Parser)]
#[command(
    name = "poly",
    about = "Polymarket prediction market TUI dashboard",
    version
)]
struct Cli {
    /// Refresh interval in seconds
    #[arg(short = 'r', long, default_value = "30")]
    refresh: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let refresh_ms = cli.refresh * 1000;

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

    let result = run(&mut terminal, refresh_ms);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, refresh_ms: u64) -> Result<()> {
    let shared = Arc::new(Mutex::new(FetchState::new(refresh_ms)));
    let _fetcher = collector::spawn_fetcher(shared.clone());
    let mut app = App::new(shared, refresh_ms);

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
                    KeyCode::Char('f') => app.toggle_fast_mode(),
                    KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                    KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                    KeyCode::Char('t') | KeyCode::Right => app.cycle_topic(),
                    KeyCode::Char('T') | KeyCode::Left => app.cycle_topic_prev(),
                    KeyCode::Char('s') => app.cycle_sort(),
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
