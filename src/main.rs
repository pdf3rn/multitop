//! multitop — a cross-platform interactive process viewer that works like htop 3.3.0.
//! Runs on Linux, macOS and Windows (terminal drawing through crossterm, system data
//! through sysinfo). See MECHANICS.md for the rules taken from the original.

mod app;
mod cli;
mod data;
mod fields;
mod screen;
mod settings;

use crossterm::event::{self, Event, KeyEventKind};
use std::io::Write;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // The user list is only read when -u needs it.
    let user_exists = |name: &str| data::Sampler::new().user_exists(name);
    let outcome = cli::parse(&args, &user_exists);
    let opts = match outcome {
        cli::Outcome::Exit(code) => std::process::exit(code),
        cli::Outcome::Run(o) => o,
    };
    let data = data::Sampler::new();
    let settings = settings::Settings::load();
    let mut app = app::App::new(settings, opts.clone(), data);

    if let Some(path) = &opts.screenshot {
        std::process::exit(screenshot(&mut app, path));
    }
    let code = run(&mut app, &opts);
    if app.s.changed {
        if let Err(e) = app.s.save() {
            eprintln!("{e}");
        }
    }
    std::process::exit(code);
}

/// Size used for --screenshot: COLUMNS/LINES when given, else the terminal's, else 120x36.
fn shot_size() -> (usize, usize) {
    let env = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<usize>().ok()).filter(|v| *v >= 20);
    if let (Some(w), Some(h)) = (env("COLUMNS"), env("LINES")) {
        return (w, h);
    }
    (120, 36)
}

fn screenshot(app: &mut app::App, path: &std::path::Path) -> i32 {
    // Same start-up as the original: two scans 75 ms apart, then a steady refresh.
    app.scan();
    std::thread::sleep(Duration::from_millis(75));
    app.scan();
    std::thread::sleep(Duration::from_millis(500));
    app.scan();
    app.rebuild();
    let (w, h) = shot_size();
    let mut buf = app.draw(w, h);
    for frame in 0..120 {
        if frame % 40 == 39 {
            std::thread::sleep(Duration::from_millis(20));
        }
        buf = app.draw(w, h);
    }
    if std::env::var_os("MULTITOP_SHOT_TEXT").is_some() {
        for y in 0..buf.h {
            let line: String = buf.cells[y * buf.w..(y + 1) * buf.w].iter().map(|c| c.ch).collect();
            println!("{}", line.trim_end());
        }
    }
    match screen::save_png(&buf, path, app.s.monochrome) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Error: could not save the picture to {}: {e}", path.display());
            1
        }
    }
}

struct TermGuard {
    mouse: bool,
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        restore(self.mouse);
    }
}

fn restore(mouse: bool) {
    let mut out = std::io::stdout();
    if mouse {
        let _ = crossterm::execute!(out, crossterm::event::DisableMouseCapture);
    }
    let _ = crossterm::execute!(
        out,
        crossterm::style::ResetColor,
        crossterm::cursor::Show,
        crossterm::terminal::LeaveAlternateScreen
    );
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = out.flush();
}

fn run(app: &mut app::App, opts: &cli::Opts) -> i32 {
    use crossterm::{cursor, execute, terminal};
    if crossterm::tty::IsTty::is_tty(&std::io::stdout()) == false {
        eprintln!("Error opening terminal: this program needs an interactive terminal (try --screenshot FILE).");
        return 1;
    }
    let mouse = app.s.enable_mouse && !opts.no_mouse;
    if terminal::enable_raw_mode().is_err() {
        eprintln!("Error opening terminal: unknown.");
        return 1;
    }
    let mut out = std::io::stdout();
    let _ = execute!(out, terminal::EnterAlternateScreen, cursor::Hide, terminal::Clear(terminal::ClearType::All));
    if mouse {
        let _ = execute!(out, crossterm::event::EnableMouseCapture);
    }
    let _guard = TermGuard { mouse };
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore(mouse);
        prev_hook(info);
    }));

    app.scan();
    std::thread::sleep(Duration::from_millis(75));
    app.scan();
    app.rebuild();
    let mut iterations: u64 = 0;
    let mut last_scan = Instant::now();
    let mut prev: Option<screen::Buf> = None;
    let mut force_full = true;
    loop {
        let (w, h) = terminal::size().map(|(w, h)| (w as usize, h as usize)).unwrap_or((80, 24));
        let buf = app.draw(w, h);
        let _ = screen::flush(&mut out, &buf, if force_full { None } else { prev.as_ref() }, app.s.monochrome);
        prev = Some(buf);
        force_full = false;
        if app.quit {
            break;
        }
        if let Some(max) = opts.max_iter {
            if iterations >= max {
                break;
            }
        }
        let delay = app.delay();
        let wait = delay.saturating_sub(last_scan.elapsed());
        if event::poll(wait).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => {
                    if k.code == crossterm::event::KeyCode::Char('l')
                        && k.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        force_full = true;
                        let _ = execute!(out, terminal::Clear(terminal::ClearType::All));
                    }
                    app.key(k);
                    app.rebuild();
                }
                Ok(Event::Mouse(m)) => {
                    app.mouse(m);
                }
                Ok(Event::Resize(_, _)) => {
                    force_full = true;
                    let _ = execute!(out, terminal::Clear(terminal::ClearType::All));
                }
                _ => {}
            }
            if app.quit {
                break;
            }
        }
        if last_scan.elapsed() >= delay {
            if !app.paused {
                app.scan();
                app.rebuild();
            }
            last_scan = Instant::now();
            iterations += 1;
        }
    }
    0
}
