mod api;
mod app;
mod colors;
mod constants;
mod diff;
mod editor;
mod generation;
mod github;
mod input;
mod layout;
mod model;
mod protocol;
mod scroll;
mod search;
mod session;
mod settings;
mod ui;

use std::io::{self, IsTerminal, Read, stdout};

use clap::Parser;
use crossterm::{
    ExecutableCommand,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use api::{ClaudeClient, TokenUsage};
use app::App;
use constants::{EVENT_POLL_INTERVAL, EVENT_RECV_TIMEOUT, VIEWPORT_HEIGHT_OFFSET};
use diff::FileFilter;
use generation::{StreamEvent, WalkthroughGenerator, create_sub_steps, format_step_for_rechunk};
use input::InputHandler;
use model::{CommitInfo, Message, ReviewMode, Step, Walkthrough};
#[cfg(debug_assertions)]
use model::mock_walkthrough;
use settings::Settings;

enum AppEvent {
    Terminal(Event),
    GenerationComplete(TokenUsage),
    GenerationError(String),
    StepReady(Step),
    ChatChunk(usize, String),
    ChatComplete(usize, TokenUsage),
    ChatError(usize, String),
    RechunkComplete(usize, Vec<Step>, TokenUsage),
    RechunkError(String),
}

fn spawn_walkthrough_generation(
    tx: mpsc::Sender<AppEvent>,
    api_key: String,
    diff_text: String,
    filter: FileFilter,
    mode: ReviewMode,
    commits: Vec<CommitInfo>,
) {
    tokio::spawn(async move {
        match WalkthroughGenerator::with_filter(&diff_text, &filter, mode, api_key, commits) {
            Ok(generator) => {
                let (event_tx, mut event_rx) = mpsc::channel::<StreamEvent>(32);

                let tx_forward = tx.clone();
                let forward_task = tokio::spawn(async move {
                    while let Some(StreamEvent::StepReady(s)) = event_rx.recv().await {
                        if tx_forward.send(AppEvent::StepReady(s)).await.is_err() {
                            break;
                        }
                    }
                });

                match generator.generate_streaming(event_tx).await {
                    Ok(usage) => {
                        let _ = forward_task.await;
                        let _ = tx.send(AppEvent::GenerationComplete(usage)).await;
                    }
                    Err(e) => {
                        let _ = tx.send(AppEvent::GenerationError(e.to_string())).await;
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(AppEvent::GenerationError(e.to_string())).await;
            }
        }
    });
}

/// Spawns a thread to read terminal events and forward them to the event channel
fn spawn_terminal_reader(tx: mpsc::Sender<AppEvent>) {
    std::thread::spawn(move || {
        loop {
            match event::poll(EVENT_POLL_INTERVAL) {
                Ok(true) => {
                    if let Ok(evt) = event::read()
                        && tx.blocking_send(AppEvent::Terminal(evt)).is_err()
                    {
                        break;
                    }
                }
                Ok(false) => {}
                Err(_) => {
                    std::thread::sleep(EVENT_POLL_INTERVAL);
                }
            }
        }
    });
}

/// Spawns a task to handle streaming chat with the Claude API
fn spawn_chat_handler(
    tx: mpsc::Sender<AppEvent>,
    api_key: String,
    step_index: usize,
    walkthrough: Walkthrough,
    messages: Vec<Message>,
    mode: ReviewMode,
) {
    tokio::spawn(async move {
        let client = ClaudeClient::new(api_key);
        let (chunk_tx, mut chunk_rx) = mpsc::channel::<String>(32);

        let tx_chunks = tx.clone();
        let forward_task = tokio::spawn(async move {
            while let Some(chunk) = chunk_rx.recv().await {
                if tx_chunks
                    .send(AppEvent::ChatChunk(step_index, chunk))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        match client
            .chat_streaming(&walkthrough, step_index, &messages, mode, chunk_tx)
            .await
        {
            Ok(usage) => {
                let _ = forward_task.await;
                let _ = tx.send(AppEvent::ChatComplete(step_index, usage)).await;
            }
            Err(e) => {
                let _ = tx
                    .send(AppEvent::ChatError(step_index, e.to_string()))
                    .await;
            }
        }
    });
}

fn spawn_rechunk(
    tx: mpsc::Sender<AppEvent>,
    api_key: String,
    step_index: usize,
    step: Step,
    diff_text: Option<String>,
    mode: ReviewMode,
) {
    tokio::spawn(async move {
        let client = ClaudeClient::new(api_key);
        let step_content = format_step_for_rechunk(&step);
        let mut prompt = format!(
            "Please split this step into smaller sub-steps.\n\n\
             ## Step: {}\n\n{}\n\n## Hunks\n\n{}",
            step.title, step.summary, step_content
        );
        if let Some(diff) = diff_text {
            prompt.push_str(&format!(
                "\n\nHere is the full diff for context:\n\n{}",
                diff
            ));
        }

        match client.rechunk_step(&prompt, mode).await {
            Ok((response, usage)) => match create_sub_steps(&step, response, &step.id) {
                Ok(sub_steps) => {
                    let _ = tx
                        .send(AppEvent::RechunkComplete(step_index, sub_steps, usage))
                        .await;
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::RechunkError(e.to_string())).await;
                }
            },
            Err(e) => {
                let _ = tx.send(AppEvent::RechunkError(e.to_string())).await;
            }
        }
    });
}

/// AI-guided code review walkthrough tool
#[derive(Parser, Debug)]
#[command(name = "docent", version, about)]
struct Args {
    /// Path to a diff/patch file or GitHub PR URL (or pipe diff via stdin)
    #[arg(value_name = "FILE_OR_URL")]
    diff_file: Option<String>,

    /// Use mock data instead of generating from a diff
    #[cfg(debug_assertions)]
    #[arg(long = "mock")]
    use_mock: bool,

    /// Only include files matching these glob patterns (e.g., "*.clj", "src/**/*.rs")
    #[arg(short = 'f', long = "filter", value_name = "PATTERN")]
    filters: Vec<String>,

    /// Exclude files matching these glob patterns
    #[arg(short = 'x', long = "exclude", value_name = "PATTERN")]
    excludes: Vec<String>,

    /// Walkthrough mode: describe the changes instead of giving an opinionated review
    #[arg(short = 'w', long = "walkthrough")]
    walkthrough: bool,

    /// Run in headless mode (server only, no TUI)
    #[arg(long = "headless")]
    headless: bool,
}

struct DiffInput {
    diff_text: String,
    commits:   Vec<CommitInfo>,
}

async fn read_diff_input(args: &Args) -> io::Result<Option<DiffInput>> {
    #[cfg(debug_assertions)]
    if args.use_mock {
        return Ok(None);
    }

    if let Some(input) = &args.diff_file {
        if let Some(parsed) = github::parse_github_url(input) {
            let diff = github::fetch_diff(input).await.map_err(io::Error::other)?;
            let commits = if let github::GitHubUrl::PullRequest { owner, repo, number } = parsed {
                github::fetch_pr_commits(owner, repo, number)
                    .await
                    .unwrap_or_default()
            } else {
                vec![]
            };
            return Ok(Some(DiffInput { diff_text: diff, commits }));
        }
        if input.starts_with("https://") || input.starts_with("http://") {
            return Err(io::Error::other(format!(
                "Unsupported URL: {input}\nExpected a GitHub URL like:\n  https://github.com/owner/repo/pull/123\n  https://github.com/owner/repo/commit/<sha>\n  https://github.com/owner/repo/compare/base...head"
            )));
        }
        if is_git_range(input) {
            return read_git_range(input).await.map(Some);
        }
        return Ok(Some(DiffInput { diff_text: std::fs::read_to_string(input)?, commits: vec![] }));
    }

    // Check if stdin is piped - read from it before crossterm initializes
    // The "use-dev-tty" feature in crossterm will handle terminal events from /dev/tty
    if !std::io::stdin().is_terminal() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;

        if input.trim().is_empty() {
            return Ok(None);
        }
        return Ok(Some(DiffInput { diff_text: input, commits: vec![] }));
    }

    Ok(None)
}

fn is_git_range(input: &str) -> bool {
    (input.contains("..") || input.contains("..."))
        && !input.starts_with("http")
        && !std::path::Path::new(input).exists()
}

async fn read_git_range(range: &str) -> io::Result<DiffInput> {
    let diff_output = tokio::process::Command::new("git")
        .args(["diff", range])
        .output()
        .await
        .map_err(|e| io::Error::other(format!("failed to run git diff: {e}")))?;

    if !diff_output.status.success() {
        let stderr = String::from_utf8_lossy(&diff_output.stderr);
        return Err(io::Error::other(format!("git diff {range} failed: {stderr}")));
    }

    let diff_text = String::from_utf8_lossy(&diff_output.stdout).into_owned();
    if diff_text.trim().is_empty() {
        return Err(io::Error::other(format!("git diff {range} produced no output")));
    }

    let commits = read_git_log(range).await.unwrap_or_default();

    Ok(DiffInput { diff_text, commits })
}

const GIT_LOG_SEPARATOR: &str = "---commit-boundary---";

async fn read_git_log(range: &str) -> io::Result<Vec<CommitInfo>> {
    let format = format!("{GIT_LOG_SEPARATOR}%n%H%n%s");
    let output = tokio::process::Command::new("git")
        .args(["log", "--reverse", &format!("--format={format}"), "--name-only", range])
        .output()
        .await?;

    if !output.status.success() {
        return Err(io::Error::other("git log failed"));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();

    for block in text.split(GIT_LOG_SEPARATOR).skip(1) {
        let mut lines = block.lines().filter(|l| !l.is_empty());
        let Some(sha) = lines.next() else { continue };
        let Some(message) = lines.next() else { continue };
        let files: Vec<String> = lines.map(String::from).collect();
        commits.push(CommitInfo {
            sha:     sha.to_string(),
            message: message.to_string(),
            files,
        });
    }

    Ok(commits)
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();
    let diff_input = read_diff_input(&args).await?;

    // Build and validate the file filter early
    let filter = FileFilter::new(&args.filters, &args.excludes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;

    let mode = if args.walkthrough {
        ReviewMode::Walkthrough
    } else {
        ReviewMode::default()
    };

    if args.headless {
        return headless::run(diff_input, filter, mode).await;
    }

    // Emit OSC 7 so tmux knows our working directory for new panes/windows
    if let Ok(cwd) = std::env::current_dir() {
        use std::io::Write;
        let _ = write!(stdout(), "\x1b]7;file://localhost{}\x1b\\", cwd.display());
        let _ = stdout().flush();
    }

    // Setup terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    // Run app
    let result = run_app(&mut terminal, diff_input, filter, mode).await;

    // Restore terminal
    stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_app<B: Backend + Send>(
    terminal: &mut Terminal<B>,
    diff_input: Option<DiffInput>,
    filter: FileFilter,
    mode: ReviewMode,
) -> io::Result<()> {
    let mut settings = Settings::load();

    let mut app = if let Some(diff) = diff_input {
        let mut app = App::setup(&settings, mode);
        app.session.commits = diff.commits;
        app.session.diff_input = Some(diff.diff_text);
        app.session.diff_filter = filter;
        app
    } else {
        #[cfg(debug_assertions)]
        {
            App::new(mock_walkthrough(), &settings, mode)
        }
        #[cfg(not(debug_assertions))]
        {
            App::setup(&settings, mode)
        }
    };

    let mut input_handler = InputHandler::new();
    let (tx, mut rx) = mpsc::channel::<AppEvent>(32);

    spawn_terminal_reader(tx.clone());

    loop {
        let viewport_height = terminal
            .size()?
            .height
            .saturating_sub(VIEWPORT_HEIGHT_OFFSET) as usize;

        terminal.draw(|frame| {
            ui::render(frame, &app);
        })?;

        if let Ok(Some(event)) = tokio::time::timeout(EVENT_RECV_TIMEOUT, rx.recv()).await {
            handle_app_event(
                event,
                &mut app,
                &mut input_handler,
                terminal,
                viewport_height,
            )?;
        }

        if let Some((step_index, walkthrough, messages)) = app.session.chat_request.take() {
            spawn_chat_handler(
                tx.clone(),
                app.session.api_key_input.clone(),
                step_index,
                walkthrough,
                messages,
                app.session.review_mode,
            );
        }

        if let Some((step_index, step, diff_text)) = app.session.rechunk_request.take() {
            spawn_rechunk(
                tx.clone(),
                app.session.api_key_input.clone(),
                step_index,
                step,
                diff_text,
                app.session.review_mode,
            );
        }

        if app.should_quit {
            break;
        }

        let should_generate = app.session.generation_requested || app.session.retry_requested;
        if app.session.generation_requested
            && app.session.api_key_source == settings::ApiKeySource::UserEntry
        {
            settings.api_key = Some(app.session.api_key_input.clone());
            let _ = settings.save();
        }
        app.session.generation_requested = false;
        app.session.retry_requested = false;

        if should_generate
            && let Some(diff_text) = app.session.diff_input.clone()
        {
            spawn_walkthrough_generation(
                tx.clone(),
                app.session.api_key_input.clone(),
                diff_text,
                app.session.diff_filter.clone(),
                app.session.review_mode,
                app.session.commits.clone(),
            );
        }
    }

    Ok(())
}

fn handle_app_event<B: Backend>(
    event: AppEvent,
    app: &mut App,
    input_handler: &mut InputHandler,
    terminal: &Terminal<B>,
    viewport_height: usize,
) -> io::Result<()> {
    match event {
        AppEvent::Terminal(Event::Key(key)) => {
            input_handler.handle_key(key, app, viewport_height);
        }
        AppEvent::Terminal(Event::Mouse(mouse)) => {
            input_handler.handle_mouse(mouse, app, terminal.size()?);
        }
        AppEvent::Terminal(_) => {}
        AppEvent::GenerationComplete(usage) => {
            app.session.add_usage(usage);
            if app.session.walkthrough.steps.is_empty() {
                app.session.generation_in_progress = false;
                app.session.set_error(
                    "Generation completed but no steps were produced".to_string(),
                );
            } else {
                app.session.generation_finished();
            }
        }
        AppEvent::GenerationError(message) => {
            if app.session.generation_in_progress
                && !app.session.walkthrough.steps.is_empty()
            {
                app.session.generation_finished();
            } else {
                app.session.generation_in_progress = false;
                app.session.set_error(message);
            }
        }
        AppEvent::StepReady(step) => {
            app.session.receive_step_ready(step);
        }
        AppEvent::ChatChunk(step_index, chunk) => {
            app.session.receive_chat_chunk(step_index, chunk);
        }
        AppEvent::ChatComplete(step_index, usage) => {
            app.session.add_usage(usage);
            app.session.receive_chat_complete(step_index);
        }
        AppEvent::ChatError(step_index, error) => {
            app.session.receive_chat_error(step_index, error);
        }
        AppEvent::RechunkComplete(step_index, sub_steps, usage) => {
            app.session.add_usage(usage);
            app.receive_rechunk_complete(step_index, sub_steps);
        }
        AppEvent::RechunkError(error) => {
            app.session.receive_rechunk_error(error);
        }
    }
    Ok(())
}

mod headless;
