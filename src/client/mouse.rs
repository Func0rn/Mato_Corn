use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::Rect;

use super::app::{App, Focus};

pub fn handle_mouse(app: &mut App, me: crossterm::event::MouseEvent) {
    let (col, row) = (me.column, me.row);
    fn in_rect(col: u16, row: u16, r: Rect) -> bool {
        col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
    }

    // Mouse passthrough to PTY when in content focus
    let terminal_inner = Rect {
        x: app.content_area.x.saturating_add(1),
        y: app.content_area.y.saturating_add(1),
        width: app.content_area.width.saturating_sub(2),
        height: app.content_area.height.saturating_sub(2),
    };
    if matches!(app.focus, Focus::Content) && in_rect(col, row, terminal_inner) {
        let tx = col.saturating_sub(terminal_inner.x) + 1;
        let ty = row.saturating_sub(terminal_inner.y) + 1;
        let mouse_mode = app.pty_mouse_mode_enabled();
        match me.kind {
            MouseEventKind::ScrollUp => {
                if mouse_mode {
                    app.pty_write(format!("\x1b[<64;{};{}M", tx, ty).as_bytes());
                } else {
                    app.pty_scroll(5);
                }
                return;
            }
            MouseEventKind::ScrollDown => {
                if mouse_mode {
                    app.pty_write(format!("\x1b[<65;{};{}M", tx, ty).as_bytes());
                } else {
                    app.pty_scroll(-5);
                }
                return;
            }
            _ => {}
        }
        if !mouse_mode {
            if matches!(me.kind, MouseEventKind::Down(MouseButton::Left)) {
                let screen = {
                    let desk_idx = app.selected();
                    let tab_idx = app.offices[app.current_office].desks[desk_idx].active_tab;
                    app.offices[app.current_office].desks[desk_idx].tabs[tab_idx]
                        .provider
                        .get_screen(terminal_inner.height, terminal_inner.width)
                };
                let target_row = ty.saturating_sub(1);
                let target_col = tx.saturating_sub(1).min(terminal_inner.width.saturating_sub(1));

                if target_row == screen.cursor.0 {
                    let bytes = cursor_move_sequence(screen.cursor.1, target_col);
                    if !bytes.is_empty() {
                        app.pty_write(&bytes);
                    }
                }
            }
            return;
        }
        let (btn, is_up) = match me.kind {
            MouseEventKind::Down(MouseButton::Left) => (0u8, false),
            MouseEventKind::Down(MouseButton::Middle) => (1, false),
            MouseEventKind::Down(MouseButton::Right) => (2, false),
            MouseEventKind::Up(_) => (3, true),
            MouseEventKind::Drag(MouseButton::Left) => (32, false),
            MouseEventKind::Moved => (35, false),
            _ => return,
        };
        let suffix = if is_up { 'm' } else { 'M' };
        app.pty_write(format!("\x1b[<{};{};{}{}", btn, tx, ty, suffix).as_bytes());
        return;
    }

    match me.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let mut handled = false;

            // Sidebar desk list
            let a = app.sidebar_list_area;
            if !handled && col >= a.x && col < a.x + a.width && row > a.y && row < a.y + a.height {
                let idx = (row - a.y - 1) as usize;
                if idx < app.offices[app.current_office].desks.len() {
                    let is_double = app
                        .last_click
                        .as_ref()
                        .map(|&(lc, lr, ref t)| {
                            lc == col && lr == row && t.elapsed().as_millis() < 400
                        })
                        .unwrap_or(false);
                    app.select_desk(idx);
                    if is_double {
                        app.focus = Focus::Content;
                        app.spawn_active_pty();
                    } else {
                        app.focus = Focus::Sidebar;
                    }
                    handled = true;
                }
            }

            // Topbar tabs
            if !handled && in_rect(col, row, app.topbar_area) {
                app.focus = Focus::Topbar;
                // New tab button
                if in_rect(col, row, app.new_tab_area) {
                    app.new_tab_inheriting_cwd();
                    app.spawn_active_pty();
                    app.dirty = true;
                    handled = true;
                }
                // Tab click
                if !handled {
                    let tab_areas = app.tab_areas.clone();
                    let tab_indices = app.tab_area_tab_indices.clone();
                    for (i, ta) in tab_areas.iter().enumerate() {
                        if in_rect(col, row, *ta) {
                            let ti = app.selected();
                            // tab_area_tab_indices maps visual position → real tab index
                            let real_tab_idx = tab_indices.get(i).copied().unwrap_or(i);
                            let is_double = app
                                .last_click
                                .as_ref()
                                .map(|&(lc, lr, ref t)| {
                                    lc == col && lr == row && t.elapsed().as_millis() < 400
                                })
                                .unwrap_or(false);
                            if is_double
                                && app.offices[app.current_office].desks[ti].active_tab
                                    == real_tab_idx
                            {
                                app.begin_rename_tab();
                            } else {
                                app.offices[app.current_office].desks[ti].active_tab = real_tab_idx;
                                app.mark_tab_switch();
                                app.spawn_active_pty();
                            }
                            handled = true;
                            break;
                        }
                    }
                }
            }

            // Content area
            if !handled && in_rect(col, row, app.content_area) {
                app.focus = Focus::Content;
                handled = true;
            }
            if !handled && in_rect(col, row, app.sidebar_area) {
                app.focus = Focus::Sidebar;
            }

            app.last_click = Some((col, row, std::time::Instant::now()));
        }
        MouseEventKind::ScrollUp => {
            if in_rect(col, row, app.sidebar_area) {
                app.nav(-1);
            } else if in_rect(col, row, app.topbar_area) {
                let prev = app.tab_scroll;
                app.tab_scroll = app.tab_scroll.saturating_sub(1);
                if app.tab_scroll != prev {
                    app.dirty = true;
                }
            }
        }
        MouseEventKind::ScrollDown => {
            if in_rect(col, row, app.sidebar_area) {
                app.nav(1);
            } else if in_rect(col, row, app.topbar_area) {
                let max = app.offices[app.current_office].desks[app.selected()]
                    .tabs
                    .len()
                    .saturating_sub(1);
                let prev = app.tab_scroll;
                if app.tab_scroll < max {
                    app.tab_scroll += 1;
                }
                if app.tab_scroll != prev {
                    app.dirty = true;
                }
            }
        }
        MouseEventKind::ScrollLeft => {
            if in_rect(col, row, app.topbar_area) {
                let prev = app.tab_scroll;
                app.tab_scroll = app.tab_scroll.saturating_sub(1);
                if app.tab_scroll != prev {
                    app.dirty = true;
                }
            }
        }
        MouseEventKind::ScrollRight => {
            if in_rect(col, row, app.topbar_area) {
                let max = app.offices[app.current_office].desks[app.selected()]
                    .tabs
                    .len()
                    .saturating_sub(1);
                let prev = app.tab_scroll;
                if app.tab_scroll < max {
                    app.tab_scroll += 1;
                }
                if app.tab_scroll != prev {
                    app.dirty = true;
                }
            }
        }
        _ => {}
    }
}

fn cursor_move_sequence(from_col: u16, to_col: u16) -> Vec<u8> {
    let (code, count) = if to_col < from_col {
        ('D', from_col - to_col)
    } else if to_col > from_col {
        ('C', to_col - from_col)
    } else {
        return Vec::new();
    };

    if count == 1 {
        format!("\x1b[{code}").into_bytes()
    } else {
        format!("\x1b[{count}{code}").into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::cursor_move_sequence;

    #[test]
    fn cursor_move_sequence_moves_left() {
        assert_eq!(cursor_move_sequence(8, 3), b"\x1b[5D");
    }

    #[test]
    fn cursor_move_sequence_moves_right() {
        assert_eq!(cursor_move_sequence(2, 6), b"\x1b[4C");
    }

    #[test]
    fn cursor_move_sequence_same_column_is_empty() {
        assert!(cursor_move_sequence(4, 4).is_empty());
    }
}
