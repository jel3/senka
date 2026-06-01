mod app;
mod form;
mod ui;

use std::io;

use crossterm::event;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub async fn run() -> anyhow::Result<()> {
    // Build app before entering raw mode so errors print normally
    let mut app = app::App::new()?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Install panic hook that restores terminal
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        let _ = execute!(io::stdout(), crossterm::cursor::Show);
        original_hook(panic_info);
    }));

    // Drain any buffered input events (e.g. the Enter key that launched this command)
    while event::poll(std::time::Duration::from_millis(0))? {
        let _ = event::read()?;
    }

    // Main loop
    let result = run_loop(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut app::App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll for events with 50ms timeout
        if event::poll(std::time::Duration::from_millis(50))? {
            let ev = event::read()?;
            app.handle_event(ev);
        }

        // Drain async results
        app.tick();

        if app.should_quit {
            return Ok(());
        }
    }
}
