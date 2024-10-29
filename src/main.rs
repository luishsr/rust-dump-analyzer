use std::fs::File;
use std::io::{self, Read};
use std::env;
use std::path::Path;
use std::time::Duration;
use memchr::memmem;
use crossterm::{execute, terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType}};
use crossterm::event::{self, Event, KeyCode, KeyEvent, poll};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Span, Line, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Clear as ClearPopup},
    Terminal,
};

#[derive(Debug, Clone)]
struct Pattern {
    name: &'static str,
    bytes: &'static [u8],
}

struct ResultEntry {
    content: String,
    address: usize,
    bytes: Vec<u8>,
}

struct Summary {
    total_entries: usize,
    total_patterns: usize,
    total_ascii_strings: usize,
}

// Reads the binary memory dump file
fn read_dump_file(filename: &str) -> io::Result<Vec<u8>> {
    let mut file = File::open(filename)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer)
}

// Converts a slice of bytes to a formatted hex dump
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02X} ", byte)).collect::<String>()
}

// Converts a hex string input by the user into a vector of bytes
fn hex_string_to_bytes(hex: &str) -> Option<Vec<u8>> {
    hex.chars()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| {
            let hex_str = chunk.iter().collect::<String>();
            u8::from_str_radix(&hex_str, 16).ok()
        })
        .collect()
}

// Finds ASCII strings in a chunk and records their positions
fn find_ascii_strings(chunk: &[u8], chunk_offset: usize, min_length: usize) -> Vec<ResultEntry> {
    let mut result = Vec::new();
    let mut current_string = Vec::new();
    let mut start_index = 0;

    for (i, &byte) in chunk.iter().enumerate() {
        if byte.is_ascii_graphic() || byte == b' ' {
            if current_string.is_empty() {
                start_index = i;
            }
            current_string.push(byte);
        } else if current_string.len() >= min_length {
            result.push(ResultEntry {
                content: format!("ASCII String '{}'", String::from_utf8_lossy(&current_string)),
                address: chunk_offset + start_index,
                bytes: current_string.clone(),
            });
            current_string.clear();
        } else {
            current_string.clear();
        }
    }

    if current_string.len() >= min_length {
        result.push(ResultEntry {
            content: format!("ASCII String '{}'", String::from_utf8_lossy(&current_string)),
            address: chunk_offset + start_index,
            bytes: current_string,
        });
    }

    result
}

// Detects specific byte patterns in a chunk
fn detect_patterns(chunk: &[u8], chunk_offset: usize, patterns: &[Pattern]) -> Vec<ResultEntry> {
    let mut results = Vec::new();

    for pattern in patterns {
        let mut start = 0;
        while let Some(pos) = memmem::find(&chunk[start..], pattern.bytes) {
            let actual_pos = chunk_offset + start + pos;
            results.push(ResultEntry {
                content: format!("Pattern '{}'", pattern.name),
                address: actual_pos,
                bytes: pattern.bytes.to_vec(),
            });
            start += pos + 1;
        }
    }

    results
}

// Analyzes the memory dump sequentially, returns processed results and summary statistics
fn analyze_dump(
    filename: &str,
    patterns: &[Pattern],
    chunk_size: usize,
    min_string_length: usize,
) -> io::Result<(Vec<ResultEntry>, Summary)> {
    let data = read_dump_file(filename)?;
    println!("File loaded, data size: {} bytes", data.len());

    let mut all_results = Vec::new();
    let mut pattern_count = 0;
    let mut ascii_count = 0;

    for (i, chunk) in data.chunks(chunk_size).enumerate() {
        let chunk_offset = i * chunk_size;
        println!("Processing chunk at offset 0x{:X}", chunk_offset);

        let ascii_results = find_ascii_strings(&chunk, chunk_offset, min_string_length);
        ascii_count += ascii_results.len();

        let pattern_results = detect_patterns(&chunk, chunk_offset, patterns);
        pattern_count += pattern_results.len();

        all_results.extend(ascii_results);
        all_results.extend(pattern_results);

        println!("Finished processing chunk at offset 0x{:X}", chunk_offset);
    }

    let summary = Summary {
        total_entries: all_results.len(),
        total_patterns: pattern_count,
        total_ascii_strings: ascii_count,
    };

    Ok((all_results, summary))
}

#[derive(PartialEq)]
enum PopupMode {
    None,
    Search,
    GoToAddress,
}

// Runs the interactive UI with a menu, two-column layout, summary panel, and smooth scrolling
fn run_ui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    results: &[ResultEntry],
    summary: &Summary,
) -> io::Result<()> {
    let mut selected_index = 0;
    let mut scroll_offset = 0;
    let max_scroll = results.len().saturating_sub(1);
    let mut popup_mode = PopupMode::None;
    let mut input = String::new();
    let mut error_message = None;

    loop {
        terminal.draw(|f| {
            let size = f.area();

            // Layout with an additional row for summary at the bottom
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(3)].as_ref())
                .split(size);

            // Menu bar
            let menu_content = match popup_mode {
                PopupMode::None => "q: Quit | ↑↓: Navigate | /: Search | g: Go to Address".to_string(),
                PopupMode::Search => format!("Search: {} (Enter to search, Esc to cancel)", input),
                PopupMode::GoToAddress => format!("Go to Address: {} (Enter to jump, Esc to cancel)", input),
            };
            let menu = Paragraph::new(Text::from(Line::from(vec![
                Span::styled(menu_content, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            ])))
                .style(Style::default().bg(Color::Blue))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(menu, chunks[0]);

            // Main layout with two columns
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
                .split(chunks[1]);

            // Calculate the height of the visible list area
            let list_height = main_chunks[0].height as usize;

            // Left column: hex dump of bytes, showing the portion within scroll_offset
            let visible_items = results
                .iter()
                .enumerate()
                .skip(scroll_offset)
                .take(list_height)
                .map(|(i, result)| {
                    let style = if i == selected_index {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Gray)
                    };
                    ListItem::new(Text::from(Line::from(Span::styled(
                        format!("0x{:X}: {}", result.address, bytes_to_hex(&result.bytes)),
                        style,
                    ))))
                })
                .collect::<Vec<_>>();

            let list = List::new(visible_items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled("Hex Dump", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))))
                .highlight_symbol(">> ");

            f.render_widget(list, main_chunks[0]);

            // Right column: details of the selected entry with context
            if let Some(selected) = results.get(selected_index) {
                let context_bytes = results.iter()
                    .filter(|e| e.address >= selected.address.saturating_sub(16) && e.address <= selected.address + 16)
                    .map(|e| format!("0x{:X}: {}", e.address, bytes_to_hex(&e.bytes)))
                    .collect::<Vec<_>>()
                    .join("\n");

                let details = Paragraph::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled("Address: ", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
                        Span::raw(format!("0x{:X}", selected.address)),
                    ]),
                    Line::from(vec![
                        Span::styled("Content: ", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
                        Span::raw(&selected.content),
                    ]),
                    Line::from(vec![
                        Span::styled("Bytes: ", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
                        Span::raw(bytes_to_hex(&selected.bytes)),
                    ]),
                    Line::from(vec![
                        Span::styled("Context: ", Style::default().fg(Color::LightMagenta).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(vec![Span::raw(context_bytes)]),
                ]))
                    .block(Block::default().borders(Borders::ALL).title("Details"))
                    .wrap(ratatui::widgets::Wrap { trim: true });

                f.render_widget(details, main_chunks[1]);
            }

            // Adjust scroll offset to keep the selected item visible within the view
            if selected_index < scroll_offset {
                scroll_offset = selected_index;
            } else if selected_index >= scroll_offset + list_height {
                scroll_offset = selected_index - list_height + 1;
            }

            // Bottom summary panel
            let summary_text = format!(
                "Total Entries: {} | Patterns Detected: {} | ASCII Strings Found: {}",
                summary.total_entries, summary.total_patterns, summary.total_ascii_strings
            );
            let summary_paragraph = Paragraph::new(Text::from(Span::styled(
                summary_text,
                Style::default().fg(Color::Green),
            )))
                .block(Block::default().borders(Borders::ALL).title("Summary"));

            f.render_widget(summary_paragraph, chunks[2]);

            // Draw popup for search or go to address
            if popup_mode != PopupMode::None {
                let popup_area = centered_rect(50, 20, size);
                f.render_widget(ClearPopup, popup_area); // Clear the area under the popup
                let title = match popup_mode {
                    PopupMode::Search => "Search for ASCII or Hex",
                    PopupMode::GoToAddress => "Go to Address",
                    PopupMode::None => unreachable!(),
                };

                let popup = Paragraph::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(title, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                    ]),
                    Line::from(vec![
                        Span::styled(&input, Style::default().fg(Color::White)),
                    ]),
                    Line::from(vec![
                        Span::styled(error_message.as_deref().unwrap_or(""), Style::default().fg(Color::Red)),
                    ]),
                ]))
                    .block(Block::default().borders(Borders::ALL).title(title));
                f.render_widget(popup, popup_area);
            }
        })?;

        // Handle input events
        if poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match (code, &popup_mode) {
                    // Open the Search popup
                    (KeyCode::Char('/'), PopupMode::None) => {
                        popup_mode = PopupMode::Search;
                        input.clear();
                        error_message = None;
                    }
                    // Open the Go to Address popup
                    (KeyCode::Char('g'), PopupMode::None) => {
                        popup_mode = PopupMode::GoToAddress;
                        input.clear();
                        error_message = None;
                    }
                    // Execute the Search functionality
                    (KeyCode::Enter, PopupMode::Search) => {
                        if let Some(bytes) = hex_string_to_bytes(&input) {
                            if let Some(index) = results.iter().position(|e| e.bytes == bytes) {
                                selected_index = index;
                                popup_mode = PopupMode::None;
                            } else {
                                error_message = Some("No match found".to_string());
                            }
                        } else {
                            error_message = Some("Invalid hex format".to_string());
                        }
                    }
                    // Execute the Go to Address functionality
                    (KeyCode::Enter, PopupMode::GoToAddress) => {
                        if let Ok(addr) = usize::from_str_radix(input.trim_start_matches("0x"), 16) {
                            if let Some(index) = results.iter().position(|e| e.address >= addr) {
                                selected_index = index;
                                popup_mode = PopupMode::None;
                            } else {
                                error_message = Some("Address not found".to_string());
                            }
                        } else {
                            error_message = Some("Invalid address format".to_string());
                        }
                    }
                    // Cancel popup with ESC
                    (KeyCode::Esc, PopupMode::Search) | (KeyCode::Esc, PopupMode::GoToAddress) => {
                        popup_mode = PopupMode::None;
                    }
                    // Collect input in the popup mode
                    (KeyCode::Char(c), PopupMode::Search | PopupMode::GoToAddress) => input.push(c),
                    (KeyCode::Backspace, PopupMode::Search | PopupMode::GoToAddress) => { input.pop(); },

                    // Handle navigation keys only when not in a popup
                    (KeyCode::Up, PopupMode::None) => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        }
                    }
                    (KeyCode::Down, PopupMode::None) => {
                        if selected_index < max_scroll {
                            selected_index += 1;
                        }
                    }
                    (KeyCode::Esc | KeyCode::Char('q'), PopupMode::None) => break, // Exit on 'q' or ESC in normal mode
                    _ => {}
                }
            }
        }
    }

    Ok(())
}

// Centers a rectangle based on percentage width and height
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn main() -> io::Result<()> {
    let patterns = [
        Pattern { name: "PDF", bytes: b"%PDF" },
        Pattern { name: "JPEG", bytes: &[0xFF, 0xD8, 0xFF, 0xE0] },
        Pattern { name: "ZIP", bytes: &[0x50, 0x4B, 0x03, 0x04] },
        Pattern { name: "PNG", bytes: &[0x89, 0x50, 0x4E, 0x47] },
    ];

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        std::process::exit(1);
    }
    let filename = &args[1];
    if !Path::new(filename).exists() {
        eprintln!("Error: File '{}' not found.", filename);
        std::process::exit(1);
    }

    let chunk_size = 1024 * 1024;
    let min_string_length = 6;

    let (results, summary) = match analyze_dump(filename, &patterns, chunk_size, min_string_length) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error analyzing dump: {}", e);
            return Err(e);
        }
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, Clear(ClearType::All))?; // Clear terminal before launching UI
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    if let Err(e) = run_ui(&mut terminal, &results, &summary) {
        eprintln!("UI error: {}", e);
    }

    disable_raw_mode()?;
    Ok(())
}
