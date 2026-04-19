mod layout;
mod panel;

use std::io;
use std::panic;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui_image::picker::Picker;
use ratatui::backend::CrosstermBackend;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders};
use ratatui::Terminal;

use panel::Panel;

#[derive(Parser)]
#[command(
    name = "sysmon",
    about = "System monitor dashboard",
    version
)]
struct Cli {
    #[arg(long)]
    cpu: bool,
    #[arg(long)]
    gpu: bool,
    #[arg(long)]
    ram: bool,
    #[arg(long)]
    dio: bool,
    #[arg(long)]
    net: bool,
    #[arg(long)]
    poly: bool,
    #[arg(long)]
    astro: bool,
    #[arg(long)]
    audio: bool,

    /// Refresh interval in milliseconds
    #[arg(short = 'r', long, default_value = "500")]
    refresh: u64,

    /// Scrollback duration in seconds
    #[arg(short = 's', long, default_value = "60")]
    scrollback: u64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let picker = Picker::from_query_stdio().ok();

    terminal::enable_raw_mode()?;
    sysmon_shared::terminal_theme::init();
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

    let result = run(&mut terminal, cli, picker);

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, cli: Cli, mut picker: Option<Picker>) -> Result<()> {
    let any_selected = cli.cpu || cli.gpu || cli.ram || cli.dio || cli.net || cli.poly || cli.astro || cli.audio;

    let mut panels: Vec<Panel> = Vec::new();

    let want_cpu = cli.cpu || !any_selected;
    let want_ram = cli.ram || !any_selected;
    let want_net = cli.net || !any_selected;

    if want_cpu {
        panels.push(Panel::new_cpu(cli.refresh, cli.scrollback)?);
    }
    if cli.gpu {
        panels.push(Panel::new_gpu(cli.refresh, cli.scrollback)?);
    }
    if want_ram {
        panels.push(Panel::new_ram(cli.refresh, cli.scrollback)?);
    }
    if cli.dio {
        panels.push(Panel::new_dio(cli.refresh, cli.scrollback, picker.as_mut())?);
    }
    if want_net {
        panels.push(Panel::new_net(cli.refresh, cli.scrollback)?);
    }
    if cli.poly {
        panels.push(Panel::new_poly(30000));
    }
    if cli.astro {
        panels.push(Panel::new_astro("2hDbCHnvukvQdR356ptvqGRJ1C0ud5S5TxUOO4Pr".to_string()));
    }
    if cli.audio {
        panels.push(Panel::new_audio()?);
    }

    if panels.is_empty() {
        return Ok(());
    }

    let mut last_ticks: Vec<Instant> = panels.iter().map(|_| Instant::now()).collect();
    let mut focused: usize = panels
        .iter()
        .position(|p| matches!(p, Panel::Poly { .. }))
        .unwrap_or(0);

    let focus_color = sysmon_shared::terminal_theme::palette().bright_cyan();
    let unfocused_color = sysmon_shared::terminal_theme::palette().surface();

    loop {
        terminal.draw(|frame| {
            let rects = layout::compute_grid(frame.area(), panels.len());
            for (i, (panel, &rect)) in panels.iter_mut().zip(rects.iter()).enumerate() {
                let border_color = if i == focused { focus_color } else { unfocused_color };
                let block = Block::default()
                    .title(format!(" {} ", panel.name()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color));
                let inner = block.inner(rect);
                frame.render_widget(block, rect);
                panel.render(frame, inner);
            }
        })?;

        let min_remaining = panels
            .iter()
            .zip(last_ticks.iter())
            .map(|(panel, last)| {
                panel
                    .refresh_rate()
                    .checked_sub(last.elapsed())
                    .unwrap_or_default()
            })
            .min()
            .unwrap_or_default()
            .min(std::time::Duration::from_millis(80));

        if event::poll(min_remaining)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('f') => {
                        for (panel, last) in panels.iter_mut().zip(last_ticks.iter_mut()) {
                            panel.toggle_fast_mode();
                            *last = Instant::now();
                        }
                    }
                    KeyCode::Tab => {
                        focused = (focused + 1) % panels.len();
                    }
                    KeyCode::BackTab => {
                        focused = if focused == 0 { panels.len() - 1 } else { focused - 1 };
                    }
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(target) = layout::neighbor(focused, panels.len(), layout::Direction::Up) {
                            panels.swap(focused, target);
                            last_ticks.swap(focused, target);
                            focused = target;
                        }
                    }
                    KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(target) = layout::neighbor(focused, panels.len(), layout::Direction::Down) {
                            panels.swap(focused, target);
                            last_ticks.swap(focused, target);
                            focused = target;
                        }
                    }
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(target) = layout::neighbor(focused, panels.len(), layout::Direction::Left) {
                            panels.swap(focused, target);
                            last_ticks.swap(focused, target);
                            focused = target;
                        }
                    }
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(target) = layout::neighbor(focused, panels.len(), layout::Direction::Right) {
                            panels.swap(focused, target);
                            last_ticks.swap(focused, target);
                            focused = target;
                        }
                    }
                    _ => {
                        panels[focused].handle_key(key);
                    }
                }
            }
        }

        for (i, (panel, last)) in panels.iter_mut().zip(last_ticks.iter_mut()).enumerate() {
            if last.elapsed() >= panel.refresh_rate() {
                if let Err(e) = panel.tick() {
                    eprintln!("Panel {} tick error: {}", i, e);
                }
                *last = Instant::now();
            }
        }
    }

    for panel in &mut panels {
        panel.stop();
    }

    Ok(())
}
