mod state;

use state::{Events, State};

use std::{
    io::{stdout, Error, Stdout},
    path::{Path, PathBuf},
    time::Duration,
    vec,
};

use crossterm::{
    event::{
        poll, read, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent,
        KeyEventKind, KeyEventState, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

use tui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};

fn main() -> Result<(), Error> {
    // Read path arg or default to current dir. Panic is ok.
    let p = std::env::args()
        .nth(1)
        .map_or_else(|| std::env::current_dir().unwrap(), PathBuf::from);

    // Create state
    let mut state = State {
        results: vec![],
        time: 0.0,
    };

    // Scan
    let start = std::time::Instant::now();
    println!("Scanning...");
    if let Err(e) = scan(&p, &mut state.results) {
        println!("Scanning failed: {e}");
        std::process::exit(1);
    }
    state.time = start.elapsed().as_secs_f32();

    // Quit if not results
    if state.results.is_empty() {
        println!("No target folders found!");
        std::process::exit(0);
    }

    // Create stateful widget state
    let mut events = Events::new(state.results.clone());
    events.next();

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Draw initial screen
    terminal.draw(|f| draw(f, &mut state, &mut events))?;

    // Poll for events every 100 millis. If got one, handle it, and draw again
    loop {
        if let Ok(true) = poll(Duration::from_millis(100)) {
            if let Ok(event) = read() {
                if let Err(e) = handle_event(&event, &mut terminal, &mut state, &mut events) {
                    println!("Error: {e}");
                    std::process::exit(2);
                }
                // Update on event
                terminal.draw(|f| draw(f, &mut state, &mut events))?;
            }
        }
    }
}

fn scan(path: &Path, results: &mut Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    // println!("Scanning: {path:?}");
    match std::fs::read_dir(path) {
        Ok(dir) => {
            let mut found_target = false;
            let mut found_cargo_toml = false;

            // Loop through every file in folder
            for entry in dir.flatten() {
                // Skip hidden files
                if entry.file_name().to_str().unwrap().starts_with('.') {
                    continue;
                }

                // Skip symlinks
                if let Ok(meta) = &entry.metadata() {
                    if meta.is_symlink() {
                        continue;
                    }

                    // Check if folder named target
                    if entry.file_name() == "target" && meta.is_dir() {
                        found_target = true;
                        continue;
                    }

                    // Check cargo toml
                    if entry.file_name() == "Cargo.toml" && meta.is_file() {
                        found_cargo_toml = true;
                    }
                }
            }

            if found_target && found_cargo_toml {
                let p = path.to_path_buf().join("target");
                results.push(p.to_str().unwrap().to_string());
            }

            // Aight bet, loop again
            let dir = std::fs::read_dir(path)?;

            for entry in dir.flatten() {
                if entry.file_type().unwrap().is_dir() {
                    scan(&entry.path(), results).unwrap();
                }
            }
        }
        Err(e) => {
            println!("Cannot scan {path:?}: {}", e.kind());
        }
    }

    Ok(())
}

fn trash_selected(state: &mut State, events: &mut Events) {
    if let Some(idx) = events.state.selected() {
        if let Some(path) = &state.results.get(idx) {
            if trash::delete(path).is_ok() {
                state.results.remove(idx);
                events.items.remove(idx);
            }
        }
    }
}

fn trash_all(state: &mut State, events: &mut Events) {
    for path in &state.results {
        trash::delete(path).unwrap();
    }
    state.results.clear();
    events.clear();
}

fn draw(f: &mut Frame<CrosstermBackend<Stdout>>, state: &mut State, events: &mut Events) {
    let size = f.size();
    let block = Block::default()
        .title(format!(
            "Found {} target folders ({:.2}s)",
            state.results.len(),
            state.time
        ))
        .borders(Borders::ALL);

    let items: Vec<ListItem> = events
        .items
        .iter()
        .map(|s| ListItem::new(s.as_ref()))
        .collect();

    let list = List::new(items)
        .block(block)
        .style(Style::default())
        .highlight_style(Style::default())
        .highlight_symbol(">>");

    let actions_block = Block::default().title("Actions").borders(Borders::ALL);

    let actions = Span::raw(
        "Select (Up/Down)  Trash all (a) Trash selected (Del) Quit (Esc)",
    );
    let paragraph = Paragraph::new(actions);

    // Rect
    let list_rect = Rect::new(0, 0, size.width, size.height - 3);
    let actions_rect = Rect::new(0, list_rect.height, size.width, 3);
    let paragraph_rect = actions_block.inner(actions_rect);

    f.render_stateful_widget(list, list_rect, &mut events.state);
    f.render_widget(actions_block, actions_rect);
    f.render_widget(paragraph, paragraph_rect);
}

fn handle_event(
    event: &Event,
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    state: &mut State,
    events: &mut Events,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        // Select previous
        Event::Key(KeyEvent {
            code: KeyCode::Up,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            events.previous();
        }

        // Select next
        Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            events.next();
        }

        // Trash one
        Event::Key(KeyEvent {
            code: KeyCode::Delete,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            trash_selected(state, events);
            events.next();
        }

        // Trash all
        Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }) => {
            trash_all(state, events);
            events.next();
        }

        // Exit
        Event::Key(
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }
            | KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            },
        ) => {
            // restore terminal
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            // Quit
            std::process::exit(0);
        }

        _ => (),
    }
    Ok(())
}
