use crate::{
    app::{App, ReviewAgentState, ReviewCommentStatus},
    markdown::display_width,
    theme::app_theme,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
    Frame,
};

use super::centered_rect;

pub(super) fn render_comments_panel(f: &mut Frame, app: &App, area: Rect) {
    let theme = app_theme();
    let inner_width = area.width.saturating_sub(4).max(1) as usize;
    let (lines, active_start) = comment_panel_lines(app, inner_width);
    let visible_height = area.height.saturating_sub(2) as usize;
    let max_scroll = lines.len().saturating_sub(visible_height);
    let scroll = active_start.saturating_sub(1).min(max_scroll) as u16;

    let title = if app.is_review_bridge_connected() {
        match app.review_agent_state() {
            ReviewAgentState::Working { comment_count, .. } => {
                format!(
                    "─ Comments {} · Pi addressing {comment_count} ",
                    app.comment_count()
                )
            }
            ReviewAgentState::Disconnected(_) => {
                format!("─ Comments {} · Pi disconnected ", app.comment_count())
            }
            ReviewAgentState::Error(_) => {
                format!("─ Comments {} · needs attention ", app.comment_count())
            }
            ReviewAgentState::Ready => format!(
                "─ Comments {} · {} addressed ",
                app.comment_count(),
                app.addressed_comment_count()
            ),
        }
    } else {
        format!("─ Comments {} · local only ", app.comment_count())
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.ui.toc_border))
        .style(Style::default().bg(theme.ui.toc_bg))
        .padding(Padding::horizontal(1));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(theme.ui.toc_bg))
            .scroll((scroll, 0)),
        area,
    );
}

pub(super) fn render_comment_composer(f: &mut Frame, app: &App) {
    let Some(composer) = app.comment_composer() else {
        return;
    };
    let theme = app_theme();
    let action = if composer.editing_id.is_some() {
        "Edit comment"
    } else {
        "Add comment"
    };
    let area = centered_rect(72, 12, f.area());
    let block = Block::default()
        .title(format!("─ {action} · line {} ", composer.source_line))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.ui.toc_accent))
        .style(Style::default().bg(theme.ui.toc_bg))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let target = composer
        .selected_text
        .as_deref()
        .map(|text| format!("Target: “{text}”"))
        .unwrap_or_else(|| "Target: entire line".to_string());
    f.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(
                    if app.is_review_bridge_connected() {
                        "Pi review"
                    } else {
                        "UI prototype"
                    },
                    Style::default()
                        .fg(theme.ui.toc_accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if app.is_review_bridge_connected() {
                        "  Save comments, then press s to send them."
                    } else {
                        "  Comments live only for this open document."
                    },
                    Style::default().fg(theme.ui.toc_secondary_text_inactive),
                ),
            ]),
            Line::from(Span::styled(
                target,
                Style::default().fg(theme.markdown.link_hover),
            )),
        ]),
        rows[0],
    );

    let input = if composer.draft.is_empty() {
        vec![Line::from(vec![
            Span::styled(
                "Write a comment…",
                Style::default().fg(theme.ui.toc_secondary_inactive),
            ),
            Span::styled("▌", Style::default().fg(theme.markdown.link_hover)),
        ])]
    } else {
        let mut lines = composer
            .draft
            .split('\n')
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(theme.ui.toc_primary_active),
                ))
            })
            .collect::<Vec<_>>();
        if let Some(last) = lines.last_mut() {
            last.spans.push(Span::styled(
                "▌",
                Style::default().fg(theme.markdown.link_hover),
            ));
        }
        lines
    };
    f.render_widget(
        Paragraph::new(input)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.ui.toc_border))
                    .style(Style::default().bg(theme.ui.content_bg)),
            )
            .style(Style::default().bg(theme.ui.content_bg))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        rows[1],
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "enter save",
                Style::default().fg(theme.ui.status_shortcut_fg),
            ),
            Span::styled(" · ", Style::default().fg(theme.ui.status_separator)),
            Span::styled(
                "alt+enter newline",
                Style::default().fg(theme.ui.status_shortcut_fg),
            ),
            Span::styled(" · ", Style::default().fg(theme.ui.status_separator)),
            Span::styled(
                "esc cancel",
                Style::default().fg(theme.ui.status_shortcut_fg),
            ),
        ])),
        rows[2],
    );
}

pub(super) fn comment_panel_lines(app: &App, width: usize) -> (Vec<Line<'static>>, usize) {
    let theme = app_theme();
    let mut lines = Vec::new();
    let mut active_start = 0usize;

    for comment in app.comments() {
        let active = app.active_comment_id() == Some(comment.id);
        if active {
            active_start = lines.len();
        }
        let bg = if active {
            theme.ui.toc_active_bg
        } else {
            theme.ui.toc_bg
        };
        let (marker, marker_color, status_label) = match (&comment.status, active) {
            (ReviewCommentStatus::Draft, false) => ("●", theme.ui.toc_accent, ""),
            (ReviewCommentStatus::Draft, true) => ("◆", theme.ui.toc_accent, ""),
            (ReviewCommentStatus::Submitted { .. }, false) => {
                ("◌", theme.ui.status_warning_fg, "  sent")
            }
            (ReviewCommentStatus::Submitted { .. }, true) => {
                ("◈", theme.ui.status_warning_fg, "  sent")
            }
            (ReviewCommentStatus::Addressed, _) => ("✓", theme.ui.status_success_fg, "  addressed"),
        };
        lines.push(padded_line(
            vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default()
                        .fg(marker_color)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("Line {}", comment.source_line),
                    Style::default()
                        .fg(theme.ui.toc_primary_active)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "  You",
                    Style::default()
                        .fg(theme.ui.toc_secondary_text_inactive)
                        .bg(bg),
                ),
                Span::styled(status_label, Style::default().fg(marker_color).bg(bg)),
            ],
            width,
            bg,
        ));

        if let Some(selected_text) = &comment.selected_text {
            for selected_line in wrap_comment_text(selected_text, width.saturating_sub(4).max(1)) {
                lines.push(padded_line(
                    vec![
                        Span::styled("  › ", Style::default().fg(marker_color).bg(bg)),
                        Span::styled(
                            selected_line,
                            Style::default()
                                .fg(if comment.status.is_addressed() {
                                    theme.ui.toc_secondary_text_inactive
                                } else {
                                    theme.markdown.link_hover
                                })
                                .bg(bg)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ],
                    width,
                    bg,
                ));
            }
        }

        for body_line in wrap_comment_text(&comment.body, width.saturating_sub(2).max(1)) {
            lines.push(padded_line(
                vec![
                    Span::styled("  ", Style::default().fg(theme.ui.toc_border).bg(bg)),
                    Span::styled(
                        body_line,
                        Style::default()
                            .fg(if comment.status.is_addressed() {
                                theme.ui.toc_secondary_text_inactive
                            } else {
                                theme.ui.toc_primary_inactive
                            })
                            .bg(bg),
                    ),
                ],
                width,
                bg,
            ));
        }
        lines.push(Line::from(Span::styled(
            " ".repeat(width),
            Style::default().bg(theme.ui.toc_bg),
        )));
    }

    (lines, active_start)
}

fn padded_line(
    mut spans: Vec<Span<'static>>,
    width: usize,
    bg: ratatui::style::Color,
) -> Line<'static> {
    let used = spans
        .iter()
        .map(|span| display_width(span.content.as_ref()))
        .sum::<usize>();
    if used < width {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    Line::from(spans)
}

fn wrap_comment_text(text: &str, width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let separator = usize::from(!current.is_empty());
            if display_width(&current) + separator + display_width(word) <= width {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
                continue;
            }
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            push_wrapped_word(word, width, &mut result, &mut current);
        }
        if !current.is_empty() {
            result.push(current);
        }
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

fn push_wrapped_word(word: &str, width: usize, result: &mut Vec<String>, current: &mut String) {
    for ch in word.chars() {
        if !current.is_empty() && display_width(current) + display_width(&ch.to_string()) > width {
            result.push(std::mem::take(current));
        }
        current.push(ch);
    }
}
