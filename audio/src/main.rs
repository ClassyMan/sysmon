mod capture;
mod spectrum;
mod ui;

use std::io;
use std::panic;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::capture::AudioCapture;
use crate::spectrum::SpectrumAnalyzer;

const FFT_SIZE: usize = 4096;
const REFRESH_MS: u64 = 33; // ~30fps for smooth animation

fn main() -> Result<()> {
    let audio = AudioCapture::start_monitor()?;
    let device_name = audio.device_name.clone();

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

    let result = run(&mut terminal, &audio, &device_name);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    audio: &AudioCapture,
    device_name: &str,
) -> Result<()> {
    let mut analyzer = SpectrumAnalyzer::new(FFT_SIZE);
    let refresh_rate = Duration::from_millis(REFRESH_MS);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|frame| {
            ui::render(frame, &analyzer, audio.sample_rate, device_name);
        })?;

        let timeout = refresh_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_default();

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= refresh_rate {
            let samples = audio.take_samples(FFT_SIZE);
            analyzer.process(&samples);
            last_tick = Instant::now();
        }
    }
}
