//! The `signin-ask` popup: for every pane queued by the `on-agent-detected`
//! hook, ask whether to sign that pane in to chat. yes/always inject a scrubbed
//! `/chat:sign-in`; always/never persist the repo's preference. The pending
//! file is drained again after the popup so a pane queued while it was open (its
//! own open having hit `ui_busy`) is picked up rather than stranded.

use crate::cmd::detect::repo_from_panes;
use crate::rt;
use crate::run::Runner;
use crate::state::{self, SigninPref};
use crate::theme::{self, AppTheme};
use crate::ui::{self, Flow};

use crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Clone, Copy, PartialEq)]
enum Choice {
    Yes,
    Always,
    Never,
    Skip,
}

pub fn run(runner: &dyn Runner) -> Result<(), String> {
    let dir = state::state_dir();
    let theme = theme::load();
    // Drain, present, apply; then drain again. A pane queued while the popup was
    // open (whose own `plugin pane open` hit ui_busy) lands here on the re-drain
    // rather than waiting for the next detection.
    loop {
        let panes = state::drain_pending(&dir).map_err(|e| e.to_string())?;
        if panes.is_empty() {
            break;
        }
        let list = rt::pane_list(runner).unwrap_or_default();
        let repos: Vec<String> = panes
            .iter()
            .map(|pane| repo_from_panes(&list, pane).unwrap_or_else(|| pane.clone()))
            .collect();
        let choices = prompt(&theme, &panes, &repos).map_err(|e| e.to_string())?;
        apply(runner, &dir, &panes, &repos, &choices)?;
    }
    Ok(())
}

/// Run the modal popup over the queued panes, one prompt per pane, and return a
/// choice for each. Esc or `q` skips the current pane and every one after it.
fn prompt(theme: &AppTheme, panes: &[String], repos: &[String]) -> std::io::Result<Vec<Choice>> {
    let mut idx = 0usize;
    let mut choices = vec![Choice::Skip; panes.len()];
    ui::popup(theme, |frame, key| {
        let mut exit = false;
        if let Some(key) = key {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    choices[idx] = Choice::Yes;
                    idx += 1;
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    choices[idx] = Choice::Always;
                    idx += 1;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    choices[idx] = Choice::Never;
                    idx += 1;
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    choices[idx] = Choice::Skip;
                    idx += 1;
                }
                KeyCode::Esc | KeyCode::Char('q') => exit = true,
                _ => {}
            }
            if idx >= panes.len() {
                exit = true;
            }
        }
        let shown = idx.min(panes.len().saturating_sub(1));
        draw(frame, theme, panes, repos, shown);
        if exit {
            Flow::Exit
        } else {
            Flow::Continue
        }
    })?;
    Ok(choices)
}

fn apply(
    runner: &dyn Runner,
    dir: &std::path::Path,
    panes: &[String],
    repos: &[String],
    choices: &[Choice],
) -> Result<(), String> {
    for ((pane, repo), choice) in panes.iter().zip(repos).zip(choices) {
        match choice {
            Choice::Yes => {
                rt::pane_send(runner, pane, "/chat:sign-in", true)?;
            }
            Choice::Always => {
                state::set_pref(dir, repo, SigninPref::Always).map_err(|e| e.to_string())?;
                rt::pane_send(runner, pane, "/chat:sign-in", true)?;
            }
            Choice::Never => {
                state::set_pref(dir, repo, SigninPref::Never).map_err(|e| e.to_string())?;
            }
            Choice::Skip => {}
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, theme: &AppTheme, panes: &[String], repos: &[String], idx: usize) {
    let area = centered(frame.area(), 62, 9);
    frame.render_widget(Clear, area);

    let title = format!(" chat sign-in ({}/{}) ", idx + 1, panes.len());
    let block = Block::new()
        .borders(Borders::ALL)
        .title(title)
        .border_style(theme.border)
        .style(theme.base);

    let body = vec![
        Line::from(Span::styled(
            format!("Agent detected in {}", repos[idx]),
            theme.accent,
        )),
        Line::from(Span::styled(format!("pane {}", panes[idx]), theme.dim)),
        Line::from(""),
        Line::from("Sign in to chat?"),
        Line::from(vec![
            Span::styled("[y]", theme.accent),
            Span::raw("es  "),
            Span::styled("[a]", theme.accent),
            Span::raw("lways  "),
            Span::styled("[n]", theme.accent),
            Span::raw("ever  "),
            Span::styled("[s]", theme.accent),
            Span::raw("kip"),
        ]),
    ];

    let para = Paragraph::new(body)
        .block(block)
        .style(theme.base)
        .wrap(Wrap { trim: true });
    frame.render_widget(para, area);
}

/// A `width` by `height` rect centered in `area`, clamped to fit.
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}
