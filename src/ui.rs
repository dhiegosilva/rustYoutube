use crate::player::play_video;
use crate::youtube::{Video, YouTubeClient};
use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use std::io;
use std::time::Duration;

pub async fn run(youtube_client: YouTubeClient) -> Result<()> {
    // Clear any pending input and prepare terminal
    use std::io::Write;
    std::io::stdout().flush()?;
    
    // Small delay to ensure terminal is ready
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;
    
    // Clear the screen
    terminal.clear()?;

    let mut videos = Vec::new();
    let mut list_state = ListState::default();
    let mut channel_url = String::new();
    let mut status_message = "Enter a YouTube channel URL (e.g., https://www.youtube.com/@channelname/videos)".to_string();
    let mut input_mode = true; // Start in URL input mode
    let mut should_quit = false;

    // Initial render to show UI immediately
    terminal.draw(|f| {
        if input_mode {
            ui_input(f, &channel_url, &status_message);
        } else {
            ui_videos(f, &videos, &mut list_state, &status_message);
        }
    })?;

    loop {
        terminal.draw(|f| {
            if input_mode {
                ui_input(f, &channel_url, &status_message);
            } else {
                ui_videos(f, &videos, &mut list_state, &status_message);
            }
        })?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if input_mode {
                        match key.code {
                            KeyCode::Char('q') => {
                                should_quit = true;
                            }
                            KeyCode::Enter => {
                                if !channel_url.trim().is_empty() {
                                    input_mode = false;
                                    status_message = "Loading videos...".to_string();
                                    terminal.draw(|f| ui_videos(f, &videos, &mut list_state, &status_message))?;
                                    
                                    match youtube_client.get_channel_videos(&channel_url).await {
                                        Ok(mut new_videos) => {
                                            // Limit to top 20
                                            if new_videos.len() > 20 {
                                                new_videos.truncate(20);
                                            }
                                            videos = new_videos;
                                            list_state.select(Some(0));
                                            status_message = format!("Loaded {} videos from channel", videos.len());
                                        }
                                        Err(e) => {
                                            let error_msg = format!("{}", e);
                                            // If it's a yt-dlp not found error, provide helpful message
                                            if error_msg.contains("not installed") || error_msg.contains("not found") {
                                                status_message = format!("Error: {}\n\nPlease install yt-dlp or restart the program to auto-install.", error_msg);
                                            } else {
                                                status_message = format!("Error: {}", error_msg);
                                            }
                                            input_mode = true; // Go back to input mode on error
                                        }
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                channel_url.pop();
                            }
                            KeyCode::Char(c) => {
                                channel_url.push(c);
                            }
                            _ => {}
                        }
                    } else {
                        // Video list mode
                        match key.code {
                            KeyCode::Char('q') => {
                                should_quit = true;
                            }
                            KeyCode::Char('u') => {
                                // Go back to URL input
                                input_mode = true;
                                channel_url.clear();
                                videos.clear();
                                status_message = "Enter a YouTube channel URL".to_string();
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if let Some(selected) = list_state.selected() {
                                    if selected > 0 {
                                        list_state.select(Some(selected - 1));
                                    }
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if let Some(selected) = list_state.selected() {
                                    if selected < videos.len().saturating_sub(1) {
                                        list_state.select(Some(selected + 1));
                                    }
                                }
                            }
                            KeyCode::Enter | KeyCode::Char(' ') => {
                                if let Some(selected) = list_state.selected() {
                                    if selected < videos.len() {
                                        let video = &videos[selected];
                                        status_message = format!("Playing: {}", video.title);
                                        terminal.draw(|f| ui_videos(f, &videos, &mut list_state, &status_message))?;
                                        
                                        // Play video in background
                                        let video_id = video.id.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = play_video(&video_id).await {
                                                eprintln!("Error playing video: {}", e);
                                            }
                                        });
                                    }
                                }
                            }
                            KeyCode::Char('r') => {
                                status_message = "Refreshing videos...".to_string();
                                terminal.draw(|f| ui_videos(f, &videos, &mut list_state, &status_message))?;
                                
                                match youtube_client.get_channel_videos(&channel_url).await {
                                    Ok(mut new_videos) => {
                                        if new_videos.len() > 20 {
                                            new_videos.truncate(20);
                                        }
                                        videos = new_videos;
                                        if list_state.selected().unwrap_or(0) >= videos.len() {
                                            list_state.select(Some(0));
                                        }
                                        status_message = format!("Loaded {} videos", videos.len());
                                    }
                                    Err(e) => {
                                        status_message = format!("Error: {}", e);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui_input(f: &mut Frame, channel_url: &str, status: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("YouTube Terminal Client - Channel Video Browser")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // URL input
    let input_text = if channel_url.is_empty() {
        "Enter YouTube channel URL here..."
    } else {
        channel_url
    };
    let input = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Channel URL"));
    f.render_widget(input, chunks[1]);

    // Status
    let help_text = "Examples: https://www.youtube.com/@channelname/videos | Press Enter to load | q: Quit";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_videos(f: &mut Frame, videos: &[Video], list_state: &mut ListState, status: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("YouTube Terminal Client - Channel Videos")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Video list
    let items: Vec<ListItem> = videos
        .iter()
        .enumerate()
        .map(|(i, video)| {
            let date = format_date(&video.published_at);
            let content = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", i + 1),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        &video.title,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        &video.channel_title,
                        Style::default().fg(Color::Blue),
                    ),
                    Span::raw(" • "),
                    Span::styled(date, Style::default().fg(Color::Gray)),
                ]),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Videos (Top 20)"))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[1], list_state);

    // Status bar
    let help_text = "↑/↓/j/k: Navigate | Enter/Space: Play | r: Refresh | u: New URL | q: Quit";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[2]);
}

fn format_date(date_str: &str) -> String {
    if date_str.is_empty() {
        return "Unknown date".to_string();
    }
    
    // Try to parse YYYY-MM-DD format
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        let now = chrono::Utc::now().date_naive();
        let days = now.signed_duration_since(dt).num_days();
        if days > 0 {
            if days > 365 {
                let years = days / 365;
                return format!("{} year{} ago", years, if years > 1 { "s" } else { "" });
            } else if days > 30 {
                let months = days / 30;
                return format!("{} month{} ago", months, if months > 1 { "s" } else { "" });
            } else {
                return format!("{} day{} ago", days, if days > 1 { "s" } else { "" });
            }
        }
    }
    
    date_str.to_string()
}
