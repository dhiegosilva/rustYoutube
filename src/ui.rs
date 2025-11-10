use crate::player::{play_video, download_video};
use crate::youtube::{Playlist, Subscription, Video, YouTubeClient};
use crate::i18n::{t, t_with_args};
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
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::process::Child;

#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    MainMenu,
    Subscriptions,
    ChannelMenu,
    SubscriptionVideos,
    SubscriptionShorts,
    SubscriptionPlaylists,
    Playlists,
    PlaylistVideos,
    ChannelInput,
    ChannelVideos,
}

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

    // Channel for yt-dlp output messages
    let (log_tx, mut log_rx) = mpsc::unbounded_channel::<String>();
    let log_tx_arc = Arc::new(log_tx);

    let mut view_mode = ViewMode::MainMenu;
    let mut all_videos: Vec<Video> = Vec::new(); // Store all videos
    let mut all_shorts: Vec<Video> = Vec::new(); // Store all shorts separately
    let mut subscriptions = Vec::new();
    let mut playlists = Vec::new();
    let mut channel_playlists = Vec::new(); // Store channel playlists
    let mut selected_channel_id: Option<String> = None; // Store selected channel ID
    let mut selected_channel_title: Option<String> = None; // Store selected channel title
    let mut video_list_state = ListState::default();
    let mut subscription_list_state = ListState::default();
    let mut playlist_list_state = ListState::default();
    let mut main_menu_selection = 0; // Track main menu selection (0=s, 1=p, 2=c, 3=q)
    let mut channel_tab = 0; // Track channel tab selection (0=Videos, 1=Shorts, 2=Playlists)
    let mut channel_url = String::new();
    let mut status_message = t("status_welcome");
    let mut log_message = String::new(); // Store yt-dlp output messages
    let mut should_quit = false;
    
    // Store handles for cancellation (download and playback)
    let download_handle: Arc<std::sync::Mutex<Option<Child>>> = Arc::new(std::sync::Mutex::new(None));
    let playback_handle: Arc<std::sync::Mutex<Option<Child>>> = Arc::new(std::sync::Mutex::new(None));
    
    // Pagination state
    const VIDEOS_PER_PAGE: usize = 20;
    let mut current_page = 0;
    
    // Helper function to get current page videos
    let get_current_page_videos = |all: &[Video], page: usize| -> Vec<Video> {
        let start = page * VIDEOS_PER_PAGE;
        let end = (start + VIDEOS_PER_PAGE).min(all.len());
        if start < all.len() {
            all[start..end].to_vec()
        } else {
            Vec::new()
        }
    };
    
    // Helper function to calculate total pages
    let calculate_total_pages = |count: usize| -> usize {
        (count + VIDEOS_PER_PAGE - 1) / VIDEOS_PER_PAGE.max(1)
    };
    
    // Helper function to separate videos and shorts
    let separate_videos_and_shorts = |videos: Vec<Video>| -> (Vec<Video>, Vec<Video>) {
        let mut regular_videos: Vec<Video> = Vec::new();
        let mut shorts: Vec<Video> = Vec::new();
        for video in videos {
            // Check if it's a short (title contains #shorts or #short)
            let is_short = video.title.to_lowercase().contains("#shorts") || video.title.to_lowercase().contains("#short");
            if is_short {
                shorts.push(video);
            } else {
                regular_videos.push(video);
            }
        }
        (regular_videos, shorts)
    };

    // Initial render
    terminal.draw(|f| {
        ui_main_menu(f, main_menu_selection, &status_message, &log_message);
    })?;

    loop {
        // Check for log messages (non-blocking) - collect all pending messages
        while let Ok(msg) = log_rx.try_recv() {
            log_message = msg;
        }
        // If multiple messages came in, keep the latest one

        // Always redraw UI to show updated log messages
        terminal.draw(|f| {
            match view_mode {
                ViewMode::MainMenu => {
                    ui_main_menu(f, main_menu_selection, &status_message, &log_message);
                }
                ViewMode::Subscriptions => {
                    ui_subscriptions(f, &subscriptions, &mut subscription_list_state, &status_message, &log_message);
                }
                ViewMode::ChannelMenu => {
                    // Channel menu is now replaced with tabs, but we keep this for backward compatibility
                    // This should not be reached in normal flow, but if it is, show videos
                    let page_videos = get_current_page_videos(&all_videos, current_page);
                    let total_pages = calculate_total_pages(all_videos.len());
                    ui_channel_with_tabs(f, &page_videos, &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, current_page + 1, total_pages, &log_message);
                }
                ViewMode::SubscriptionVideos | ViewMode::SubscriptionShorts => {
                    let current_list = if view_mode == ViewMode::SubscriptionShorts { &all_shorts } else { &all_videos };
                    let page_videos = get_current_page_videos(current_list, current_page);
                    let total_pages = calculate_total_pages(current_list.len());
                    ui_channel_with_tabs(f, &page_videos, &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, current_page + 1, total_pages, &log_message);
                }
                ViewMode::SubscriptionPlaylists => {
                    // Show playlists tab view - render playlists list
                    ui_channel_with_tabs_playlists(f, &channel_playlists, &mut playlist_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, &log_message);
                }
                ViewMode::Playlists => {
                    ui_playlists(f, &playlists, &mut playlist_list_state, &status_message, &log_message);
                }
                ViewMode::PlaylistVideos => {
                    let page_videos = get_current_page_videos(&all_videos, current_page);
                    let total_pages = calculate_total_pages(all_videos.len());
                    ui_videos(f, &page_videos, &mut video_list_state, &status_message, current_page + 1, total_pages, &log_message);
                }
                ViewMode::ChannelInput => {
                    ui_input(f, &channel_url, &status_message, &log_message);
                }
                ViewMode::ChannelVideos => {
                    let page_videos = get_current_page_videos(&all_videos, current_page);
                    let total_pages = calculate_total_pages(all_videos.len());
                    ui_videos(f, &page_videos, &mut video_list_state, &status_message, current_page + 1, total_pages, &log_message);
                }
            }
        })?;

        // Use shorter poll timeout to update UI more frequently
        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match view_mode {
                        ViewMode::MainMenu => {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Char('Q') => {
                                    should_quit = true;
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if main_menu_selection > 0 {
                                        main_menu_selection -= 1;
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if main_menu_selection < 3 {
                                        main_menu_selection += 1;
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    match main_menu_selection {
                                        0 => {
                                            // Subscriptions
                                            if youtube_client.is_authenticated() {
                                                view_mode = ViewMode::Subscriptions;
                                                status_message = t("status_loading_subscriptions");
                                                terminal.draw(|f| ui_subscriptions(f, &subscriptions, &mut subscription_list_state, &status_message, &log_message))?;
                                                
                                                match youtube_client.get_subscriptions().await {
                                                    Ok(new_subs) => {
                                                        subscriptions = new_subs;
                                                        if subscriptions.is_empty() {
                                                            status_message = t("status_no_subscriptions");
                                                        } else {
                                                            subscription_list_state.select(Some(0));
                                                            status_message = t_with_args("status_loaded_subscriptions", &[("count", &subscriptions.len().to_string())]);
                                                        }
                                                    }
                                                    Err(e) => {
                                                        status_message = format!("Error: {}", e);
                                                        view_mode = ViewMode::MainMenu;
                                                    }
                                                }
                                            } else {
                                                status_message = "Not authenticated. Please check your credentials.".to_string();
                                            }
                                        }
                                        1 => {
                                            // Playlists
                                            if youtube_client.is_authenticated() {
                                                view_mode = ViewMode::Playlists;
                                                status_message = t("status_loading_playlists");
                                                terminal.draw(|f| ui_playlists(f, &playlists, &mut playlist_list_state, &status_message, &log_message))?;
                                                
                                                match youtube_client.get_playlists().await {
                                                    Ok(new_playlists) => {
                                                        playlists = new_playlists;
                                                        playlist_list_state.select(Some(0));
                                                        status_message = format!("Loaded {} playlists", playlists.len());
                                                    }
                                                    Err(e) => {
                                                        status_message = format!("Error: {}", e);
                                                        view_mode = ViewMode::MainMenu;
                                                    }
                                                }
                                            } else {
                                                status_message = "Not authenticated. Please check your credentials.".to_string();
                                            }
                                        }
                                        2 => {
                                            // Channel Input
                                            view_mode = ViewMode::ChannelInput;
                                            channel_url.clear();
                                            status_message = t("channel_input_title");
                                        }
                                        3 => {
                                            // Quit
                                            should_quit = true;
                                        }
                                        _ => {}
                                    }
                                }
                                KeyCode::Char('s') | KeyCode::Char('S') => {
                                    if youtube_client.is_authenticated() {
                                        view_mode = ViewMode::Subscriptions;
                                        status_message = t("status_loading_subscriptions");
                                        terminal.draw(|f| ui_subscriptions(f, &subscriptions, &mut subscription_list_state, &status_message, &log_message))?;
                                        
                                        match youtube_client.get_subscriptions().await {
                                            Ok(new_subs) => {
                                                subscriptions = new_subs;
                                                if subscriptions.is_empty() {
                                                    status_message = t("status_no_subscriptions");
                                                } else {
                                                    subscription_list_state.select(Some(0));
                                                    status_message = t_with_args("status_loaded_subscriptions", &[("count", &subscriptions.len().to_string())]);
                                                }
                                            }
                                            Err(e) => {
                                                status_message = format!("Error: {}", e);
                                                view_mode = ViewMode::MainMenu;
                                            }
                                        }
                                    } else {
                                        status_message = "Not authenticated. Please check your credentials.".to_string();
                                    }
                                }
                                KeyCode::Char('p') | KeyCode::Char('P') => {
                                    if youtube_client.is_authenticated() {
                                        view_mode = ViewMode::Playlists;
                                        status_message = t("status_loading_playlists");
                                        terminal.draw(|f| ui_playlists(f, &playlists, &mut playlist_list_state, &status_message, &log_message))?;
                                        
                                        match youtube_client.get_playlists().await {
                                            Ok(new_playlists) => {
                                                playlists = new_playlists;
                                                playlist_list_state.select(Some(0));
                                                status_message = format!("Loaded {} playlists", playlists.len());
                                            }
                                            Err(e) => {
                                                status_message = format!("Error: {}", e);
                                                view_mode = ViewMode::MainMenu;
                                            }
                                        }
                                    } else {
                                        status_message = "Not authenticated. Please check your credentials.".to_string();
                                    }
                                }
                                KeyCode::Char('c') | KeyCode::Char('C') => {
                                    view_mode = ViewMode::ChannelInput;
                                    channel_url.clear();
                                    status_message = t("channel_input_title");
                                }
                                _ => {}
                            }
                        }
                        ViewMode::Subscriptions => {
                            match key.code {
                                KeyCode::Char('q') => {
                                    should_quit = true;
                                }
                                KeyCode::Char('m') | KeyCode::Esc => {
                                    view_mode = ViewMode::MainMenu;
                                    status_message = "Main menu".to_string();
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if let Some(selected) = subscription_list_state.selected() {
                                        if selected > 0 {
                                            subscription_list_state.select(Some(selected - 1));
                                        }
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if let Some(selected) = subscription_list_state.selected() {
                                        if selected < subscriptions.len().saturating_sub(1) {
                                            subscription_list_state.select(Some(selected + 1));
                                        }
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    if let Some(selected) = subscription_list_state.selected() {
                                        if selected < subscriptions.len() {
                                            let sub = &subscriptions[selected];
                                            selected_channel_id = Some(sub.channel_id.clone());
                                            selected_channel_title = Some(sub.channel_title.clone());
                                            // Default to Videos tab
                                            channel_tab = 0;
                                            view_mode = ViewMode::SubscriptionVideos;
                                            current_page = 0;
                                            status_message = format!("Loading videos from {}...", sub.channel_title);
                                            let empty: Vec<Video> = Vec::new();
                                            terminal.draw(|f| ui_channel_with_tabs(f, &empty, &mut video_list_state, &sub.channel_title, channel_tab, &status_message, 1, 1, &log_message))?;
                                            
                                            match youtube_client.get_channel_videos_by_id(&sub.channel_id).await {
                                                Ok(new_videos) => {
                                                    // Separate videos and shorts
                                                    let (videos, shorts) = separate_videos_and_shorts(new_videos);
                                                    all_videos = videos;
                                                    all_shorts = shorts;
                                                    video_list_state.select(Some(0));
                                                    let total_pages = calculate_total_pages(all_videos.len());
                                                    status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), sub.channel_title, current_page + 1, total_pages.max(1));
                                                }
                                                Err(e) => {
                                                    status_message = format!("Error: {}", e);
                                                    view_mode = ViewMode::Subscriptions;
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('r') => {
                                    status_message = t("status_refreshing");
                                    terminal.draw(|f| ui_subscriptions(f, &subscriptions, &mut subscription_list_state, &status_message, &log_message))?;
                                    
                                    match youtube_client.get_subscriptions().await {
                                        Ok(new_subs) => {
                                            subscriptions = new_subs;
                                            if subscription_list_state.selected().unwrap_or(0) >= subscriptions.len() {
                                                subscription_list_state.select(Some(0));
                                            }
                                            status_message = format!("Loaded {} subscriptions", subscriptions.len());
                                        }
                                        Err(e) => {
                                            status_message = format!("Error: {}", e);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        ViewMode::ChannelMenu => {
                            // This mode is now deprecated, but kept for compatibility
                            // Should redirect to SubscriptionVideos with tabs
                            view_mode = ViewMode::SubscriptionVideos;
                            channel_tab = 0;
                        }
                        ViewMode::Playlists => {
                            match key.code {
                                KeyCode::Char('q') => {
                                    should_quit = true;
                                }
                                KeyCode::Char('m') | KeyCode::Esc => {
                                    view_mode = ViewMode::MainMenu;
                                    status_message = "Main menu".to_string();
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if let Some(selected) = playlist_list_state.selected() {
                                        if selected > 0 {
                                            playlist_list_state.select(Some(selected - 1));
                                        }
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if let Some(selected) = playlist_list_state.selected() {
                                        if selected < playlists.len().saturating_sub(1) {
                                            playlist_list_state.select(Some(selected + 1));
                                        }
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    if let Some(selected) = playlist_list_state.selected() {
                                        if selected < playlists.len() {
                                            let playlist = &playlists[selected];
                                            view_mode = ViewMode::PlaylistVideos;
                                            current_page = 0;
                                            status_message = format!("Loading videos from {}...", playlist.title);
                                            let empty: Vec<Video> = Vec::new();
                                            terminal.draw(|f| ui_videos(f, &empty, &mut video_list_state, &status_message, 1, 1, &log_message))?;
                                            
                                            match youtube_client.get_playlist_videos(&playlist.id).await {
                                                Ok(new_videos) => {
                                                    all_videos = new_videos;
                                                    video_list_state.select(Some(0));
                                                    let total_pages = calculate_total_pages(all_videos.len());
                                                    status_message = t_with_args("status_loaded_videos_from", &[
                                                        ("count", &all_videos.len().to_string()),
                                                        ("channel", &playlist.title),
                                                        ("page", &(current_page + 1).to_string()),
                                                        ("total", &total_pages.max(1).to_string())
                                                    ]);
                                                }
                                                Err(e) => {
                                                    status_message = format!("Error: {}", e);
                                                    view_mode = ViewMode::Playlists;
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('r') => {
                                    status_message = t("status_refreshing");
                                    terminal.draw(|f| ui_playlists(f, &playlists, &mut playlist_list_state, &status_message, &log_message))?;
                                    
                                    match youtube_client.get_playlists().await {
                                        Ok(new_playlists) => {
                                            playlists = new_playlists;
                                            if playlist_list_state.selected().unwrap_or(0) >= playlists.len() {
                                                playlist_list_state.select(Some(0));
                                            }
                                            status_message = format!("Loaded {} playlists", playlists.len());
                                        }
                                        Err(e) => {
                                            status_message = format!("Error: {}", e);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        ViewMode::SubscriptionPlaylists => {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Char('Q') => {
                                    should_quit = true;
                                }
                                KeyCode::Esc => {
                                    view_mode = ViewMode::Subscriptions;
                                    selected_channel_id = None;
                                    selected_channel_title = None;
                                    channel_tab = 0;
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if let Some(selected) = playlist_list_state.selected() {
                                        if selected > 0 {
                                            playlist_list_state.select(Some(selected - 1));
                                        }
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if let Some(selected) = playlist_list_state.selected() {
                                        if selected < channel_playlists.len().saturating_sub(1) {
                                            playlist_list_state.select(Some(selected + 1));
                                        }
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    if let Some(selected) = playlist_list_state.selected() {
                                        if selected < channel_playlists.len() {
                                            let playlist = &channel_playlists[selected];
                                            view_mode = ViewMode::PlaylistVideos;
                                            current_page = 0;
                                            status_message = format!("Loading videos from {}...", playlist.title);
                                            let empty: Vec<Video> = Vec::new();
                                            terminal.draw(|f| ui_videos(f, &empty, &mut video_list_state, &status_message, 1, 1, &log_message))?;
                                            
                                            match youtube_client.get_playlist_videos(&playlist.id).await {
                                                Ok(new_videos) => {
                                                    all_videos = new_videos;
                                                    video_list_state.select(Some(0));
                                                    let total_pages = calculate_total_pages(all_videos.len());
                                                    status_message = t_with_args("status_loaded_videos_from", &[
                                                        ("count", &all_videos.len().to_string()),
                                                        ("channel", &playlist.title),
                                                        ("page", &(current_page + 1).to_string()),
                                                        ("total", &total_pages.max(1).to_string())
                                                    ]);
                                                }
                                                Err(e) => {
                                                    status_message = format!("Error: {}", e);
                                                    view_mode = ViewMode::SubscriptionPlaylists;
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Left | KeyCode::Char('h') => {
                                    // Switch to previous tab
                                    if channel_tab > 0 {
                                        channel_tab -= 1;
                                        current_page = 0;
                                        video_list_state.select(Some(0));
                                        if channel_tab == 0 {
                                            view_mode = ViewMode::SubscriptionVideos;
                                            if !all_videos.is_empty() {
                                                let total_pages = calculate_total_pages(all_videos.len());
                                                status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                            }
                                        } else if channel_tab == 1 {
                                            view_mode = ViewMode::SubscriptionShorts;
                                            if !all_shorts.is_empty() {
                                                let total_pages = calculate_total_pages(all_shorts.len());
                                                status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                            }
                                        }
                                    }
                                }
                                KeyCode::Right | KeyCode::Char('l') => {
                                    // Already on last tab
                                }
                                KeyCode::Char('1') => {
                                    channel_tab = 0;
                                    current_page = 0;
                                    video_list_state.select(Some(0));
                                    view_mode = ViewMode::SubscriptionVideos;
                                    if !all_videos.is_empty() {
                                        let total_pages = calculate_total_pages(all_videos.len());
                                        status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                    }
                                }
                                KeyCode::Char('2') => {
                                    channel_tab = 1;
                                    current_page = 0;
                                    video_list_state.select(Some(0));
                                    view_mode = ViewMode::SubscriptionShorts;
                                    if !all_shorts.is_empty() {
                                        let total_pages = calculate_total_pages(all_shorts.len());
                                        status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                    }
                                }
                                KeyCode::Char('3') => {
                                    // Already on playlists tab
                                }
                                KeyCode::Char('r') | KeyCode::Char('R') => {
                                    status_message = t("status_refreshing");
                                    terminal.draw(|f| ui_channel_with_tabs_playlists(f, &channel_playlists, &mut playlist_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, &log_message))?;
                                    
                                    if let Some(channel_id) = &selected_channel_id {
                                        match youtube_client.get_channel_playlists(channel_id).await {
                                            Ok(new_playlists) => {
                                                channel_playlists = new_playlists;
                                                if playlist_list_state.selected().unwrap_or(0) >= channel_playlists.len() {
                                                    playlist_list_state.select(Some(0));
                                                }
                                                status_message = format!("Loaded {} playlists from {}", channel_playlists.len(), selected_channel_title.as_deref().unwrap_or("channel"));
                                            }
                                            Err(e) => {
                                                status_message = format!("Error: {}", e);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        ViewMode::SubscriptionVideos | ViewMode::SubscriptionShorts | ViewMode::PlaylistVideos | ViewMode::ChannelVideos => {
                            // Determine which list to use based on view mode
                            let current_list = if view_mode == ViewMode::SubscriptionShorts { &all_shorts } else { &all_videos };
                            
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Char('Q') => {
                                    should_quit = true;
                                }
                                KeyCode::Esc => {
                                    // Go back to previous view
                                    if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists {
                                        view_mode = ViewMode::Subscriptions;
                                        selected_channel_id = None;
                                        selected_channel_title = None;
                                        channel_tab = 0;
                                    } else if view_mode == ViewMode::PlaylistVideos {
                                        // Check if we came from channel playlists or regular playlists
                                        if selected_channel_id.is_some() {
                                            // Go back to channel tabs view
                                            view_mode = ViewMode::SubscriptionPlaylists;
                                            channel_tab = 2; // Playlists tab
                                        } else {
                                            view_mode = ViewMode::Playlists;
                                        }
                                    } else {
                                        view_mode = ViewMode::ChannelInput;
                                    }
                                    all_videos.clear();
                                    all_shorts.clear();
                                    current_page = 0;
                                }
                                // Tab navigation for channel views
                                KeyCode::Left | KeyCode::Char('h') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists => {
                                    if channel_tab > 0 {
                                        channel_tab -= 1;
                                        current_page = 0;
                                        video_list_state.select(Some(0));
                                        
                                        // Switch to the appropriate view mode and load data
                                        if let Some(channel_id) = &selected_channel_id {
                                            match channel_tab {
                                                0 => {
                                                    // Videos tab
                                                    view_mode = ViewMode::SubscriptionVideos;
                                                    status_message = format!("Loading videos from {}...", selected_channel_title.as_deref().unwrap_or("channel"));
                                                    let empty: Vec<Video> = Vec::new();
                                                    terminal.draw(|f| ui_channel_with_tabs(f, &empty, &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, 1, 1, &log_message))?;
                                                    
                                                    // If we already have videos loaded, just switch view
                                                    if !all_videos.is_empty() {
                                                        let total_pages = calculate_total_pages(all_videos.len());
                                                        status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                    } else {
                                                        match youtube_client.get_channel_videos_by_id(channel_id).await {
                                                            Ok(new_videos) => {
                                                                let (videos, shorts) = separate_videos_and_shorts(new_videos);
                                                                all_videos = videos;
                                                                all_shorts = shorts;
                                                                let total_pages = calculate_total_pages(all_videos.len());
                                                                status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                            }
                                                            Err(e) => {
                                                                status_message = format!("Error: {}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                                1 => {
                                                    // Shorts tab
                                                    view_mode = ViewMode::SubscriptionShorts;
                                                    status_message = format!("Loading shorts from {}...", selected_channel_title.as_deref().unwrap_or("channel"));
                                                    let empty: Vec<Video> = Vec::new();
                                                    terminal.draw(|f| ui_channel_with_tabs(f, &empty, &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, 1, 1, &log_message))?;
                                                    
                                                    if !all_shorts.is_empty() {
                                                        let total_pages = calculate_total_pages(all_shorts.len());
                                                        status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                    } else if !all_videos.is_empty() {
                                                        // Videos were loaded, but shorts might not be separated yet
                                                        let total_pages = calculate_total_pages(all_shorts.len());
                                                        status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                    } else {
                                                        match youtube_client.get_channel_videos_by_id(channel_id).await {
                                                            Ok(new_videos) => {
                                                                let (videos, shorts) = separate_videos_and_shorts(new_videos);
                                                                all_videos = videos;
                                                                all_shorts = shorts;
                                                                let total_pages = calculate_total_pages(all_shorts.len());
                                                                status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                            }
                                                            Err(e) => {
                                                                status_message = format!("Error: {}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                                2 => {
                                                    // Playlists tab
                                                    view_mode = ViewMode::SubscriptionPlaylists;
                                                    status_message = format!("Loading playlists from {}...", selected_channel_title.as_deref().unwrap_or("channel"));
                                                    terminal.draw(|f| ui_channel_with_tabs(f, &[], &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, 1, 1, &log_message))?;
                                                    
                                                    match youtube_client.get_channel_playlists(channel_id).await {
                                                        Ok(new_playlists) => {
                                                            channel_playlists = new_playlists;
                                                            if channel_playlists.is_empty() {
                                                                status_message = format!("No playlists found for {}", selected_channel_title.as_deref().unwrap_or("channel"));
                                                            } else {
                                                                playlist_list_state.select(Some(0));
                                                                status_message = format!("Loaded {} playlists from {}", channel_playlists.len(), selected_channel_title.as_deref().unwrap_or("channel"));
                                                            }
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
                                KeyCode::Right | KeyCode::Char('l') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists => {
                                    if channel_tab < 2 {
                                        channel_tab += 1;
                                        current_page = 0;
                                        video_list_state.select(Some(0));
                                        
                                        // Switch to the appropriate view mode and load data
                                        if let Some(channel_id) = &selected_channel_id {
                                            match channel_tab {
                                                0 => {
                                                    view_mode = ViewMode::SubscriptionVideos;
                                                    if !all_videos.is_empty() {
                                                        let total_pages = calculate_total_pages(all_videos.len());
                                                        status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                    }
                                                }
                                                1 => {
                                                    view_mode = ViewMode::SubscriptionShorts;
                                                    status_message = format!("Loading shorts from {}...", selected_channel_title.as_deref().unwrap_or("channel"));
                                                    let empty: Vec<Video> = Vec::new();
                                                    terminal.draw(|f| ui_channel_with_tabs(f, &empty, &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, 1, 1, &log_message))?;
                                                    
                                                    if !all_shorts.is_empty() {
                                                        let total_pages = calculate_total_pages(all_shorts.len());
                                                        status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                    } else if !all_videos.is_empty() {
                                                        // Videos were loaded, but shorts might not be separated yet
                                                        let total_pages = calculate_total_pages(all_shorts.len());
                                                        status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                    } else {
                                                        match youtube_client.get_channel_videos_by_id(channel_id).await {
                                                            Ok(new_videos) => {
                                                                let (videos, shorts) = separate_videos_and_shorts(new_videos);
                                                                all_videos = videos;
                                                                all_shorts = shorts;
                                                                let total_pages = calculate_total_pages(all_shorts.len());
                                                                status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                                            }
                                                            Err(e) => {
                                                                status_message = format!("Error: {}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                                2 => {
                                                    view_mode = ViewMode::SubscriptionPlaylists;
                                                    status_message = format!("Loading playlists from {}...", selected_channel_title.as_deref().unwrap_or("channel"));
                                                    terminal.draw(|f| ui_channel_with_tabs(f, &[], &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, 1, 1, &log_message))?;
                                                    
                                                    match youtube_client.get_channel_playlists(channel_id).await {
                                                        Ok(new_playlists) => {
                                                            channel_playlists = new_playlists;
                                                            if channel_playlists.is_empty() {
                                                                status_message = format!("No playlists found for {}", selected_channel_title.as_deref().unwrap_or("channel"));
                                                            } else {
                                                                playlist_list_state.select(Some(0));
                                                                status_message = format!("Loaded {} playlists from {}", channel_playlists.len(), selected_channel_title.as_deref().unwrap_or("channel"));
                                                            }
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
                                KeyCode::Char('1') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists => {
                                    // Switch to Videos tab
                                    if channel_tab != 0 {
                                        channel_tab = 0;
                                        current_page = 0;
                                        video_list_state.select(Some(0));
                                        view_mode = ViewMode::SubscriptionVideos;
                                        if !all_videos.is_empty() {
                                            let total_pages = calculate_total_pages(all_videos.len());
                                            status_message = format!("Loaded {} videos from {} (Page {}/{})", all_videos.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                        }
                                    }
                                }
                                KeyCode::Char('2') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists => {
                                    // Switch to Shorts tab
                                    if channel_tab != 1 {
                                        channel_tab = 1;
                                        current_page = 0;
                                        video_list_state.select(Some(0));
                                        view_mode = ViewMode::SubscriptionShorts;
                                        if !all_shorts.is_empty() {
                                            let total_pages = calculate_total_pages(all_shorts.len());
                                            status_message = format!("Loaded {} shorts from {} (Page {}/{})", all_shorts.len(), selected_channel_title.as_deref().unwrap_or("channel"), current_page + 1, total_pages.max(1));
                                        }
                                    }
                                }
                                KeyCode::Char('3') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists => {
                                    // Switch to Playlists tab
                                    if channel_tab != 2 {
                                        channel_tab = 2;
                                        current_page = 0;
                                        video_list_state.select(Some(0));
                                        view_mode = ViewMode::SubscriptionPlaylists;
                                        if let Some(channel_id) = &selected_channel_id {
                                            status_message = format!("Loading playlists from {}...", selected_channel_title.as_deref().unwrap_or("channel"));
                                            terminal.draw(|f| ui_channel_with_tabs(f, &[], &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, 1, 1, &log_message))?;
                                            
                                            match youtube_client.get_channel_playlists(channel_id).await {
                                                Ok(new_playlists) => {
                                                    channel_playlists = new_playlists;
                                                    if channel_playlists.is_empty() {
                                                        status_message = format!("No playlists found for {}", selected_channel_title.as_deref().unwrap_or("channel"));
                                                    } else {
                                                        playlist_list_state.select(Some(0));
                                                        status_message = format!("Loaded {} playlists from {}", channel_playlists.len(), selected_channel_title.as_deref().unwrap_or("channel"));
                                                    }
                                                }
                                                Err(e) => {
                                                    status_message = format!("Error: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if let Some(selected) = video_list_state.selected() {
                                        if selected > 0 {
                                            video_list_state.select(Some(selected - 1));
                                        }
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    let page_videos = get_current_page_videos(current_list, current_page);
                                    if let Some(selected) = video_list_state.selected() {
                                        if selected < page_videos.len().saturating_sub(1) {
                                            video_list_state.select(Some(selected + 1));
                                        }
                                    }
                                }
                                KeyCode::Char('n') | KeyCode::Right if view_mode != ViewMode::SubscriptionVideos && view_mode != ViewMode::SubscriptionShorts && view_mode != ViewMode::SubscriptionPlaylists => {
                                    // Next page (only if not in channel tabs view)
                                    let total_pages = calculate_total_pages(current_list.len());
                                    if current_page < total_pages.saturating_sub(1) {
                                        current_page += 1;
                                        video_list_state.select(Some(0));
                                        status_message = format!("Page {}/{}", current_page + 1, total_pages.max(1));
                                    }
                                }
                                KeyCode::Left if view_mode != ViewMode::SubscriptionVideos && view_mode != ViewMode::SubscriptionShorts && view_mode != ViewMode::SubscriptionPlaylists => {
                                    // Previous page (only if not in channel tabs view)
                                    if current_page > 0 {
                                        current_page -= 1;
                                        video_list_state.select(Some(0));
                                        let total_pages = calculate_total_pages(current_list.len());
                                        status_message = format!("Page {}/{}", current_page + 1, total_pages.max(1));
                                    }
                                }
                                KeyCode::Char('b') if view_mode != ViewMode::SubscriptionVideos && view_mode != ViewMode::SubscriptionShorts && view_mode != ViewMode::SubscriptionPlaylists && view_mode != ViewMode::PlaylistVideos => {
                                    // Previous page (only if not going back to menu)
                                    if current_page > 0 {
                                        current_page -= 1;
                                        video_list_state.select(Some(0));
                                        let total_pages = calculate_total_pages(current_list.len());
                                        status_message = format!("Page {}/{}", current_page + 1, total_pages.max(1));
                                    }
                                }
                                // Page navigation for channel tabs (when not switching tabs)
                                KeyCode::Char('n') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts => {
                                    // Next page
                                    let total_pages = calculate_total_pages(current_list.len());
                                    if current_page < total_pages.saturating_sub(1) {
                                        current_page += 1;
                                        video_list_state.select(Some(0));
                                        status_message = format!("Page {}/{}", current_page + 1, total_pages.max(1));
                                    }
                                }
                                KeyCode::Char('b') if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts => {
                                    // Previous page
                                    if current_page > 0 {
                                        current_page -= 1;
                                        video_list_state.select(Some(0));
                                        let total_pages = calculate_total_pages(current_list.len());
                                        status_message = format!("Page {}/{}", current_page + 1, total_pages.max(1));
                                    }
                                }
                                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('p') => {
                                    let page_videos = get_current_page_videos(current_list, current_page);
                                    if let Some(selected) = video_list_state.selected() {
                                        if selected < page_videos.len() {
                                            // Calculate actual index in current_list
                                            let actual_index = current_page * VIDEOS_PER_PAGE + selected;
                                            if actual_index < current_list.len() {
                                                let video = &current_list[actual_index];
                                                status_message = t_with_args("status_playing", &[("title", &video.title)]);
                                                let page_videos = get_current_page_videos(current_list, current_page);
                                                let total_pages = calculate_total_pages(current_list.len());
                                                terminal.draw(|f| ui_videos(f, &page_videos, &mut video_list_state, &status_message, current_page + 1, total_pages, &log_message))?;
                                                
                                                // Play video in background
                                                let video_id = video.id.clone();
                                                let log_tx = log_tx_arc.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = play_video(&video_id, Some((*log_tx).clone())).await {
                                                        let _ = (*log_tx).send(format!("Error: {}", e));
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('d') => {
                                    let page_videos = get_current_page_videos(current_list, current_page);
                                    if let Some(selected) = video_list_state.selected() {
                                        if selected < page_videos.len() {
                                            // Calculate actual index in current_list
                                            let actual_index = current_page * VIDEOS_PER_PAGE + selected;
                                            if actual_index < current_list.len() {
                                                let video = &current_list[actual_index];
                                                status_message = t_with_args("status_downloading", &[("title", &video.title)]);
                                                let page_videos = get_current_page_videos(current_list, current_page);
                                                let total_pages = calculate_total_pages(current_list.len());
                                                terminal.draw(|f| ui_videos(f, &page_videos, &mut video_list_state, &status_message, current_page + 1, total_pages, &log_message))?;
                                                
                                                // Download video in background
                                                let video_id = video.id.clone();
                                                let log_tx = log_tx_arc.clone();
                                                let download_handle_clone = download_handle.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = download_video(&video_id, Some((*log_tx).clone()), Some(download_handle_clone)).await {
                                                        let _ = (*log_tx).send(format!("Error: {}", e));
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }
                                KeyCode::Char('c') => {
                                    // Cancel ongoing download
                                    let mut handle_guard = download_handle.lock().unwrap();
                                    if let Some(mut child) = handle_guard.take() {
                                        // Kill the process asynchronously
                                        let log_tx_cancel = log_tx_arc.clone();
                                        tokio::spawn(async move {
                                            if let Err(e) = child.kill().await {
                                                // Process might have already finished, ignore error
                                                let _ = e;
                                            }
                                        });
                                        status_message = "Download cancelled".to_string();
                                        let _ = log_tx_arc.send("Download cancelled by user".to_string());
                                    } else {
                                        status_message = "No active download to cancel".to_string();
                                    }
                                }
                                KeyCode::Char('r') | KeyCode::Char('R') => {
                                    status_message = t("status_refreshing");
                                    let page_videos = get_current_page_videos(current_list, current_page);
                                    let total_pages = calculate_total_pages(current_list.len());
                                    if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts || view_mode == ViewMode::SubscriptionPlaylists {
                                        terminal.draw(|f| ui_channel_with_tabs(f, &page_videos, &mut video_list_state, selected_channel_title.as_deref().unwrap_or("Channel"), channel_tab, &status_message, current_page + 1, total_pages, &log_message))?;
                                    } else {
                                        terminal.draw(|f| ui_videos(f, &page_videos, &mut video_list_state, &status_message, current_page + 1, total_pages, &log_message))?;
                                    }
                                    
                                    // Refresh based on current view
                                    let result = match view_mode {
                                        ViewMode::SubscriptionVideos | ViewMode::SubscriptionShorts => {
                                            if let Some(channel_id) = &selected_channel_id {
                                                youtube_client.get_channel_videos_by_id(channel_id).await
                                            } else {
                                                continue;
                                            }
                                        }
                                        ViewMode::SubscriptionPlaylists => {
                                            if let Some(channel_id) = &selected_channel_id {
                                                // Refresh playlists
                                                match youtube_client.get_channel_playlists(channel_id).await {
                                                    Ok(new_playlists) => {
                                                        channel_playlists = new_playlists;
                                                        if channel_playlists.is_empty() {
                                                            status_message = format!("No playlists found for {}", selected_channel_title.as_deref().unwrap_or("channel"));
                                                        } else {
                                                            playlist_list_state.select(Some(0));
                                                            status_message = format!("Loaded {} playlists from {}", channel_playlists.len(), selected_channel_title.as_deref().unwrap_or("channel"));
                                                        }
                                                    }
                                                    Err(e) => {
                                                        status_message = format!("Error: {}", e);
                                                    }
                                                }
                                                continue;
                                            } else {
                                                continue;
                                            }
                                        }
                                        ViewMode::PlaylistVideos => {
                                            if let Some(selected) = playlist_list_state.selected() {
                                                // Check if we came from channel playlists or regular playlists
                                                if selected_channel_id.is_some() && selected < channel_playlists.len() {
                                                    let playlist = &channel_playlists[selected];
                                                    youtube_client.get_playlist_videos(&playlist.id).await
                                                } else if selected < playlists.len() {
                                                    let playlist = &playlists[selected];
                                                    youtube_client.get_playlist_videos(&playlist.id).await
                                                } else {
                                                    continue;
                                                }
                                            } else {
                                                continue;
                                            }
                                        }
                                        ViewMode::ChannelVideos => {
                                            youtube_client.get_channel_videos(&channel_url).await
                                        }
                                        _ => continue,
                                    };
                                    
                                    match result {
                                        Ok(new_videos) => {
                                            // Separate videos and shorts if refreshing subscription videos/shorts
                                            if view_mode == ViewMode::SubscriptionVideos || view_mode == ViewMode::SubscriptionShorts {
                                                let (videos, shorts) = separate_videos_and_shorts(new_videos);
                                                all_videos = videos;
                                                all_shorts = shorts;
                                                let refreshed_list = if view_mode == ViewMode::SubscriptionShorts { &all_shorts } else { &all_videos };
                                                current_page = 0;
                                                video_list_state.select(Some(0));
                                                let total_pages = calculate_total_pages(refreshed_list.len());
                                                status_message = format!("Loaded {} {} from {} (Page {}/{})", 
                                                    refreshed_list.len(),
                                                    if view_mode == ViewMode::SubscriptionShorts { "shorts" } else { "videos" },
                                                    selected_channel_title.as_deref().unwrap_or("channel"),
                                                    current_page + 1, 
                                                    total_pages.max(1));
                                            } else {
                                                all_videos = new_videos;
                                                current_page = 0;
                                                video_list_state.select(Some(0));
                                                let total_pages = calculate_total_pages(all_videos.len());
                                                status_message = t_with_args("status_loaded_videos_channel", &[
                                                    ("count", &all_videos.len().to_string()),
                                                    ("page", &(current_page + 1).to_string()),
                                                    ("total", &total_pages.max(1).to_string())
                                                ]);
                                            }
                                        }
                                        Err(e) => {
                                            status_message = format!("Error: {}", e);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        ViewMode::ChannelInput => {
                            match key.code {
                                KeyCode::Char('q') => {
                                    should_quit = true;
                                }
                                KeyCode::Char('m') | KeyCode::Esc => {
                                    view_mode = ViewMode::MainMenu;
                                    channel_url.clear();
                                    status_message = "Main menu".to_string();
                                }
                                KeyCode::Enter => {
                                    if !channel_url.trim().is_empty() {
                                        view_mode = ViewMode::ChannelVideos;
                                        current_page = 0;
                                        status_message = t("status_loading_videos");
                                        let empty: Vec<Video> = Vec::new();
                                        terminal.draw(|f| ui_videos(f, &empty, &mut video_list_state, &status_message, 1, 1, &log_message))?;
                                        
                                        match youtube_client.get_channel_videos(&channel_url).await {
                                            Ok(new_videos) => {
                                                all_videos = new_videos;
                                                video_list_state.select(Some(0));
                                                let total_pages = calculate_total_pages(all_videos.len());
                                                status_message = t_with_args("status_loaded_videos_channel", &[
                                                    ("count", &all_videos.len().to_string()),
                                                    ("page", &(current_page + 1).to_string()),
                                                    ("total", &total_pages.max(1).to_string())
                                                ]);
                                            }
                                            Err(e) => {
                                                let error_msg = format!("{}", e);
                                                if error_msg.contains("not installed") || error_msg.contains("not found") {
                                                    status_message = format!("Error: {}\n\nPlease install yt-dlp or restart the program to auto-install.", error_msg);
                                                } else {
                                                    status_message = format!("Error: {}", error_msg);
                                                }
                                                view_mode = ViewMode::ChannelInput;
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

fn ui_channel_menu(f: &mut Frame, channel_name: &str, status: &str, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new(format!("{} - Channel Menu", channel_name))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Menu options
    let menu_items = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("v", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" or "),
            Span::styled("1", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" - View Videos"),
        ]),
        Line::from(vec![
            Span::styled("s", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" or "),
            Span::styled("2", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" - View Shorts"),
        ]),
        Line::from(vec![
            Span::styled("p", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" or "),
            Span::styled("3", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" - View Playlists"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw(" - Back to Subscriptions"),
        ]),
    ];

    let menu = Paragraph::new(menu_items)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title("Select Option"))
        .alignment(Alignment::Left);
    f.render_widget(menu, chunks[1]);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status
    let help_text = "v/1: Videos | s/2: Shorts | p/3: Playlists | Esc: Back";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_main_menu(f: &mut Frame, selection: usize, status: &str, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new(t("app_title"))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Menu options with selection highlighting
    let menu_items: Vec<Line> = (0..4)
        .map(|i| {
            let (text, style) = match i {
                0 => (t("menu_subscriptions"), if selection == i { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) }),
                1 => (t("menu_playlists"), if selection == i { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) }),
                2 => (t("menu_channel"), if selection == i { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) }),
                3 => (t("menu_quit"), if selection == i { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) }),
                _ => (String::new(), Style::default()),
            };
            Line::from(vec![
                Span::styled(if selection == i { "▶ " } else { "  " }, Style::default().fg(if selection == i { Color::Yellow } else { Color::White })),
                Span::styled(text, style),
            ])
        })
        .collect();

    let menu = Paragraph::new(menu_items)
        .style(Style::default().fg(Color::White))
        .block(Block::default().borders(Borders::ALL).title(t("main_menu_title")))
        .alignment(Alignment::Left);
    f.render_widget(menu, chunks[1]);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status
    let help_text = format!("{} | {} | {} | {}", t("help_navigate"), t("help_select"), t("help_quit"), t("help_back"));
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_subscriptions(f: &mut Frame, subscriptions: &[Subscription], list_state: &mut ListState, status: &str, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("Subscriptions")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Subscription list
    let items: Vec<ListItem> = subscriptions
        .iter()
        .enumerate()
        .map(|(i, sub)| {
            let content = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", i + 1),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        &sub.channel_title,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Subscriptions"))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[1], list_state);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status bar
    let help_text = "↑/↓/j/k: Navigate | Enter/Space: View Videos | r: Refresh | Esc/m: Back | q: Quit";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_playlists(f: &mut Frame, playlists: &[Playlist], list_state: &mut ListState, status: &str, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("Playlists")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Playlist list
    let items: Vec<ListItem> = playlists
        .iter()
        .enumerate()
        .map(|(i, playlist)| {
            let content = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", i + 1),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        &playlist.title,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{} videos", playlist.item_count),
                        Style::default().fg(Color::Gray),
                    ),
                ]),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Playlists"))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[1], list_state);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status bar
    let help_text = "↑/↓/j/k: Navigate | Enter/Space: View Videos | r: Refresh | Esc/m: Back | q: Quit";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_input(f: &mut Frame, channel_url: &str, status: &str, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title
    let title = Paragraph::new("Browse Channel by URL")
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

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[3]);

    // Status
    let help_text = "Examples: https://www.youtube.com/@channelname/videos | Press Enter to load | Esc/m: Back | q: Quit";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[4]);
}

fn ui_videos(f: &mut Frame, videos: &[Video], list_state: &mut ListState, status: &str, current_page: usize, total_pages: usize, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.size());

    // Title with pagination
    let title_text = if total_pages > 1 {
        format!("Videos (Page {}/{})", current_page, total_pages)
    } else {
        "Videos".to_string()
    };
    let title = Paragraph::new(title_text)
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
            // Calculate global index: (current_page - 1) * 20 + local_index + 1
            let global_index = (current_page - 1) * 20 + i + 1;
            let content = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", global_index),
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
        .block(Block::default().borders(Borders::ALL).title("Videos"))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[1], list_state);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status bar
    let help_text = if total_pages > 1 {
        "↑/↓/j/k: Navigate | p: Play | d: Download | c: Cancel | ←/→: Prev/Next Page | r: Refresh | Esc: Back | q: Quit"
    } else {
        "↑/↓/j/k: Navigate | p: Play | d: Download | c: Cancel | r: Refresh | Esc: Back | q: Quit"
    };
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_channel_with_tabs(f: &mut Frame, videos: &[Video], list_state: &mut ListState, channel_name: &str, active_tab: usize, status: &str, current_page: usize, total_pages: usize, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title with tabs
            Constraint::Min(0),     // Content
            Constraint::Length(3), // Log
            Constraint::Length(3), // Status
        ])
        .split(f.size());

    // Title with tabs
    let tabs = vec!["Videos", "Shorts", "Playlists"];
    let mut tab_spans = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::raw(" | "));
        }
        let is_active = i == active_tab;
        let style = if is_active {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::Gray)
        };
        tab_spans.push(Span::styled(*tab, style));
    }
    let tab_line = Line::from(tab_spans);
    
    let title_text = if total_pages > 1 && active_tab != 2 {
        format!("{} - {} (Page {}/{})", channel_name, tabs[active_tab], current_page, total_pages)
    } else {
        format!("{} - {}", channel_name, tabs[active_tab])
    };
    
    let title = Paragraph::new(vec![
        Line::from(tab_line),
        Line::from(title_text),
    ])
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
            // Calculate global index: (current_page - 1) * 20 + local_index + 1
            let global_index = (current_page - 1) * 20 + i + 1;
            let content = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", global_index),
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
        .block(Block::default().borders(Borders::ALL).title(tabs[active_tab]))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[1], list_state);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status bar
    let help_text = if active_tab == 2 {
        "↑/↓/j/k: Navigate | Enter/Space: View Playlist | ←/→/1/2/3: Switch Tab | Esc: Back | q: Quit"
    } else if total_pages > 1 {
        "↑/↓/j/k: Navigate | p: Play | d: Download | c: Cancel | ←/→/1/2/3: Switch Tab | n/b: Page | r: Refresh | Esc: Back | q: Quit"
    } else {
        "↑/↓/j/k: Navigate | p: Play | d: Download | c: Cancel | ←/→/1/2/3: Switch Tab | r: Refresh | Esc: Back | q: Quit"
    };
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn ui_channel_with_tabs_playlists(f: &mut Frame, playlists: &[Playlist], list_state: &mut ListState, channel_name: &str, active_tab: usize, status: &str, log: &str) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title with tabs
            Constraint::Min(0),     // Content
            Constraint::Length(3), // Log
            Constraint::Length(3), // Status
        ])
        .split(f.size());

    // Title with tabs
    let tabs = vec!["Videos", "Shorts", "Playlists"];
    let mut tab_spans = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::raw(" | "));
        }
        let is_active = i == active_tab;
        let style = if is_active {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::Gray)
        };
        tab_spans.push(Span::styled(*tab, style));
    }
    let tab_line = Line::from(tab_spans);
    
    let title_text = format!("{} - {}", channel_name, tabs[active_tab]);
    
    let title = Paragraph::new(vec![
        Line::from(tab_line),
        Line::from(title_text),
    ])
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Playlist list
    let items: Vec<ListItem> = playlists
        .iter()
        .enumerate()
        .map(|(i, playlist)| {
            let content = vec![
                Line::from(vec![
                    Span::styled(
                        format!("{}. ", i + 1),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(
                        &playlist.title,
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{} videos", playlist.item_count),
                        Style::default().fg(Color::Gray),
                    ),
                ]),
            ];
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(tabs[active_tab]))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");
    f.render_stateful_widget(list, chunks[1], list_state);

    // Log output (pink box)
    let log_text = if log.is_empty() { "Ready" } else { log };
    let log_widget = Paragraph::new(log_text)
        .style(Style::default().fg(Color::Magenta))
        .block(Block::default().borders(Borders::ALL).title("yt-dlp Output"))
        .wrap(Wrap { trim: true });
    f.render_widget(log_widget, chunks[2]);

    // Status bar
    let help_text = "↑/↓/j/k: Navigate | Enter/Space: View Playlist | ←/→/1/2/3: Switch Tab | Esc: Back | q: Quit";
    let status_text = format!("{} | {}", status, help_text);
    let status_widget = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Green))
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(status_widget, chunks[3]);
}

fn format_date(date_str: &str) -> String {
    if date_str.is_empty() {
        return "Unknown date".to_string();
    }
    
    // Try to parse ISO 8601 format (from API)
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(dt);
        let days = duration.num_days();
        
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
        } else {
            return "Today".to_string();
        }
    }
    
    // Try to parse YYYY-MM-DD format (from yt-dlp)
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
