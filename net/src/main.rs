mod app;
mod collector;
mod ui;

use std::io;
use std::panic;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;

#[derive(Parser)]
#[command(
    name = "net",
    about = "Network activity visualizer with mirrored waveform display",
    version
)]
struct Cli {
    /// Refresh interval in milliseconds
    #[arg(short = 'r', long, default_value = "500")]
    refresh: u64,

    /// Scrollback duration in seconds
    #[arg(short = 's', long, default_value = "60")]
    scrollback: u64,
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

    let result = run(&mut terminal, cli);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, cli: Cli) -> Result<()> {
    let mut app = App::new(cli.refresh, cli.scrollback);

    app.tick()?;

    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| ui::render(frame, &app))?;

        let timeout = app
            .refresh_rate()
            .checked_sub(last_tick.elapsed())
            .unwrap_or_default();

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('f') => {
                        app.toggle_fast_mode();
                        last_tick = Instant::now();
                    }
                    KeyCode::Char('d') | KeyCode::Right => app.next_interface(),
                    KeyCode::Char('D') | KeyCode::Left => app.prev_interface(),
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= app.refresh_rate() {
            app.tick()?;
            last_tick = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
