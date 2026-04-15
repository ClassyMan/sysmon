use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

pub enum Panel {
    Cpu(cpu::app::App),
    Gpu(gpu::app::App),
    Ram(ram::app::App),
    Dio(dio::app::App),
    Net(net::app::App),
    Poly {
        app: poly::app::App,
        shared: Arc<Mutex<poly::collector::FetchState>>,
        _fetcher: JoinHandle<()>,
    },
    Astro {
        app: astro::app::App,
        shared: Arc<Mutex<astro::collector::FetchState>>,
        _fetcher: JoinHandle<()>,
    },
}

impl Panel {
    pub fn new_cpu(refresh_ms: u64, scrollback_secs: u64) -> Result<Self> {
        let mut app = cpu::app::App::new(refresh_ms, scrollback_secs);
        app.tick()?;
        Ok(Panel::Cpu(app))
    }

    pub fn new_gpu(refresh_ms: u64, scrollback_secs: u64) -> Result<Self> {
        let mut app = gpu::app::App::new(refresh_ms, scrollback_secs);
        app.tick()?;
        Ok(Panel::Gpu(app))
    }

    pub fn new_ram(refresh_ms: u64, scrollback_secs: u64) -> Result<Self> {
        let mut app = ram::app::App::new(refresh_ms, scrollback_secs);
        app.tick()?;
        Ok(Panel::Ram(app))
    }

    pub fn new_dio(refresh_ms: u64, scrollback_secs: u64) -> Result<Self> {
        let mut app = dio::app::App::new(refresh_ms, scrollback_secs, false);
        app.tick()?;
        Ok(Panel::Dio(app))
    }

    pub fn new_net(refresh_ms: u64, scrollback_secs: u64) -> Result<Self> {
        let mut app = net::app::App::new(refresh_ms, scrollback_secs);
        app.tick()?;
        Ok(Panel::Net(app))
    }

    pub fn new_poly(refresh_ms: u64) -> Self {
        let shared = Arc::new(Mutex::new(poly::collector::FetchState::new(refresh_ms)));
        let fetcher = poly::collector::spawn_fetcher(shared.clone());
        let app = poly::app::App::new(shared.clone(), refresh_ms);
        Panel::Poly {
            app,
            shared,
            _fetcher: fetcher,
        }
    }

    pub fn new_astro(api_key: String) -> Self {
        let shared = Arc::new(Mutex::new(astro::collector::FetchState::new(
            3_600_000,
            api_key,
        )));
        let fetcher = astro::collector::spawn_fetcher(shared.clone());
        let app = astro::app::App::new(shared.clone());
        Panel::Astro {
            app,
            shared,
            _fetcher: fetcher,
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        match self {
            Panel::Cpu(app) => app.tick(),
            Panel::Gpu(app) => app.tick(),
            Panel::Ram(app) => app.tick(),
            Panel::Dio(app) => app.tick(),
            Panel::Net(app) => app.tick(),
            Panel::Poly { app, .. } => {
                app.tick();
                Ok(())
            }
            Panel::Astro { app, .. } => {
                app.tick();
                Ok(())
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        match self {
            Panel::Cpu(app) => cpu::ui::render_in(frame, area, app),
            Panel::Gpu(app) => gpu::ui::render_in(frame, area, app),
            Panel::Ram(app) => ram::ui::render_in(frame, area, app),
            Panel::Dio(app) => dio::ui::render_in(frame, area, app),
            Panel::Net(app) => net::ui::render_in(frame, area, app),
            Panel::Poly { app, .. } => poly::ui::render_in(frame, area, app),
            Panel::Astro { app, .. } => astro::ui::render_in(frame, area, app),
        }
    }

    pub fn toggle_fast_mode(&mut self) {
        match self {
            Panel::Cpu(app) => app.toggle_fast_mode(),
            Panel::Gpu(app) => app.toggle_fast_mode(),
            Panel::Ram(app) => app.toggle_fast_mode(),
            Panel::Dio(app) => {
                app.handle_action(dio::input::AppAction::ToggleFastMode);
            }
            Panel::Net(app) => app.toggle_fast_mode(),
            Panel::Poly { .. } => {}
            Panel::Astro { .. } => {}
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self {
            Panel::Dio(app) => {
                let action = dio::input::map_key(key);
                app.handle_action(action);
            }
            Panel::Net(app) => match key.code {
                KeyCode::Char('v') => app.toggle_view(),
                KeyCode::Char('d') | KeyCode::Right => app.next_interface(),
                KeyCode::Char('D') | KeyCode::Left => app.prev_interface(),
                _ => {}
            },
            Panel::Poly { app, .. } => match key.code {
                KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                KeyCode::Char('t') | KeyCode::Right => app.cycle_topic(),
                KeyCode::Char('T') | KeyCode::Left => app.cycle_topic_prev(),
                KeyCode::Char('s') => app.cycle_sort(),
                _ => {}
            },
            Panel::Astro { app, .. } => match key.code {
                KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                KeyCode::Char('v') => app.toggle_view(),
                KeyCode::Char('J') => app.scroll_down(),
                KeyCode::Char('K') => app.scroll_up(),
                _ => {}
            },
            _ => {}
        }
    }

    pub fn refresh_rate(&self) -> Duration {
        match self {
            Panel::Cpu(app) => app.refresh_rate(),
            Panel::Gpu(app) => app.refresh_rate(),
            Panel::Ram(app) => app.refresh_rate(),
            Panel::Dio(app) => app.refresh_rate,
            Panel::Net(app) => app.refresh_rate(),
            Panel::Poly { .. } => Duration::from_millis(500),
            Panel::Astro { .. } => Duration::from_millis(500),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Panel::Cpu(_) => "CPU",
            Panel::Gpu(_) => "GPU",
            Panel::Ram(_) => "RAM",
            Panel::Dio(_) => "DIO",
            Panel::Net(_) => "NET",
            Panel::Poly { .. } => "POLY",
            Panel::Astro { .. } => "ASTRO",
        }
    }

    pub fn stop(&mut self) {
        match self {
            Panel::Poly { shared, .. } => {
                shared.lock().unwrap().should_stop = true;
            }
            Panel::Astro { shared, .. } => {
                shared.lock().unwrap().should_stop = true;
            }
            _ => {}
        }
    }
}
