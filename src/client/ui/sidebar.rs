use crate::client::app::{App, Focus};
use crate::theme::ThemeColors;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub(super) fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect, t: &ThemeColors) {
    let active = app.focus == Focus::Sidebar;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let workspace_name = "Mato Corn";

    let office_style = if active {
        if t.follow_terminal {
            Style::default()
                .add_modifier(ratatui::style::Modifier::BOLD | ratatui::style::Modifier::REVERSED)
        } else {
            Style::default()
                .fg(t.fg())
                .bg(t.surface())
                .add_modifier(ratatui::style::Modifier::BOLD)
        }
    } else if t.follow_terminal {
        Style::default().add_modifier(ratatui::style::Modifier::DIM)
    } else {
        Style::default().fg(t.fg_dim()).bg(t.surface())
    };

    let office_text = Line::from(vec![
        Span::styled(" ▣ ", office_style),
        Span::styled(workspace_name, office_style),
    ]);

    f.render_widget(
        ratatui::widgets::Paragraph::new(office_text)
            .alignment(ratatui::layout::Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(super::border_type(t, active))
                    .border_style(super::border_style(t, active))
                    .style(Style::default().bg(t.surface())),
            ),
        rows[0],
    );
    app.new_desk_area = Rect::default();

    let selected_desk_idx = app.selected();
    let items: Vec<ListItem> = app.offices[app.current_office]
        .desks
        .iter()
        .enumerate()
        .map(|(i, task)| {
            let sel = app.list_state.selected() == Some(i);
            let alarm_count = task
                .tabs
                .iter()
                .filter(|tab| app.alarm_tabs.contains(&tab.id))
                .count();

            // Active desk should never show spinner in sidebar.
            let has_spinner = if i == selected_desk_idx {
                false
            } else {
                task.tabs
                    .iter()
                    .any(|tab| app.active_tabs.contains(&tab.id))
            };

            let mut name = if alarm_count > 0 {
                format!("{} ⚑{}", task.name, alarm_count)
            } else {
                task.name.clone()
            };
            if has_spinner {
                name = format!("{} {}", name, app.get_spinner());
            }
            let has_alarm_pulse = alarm_count > 0 && app.alarm_pulse_on();
            let item_style = if has_alarm_pulse && !t.follow_terminal {
                Style::default()
                    .fg(t.accent2())
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else if has_alarm_pulse {
                Style::default().add_modifier(ratatui::style::Modifier::BOLD)
            } else if t.follow_terminal {
                if sel {
                    Style::default().add_modifier(
                        ratatui::style::Modifier::BOLD | ratatui::style::Modifier::REVERSED,
                    )
                } else if alarm_count > 0 {
                    Style::default().add_modifier(ratatui::style::Modifier::BOLD)
                } else {
                    Style::default().add_modifier(ratatui::style::Modifier::DIM)
                }
            } else if alarm_count > 0 {
                Style::default()
                    .fg(t.accent2())
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                Style::default().fg(if sel { t.fg() } else { t.fg_dim() })
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    if sel { " ▶ " } else { "   " },
                    Style::default().fg(t.accent()),
                ),
                Span::styled(name, item_style),
            ]))
            .style(Style::default().bg(if sel { t.sel_bg() } else { t.surface() }))
        })
        .collect();

    app.sidebar_list_area = rows[1];
    f.render_stateful_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(super::border_type(t, active))
                .title(Span::styled(" Desks ", super::title_style(t, active)))
                .border_style(super::border_style(t, active))
                .style(Style::default().bg(t.surface())),
        ),
        rows[1],
        &mut app.list_state,
    );
}
