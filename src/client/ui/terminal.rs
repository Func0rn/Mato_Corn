use crate::client::app::{App, Focus};
use crate::terminal_provider::CursorShape;
use crate::theme::ThemeColors;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
    Frame,
};

pub(super) fn draw_terminal(f: &mut Frame, app: &mut App, area: Rect, t: &ThemeColors) {
    let active = app.focus == Focus::Content;
    let task = &app.offices[app.current_office].desks[app.selected()];
    let tab = task.active_tab_ref();

    let term_bg = t.bg();
    let (ix, iy, iw, ih) = if app.copy_mode {
        f.render_widget(Block::default().style(Style::default().bg(term_bg)), area);
        (area.x, area.y, area.width, area.height)
    } else {
        f.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_type(super::border_type(t, active))
                .title(Span::styled(
                    format!(
                        " {} ",
                        match app.terminal_titles.get(&tab.id) {
                            Some(term_title) if !term_title.is_empty() =>
                                format!("{} : {}", tab.name, term_title),
                            _ => tab.name.clone(),
                        }
                    ),
                    super::title_style(t, active),
                ))
                .border_style(super::border_style(t, active))
                .style(Style::default().bg(term_bg)),
            area,
        );
        (
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        )
    };
    let screen = tab.provider.get_screen(ih, iw);
    let screen_rows = (screen.lines.len() as u16).min(ih);
    // Copy mode: bottom-align so scrollback content is viewable from bottom.
    // Normal mode: also bottom-align when content is shorter than the display area.
    // This handles the Android keyboard resize race: `resize_all_ptys` fires
    // a fire-and-forget Resize while the sync GetScreen fallback may still see
    // the old (smaller) PTY size, returning fewer lines than `ih`. Without
    // bottom-alignment those lines render top-aligned with empty rows below,
    // leaving the cursor visually "stuck in the middle" until the push loop
    // delivers a correctly-sized full screen. Bottom-aligning keeps the cursor
    // pinned near the visual bottom regardless of transient size mismatches.
    let row_base = ih.saturating_sub(screen_rows);

    if screen.bell {
        app.pending_bell = true;
    }

    let buf = f.buffer_mut();
    let bg_style = Style::default().bg(term_bg);

    for row_idx in 0..ih {
        let src_row = if row_idx < row_base {
            None
        } else {
            Some((row_idx - row_base) as usize)
        };
        let by = iy + row_idx;
        if let Some(line) = src_row.and_then(|r| screen.lines.get(r)) {
            let mut bx = ix;
            let bx_end = ix + iw;
            for cell in &line.cells {
                if bx >= bx_end {
                    break;
                }
                if cell.display_width == 0 {
                    continue;
                }
                if let Some(buf_cell) = buf.cell_mut((bx, by)) {
                    // Build style with bitwise modifier accumulation
                    let mut style = Style::default();
                    if let Some(fg) = cell.fg {
                        style = style.fg(fg);
                    }
                    if let Some(bg) = cell.bg {
                        style = style.bg(bg);
                    }
                    let mut mods = Modifier::empty();
                    if cell.bold {
                        mods |= Modifier::BOLD;
                    }
                    if cell.italic {
                        mods |= Modifier::ITALIC;
                    }
                    if cell.underline {
                        mods |= Modifier::UNDERLINED;
                        if let Some(uc) = cell.underline_color {
                            style = style.underline_color(uc);
                        }
                    }
                    if cell.dim {
                        mods |= Modifier::DIM;
                    }
                    if cell.reverse {
                        mods |= Modifier::REVERSED;
                    }
                    if cell.strikethrough {
                        mods |= Modifier::CROSSED_OUT;
                    }
                    if cell.hidden {
                        mods |= Modifier::HIDDEN;
                    }
                    if !mods.is_empty() {
                        style = style.add_modifier(mods);
                    }
                    buf_cell.set_style(style);
                    if cell.ch == '\0' {
                        buf_cell.set_char(' ');
                    } else if let Some(ref zw) = cell.zerowidth {
                        let mut sym = cell.ch.to_string();
                        for &c in zw {
                            sym.push(c);
                        }
                        buf_cell.set_symbol(&sym);
                    } else {
                        buf_cell.set_char(cell.ch);
                    }
                    // Wide chars: reset following continuation cells
                    if cell.display_width > 1 {
                        for dx in 1..cell.display_width as u16 {
                            let cx = bx + dx;
                            if cx < bx_end {
                                if let Some(next_cell) = buf.cell_mut((cx, by)) {
                                    next_cell.reset();
                                }
                            }
                        }
                    }
                }
                bx += cell.display_width as u16;
            }
            // Pad remaining columns with terminal background
            while bx < bx_end {
                if let Some(buf_cell) = buf.cell_mut((bx, by)) {
                    buf_cell.set_char(' ');
                    buf_cell.set_style(bg_style);
                }
                bx += 1;
            }
        } else {
            // Empty row — fill with background
            for col in 0..iw {
                if let Some(buf_cell) = buf.cell_mut((ix + col, by)) {
                    buf_cell.set_char(' ');
                    buf_cell.set_style(bg_style);
                }
            }
        }
    }
    draw_scrollbar(
        buf,
        Rect {
            x: ix,
            y: iy,
            width: iw,
            height: ih,
        },
        screen.scroll_offset,
        screen.scrollback_len,
        t,
    );

    let (cr, cc) = screen.cursor;
    // Hardware cursor is always hidden (terminal.hide_cursor at startup).
    // We use a software cursor overlay rendered in the buffer instead.
    // For Hidden cursor shape (e.g. Claude Code), skip the overlay entirely —
    // the inner TUI app renders its own visual cursor via INVERSE text.
    if !app.copy_mode
        && ih > 0
        && iw > 0
        && screen_rows > 0
        && screen.cursor_shape != CursorShape::Hidden
    {
        let cursor_row = cr.min(screen_rows.saturating_sub(1));
        let cursor_col = cc.min(iw.saturating_sub(1));
        let cursor_y = iy + row_base + cursor_row;

        // Software cursor overlay: terminal cursor columns are already visual
        // display columns. Do not recalculate them from character widths, or
        // wide CJK cells can push the cursor one column too far to the right.
        let line = screen.lines.get(cursor_row as usize);
        let mut glyph = " ".to_string();
        let mut cursor_x = ix + cursor_col;
        let mut cursor_width = 1u16;
        let mut caret_style = Style::default()
            .bg(term_bg)
            .add_modifier(Modifier::REVERSED);
        if let Some(line) = line {
            let mut idx = cc as usize;
            if idx >= line.cells.len() && !line.cells.is_empty() {
                idx = line.cells.len() - 1;
            }
            let mut cell = line.cells.get(idx);
            if matches!(cell, Some(c) if c.ch == '\0') && idx > 0 {
                cursor_x = cursor_x.saturating_sub(1);
                cell = line.cells.get(idx - 1);
            }
            if let Some(cell) = cell {
                if cell.ch != '\0' {
                    glyph = cell.ch.to_string();
                }
                cursor_width = u16::from(cell.display_width.max(1)).min(2);
                if let Some(fg) = cell.fg {
                    caret_style = caret_style.fg(fg);
                }
                if let Some(bg) = cell.bg {
                    caret_style = caret_style.bg(bg);
                }
                if cell.bold {
                    caret_style = caret_style.add_modifier(Modifier::BOLD);
                }
                if cell.italic {
                    caret_style = caret_style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline {
                    caret_style = caret_style.add_modifier(Modifier::UNDERLINED);
                }
                caret_style = caret_style.add_modifier(Modifier::REVERSED);
            }
        }
        draw_cursor(buf, cursor_x, cursor_y, cursor_width, &glyph, caret_style, ix + iw);
    }
}

fn draw_cursor(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    glyph: &str,
    style: Style,
    x_end: u16,
) {
    if x >= x_end {
        return;
    }
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_style(style);
        cell.set_symbol(glyph);
    }
    if width > 1 && x + 1 < x_end {
        if let Some(cell) = buf.cell_mut((x + 1, y)) {
            cell.reset();
            cell.set_style(style);
        }
    }
}

fn draw_scrollbar(
    buf: &mut Buffer,
    area: Rect,
    scroll_offset: u16,
    scrollback_len: u16,
    t: &ThemeColors,
) {
    if area.width == 0 || area.height < 2 || scrollback_len == 0 {
        return;
    }

    let x = area.x + area.width - 1;
    let track_height = area.height;
    let visible = u32::from(area.height);
    let total = visible + u32::from(scrollback_len);
    let thumb_height = ((visible * visible) / total).max(1).min(visible) as u16;
    let movable = track_height.saturating_sub(thumb_height);
    let top_distance = u32::from(scrollback_len.saturating_sub(scroll_offset));
    let thumb_top = if scrollback_len == 0 {
        0
    } else {
        ((top_distance * u32::from(movable)) / u32::from(scrollback_len)) as u16
    };

    let track_style = if t.follow_terminal {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(t.border()).bg(t.bg())
    };
    let thumb_style = if t.follow_terminal {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(t.accent()).bg(t.bg())
    };

    for dy in 0..track_height {
        let in_thumb = dy >= thumb_top && dy < thumb_top + thumb_height;
        if let Some(cell) = buf.cell_mut((x, area.y + dy)) {
            cell.set_char(if in_thumb { '█' } else { '│' });
            cell.set_style(if in_thumb { thumb_style } else { track_style });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::draw_cursor;
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::{Modifier, Style},
    };

    #[test]
    fn wide_cursor_marks_leading_and_continuation_cells() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 1));
        let style = Style::default().add_modifier(Modifier::REVERSED);
        draw_cursor(&mut buf, 1, 0, 2, "中", style, 4);

        assert_eq!(buf.cell((1, 0)).unwrap().symbol(), "中");
        assert_eq!(buf.cell((2, 0)).unwrap().symbol(), " ");
        assert!(buf
            .cell((1, 0))
            .unwrap()
            .style()
            .add_modifier
            .contains(Modifier::REVERSED));
        assert!(buf
            .cell((2, 0))
            .unwrap()
            .style()
            .add_modifier
            .contains(Modifier::REVERSED));
    }
}
