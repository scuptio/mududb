//! Terminal UI helpers for rendering query results as an interactive table.

use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    backend::TestBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
};
use serde_json::Value;
use std::io::{self, IsTerminal, Stdout};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// A query result shaped for TUI table rendering.
pub struct QueryTable {
    /// Column names.
    pub columns: Vec<String>,
    /// Rows as rendered cell strings.
    pub rows: Vec<Vec<String>>,
    /// Number of affected rows reported by the server.
    pub affected_rows: u64,
    /// Optional server-side error message.
    pub error: Option<String>,
}

/// Try to extract a [`QueryTable`] from a command JSON response.
pub fn extract_query_table(value: &Value) -> Option<QueryTable> {
    let object = value.as_object()?;

    let columns = object.get("columns")?.as_array()?;
    let columns: Vec<String> = columns
        .iter()
        .map(|v| {
            v.as_str()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .collect();
    if columns.is_empty() {
        return None;
    }

    let rows = object.get("rows")?.as_array()?;
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            row.as_array()
                .map(|cells| {
                    cells
                        .iter()
                        .map(|c| {
                            c.as_str()
                                .map(ToOwned::to_owned)
                                .unwrap_or_else(|| c.to_string())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![row.to_string()])
        })
        .collect();

    let affected_rows = object
        .get("affected_rows")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let error = object
        .get("error")
        .and_then(|v| v.as_str().map(ToOwned::to_owned));

    Some(QueryTable {
        columns,
        rows,
        affected_rows,
        error,
    })
}

/// Run an interactive ratatui table for `table` and return when the user quits.
pub fn run_query_table(table: QueryTable) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(
            "tui requires a real TTY for stdin/stdout (keyboard input). If you're running under an IDE \"Debug Console\", switch to an integrated terminal or pass --no-table."
                .to_string(),
        );
    }
    let mut tui = Tui::enter().map_err(|e| format!("enter tui failed: {e}"))?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tui.run(table)));
    match result {
        Ok(inner) => inner,
        Err(_) => Err("tui crashed (panic)".to_string()),
    }
}

/// Render the query table to a string snapshot without touching the real terminal.
///
/// This is intended for integration tests to exercise the TUI rendering path.
pub fn render_query_table_snapshot(
    table: QueryTable,
    width: u16,
    height: u16,
) -> Result<String, String> {
    let backend = TestBackend::new(width, height);
    let mut terminal =
        Terminal::new(backend).map_err(|e| format!("init test terminal failed: {e}"))?;
    let mut state = QueryTableState::new(&table);
    terminal
        .draw(|f| draw_query_table(f, &table, &mut state))
        .map_err(|e| format!("tui draw failed: {e}"))?;

    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    Ok(out)
}

struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    _guard: TerminalGuard,
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
    }
}

impl Tui {
    fn enter() -> io::Result<Self> {
        let guard = TerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            _guard: guard,
        })
    }

    fn run(&mut self, table: QueryTable) -> Result<(), String> {
        let mut state = QueryTableState::new(&table);
        loop {
            self.terminal
                .draw(|f| draw_query_table(f, &table, &mut state))
                .map_err(|e| format!("tui draw failed: {e}"))?;

            if event::poll(Duration::from_millis(150)).map_err(|e| e.to_string())? {
                match event::read().map_err(|e| e.to_string())? {
                    Event::Key(key)
                        if key.kind == event::KeyEventKind::Press
                            && handle_key(&mut state, &table, key) =>
                    {
                        break;
                    }
                    Event::Resize(_, _) => {
                        // The next loop iteration redraws with the new size.
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

struct QueryTableState {
    selected_row: usize,
    selected_col: usize,
    row_offset: usize,
    table_state: TableState,
}

impl QueryTableState {
    fn new(table: &QueryTable) -> Self {
        let mut table_state = TableState::default();
        if !table.rows.is_empty() {
            table_state.select(Some(0));
        }
        Self {
            selected_row: 0,
            selected_col: 0,
            row_offset: 0,
            table_state,
        }
    }

    fn clamp_selection(&mut self, table: &QueryTable) {
        if table.rows.is_empty() {
            self.selected_row = 0;
            self.row_offset = 0;
            self.table_state.select(None);
            return;
        }
        self.selected_row = self.selected_row.min(table.rows.len() - 1);
        self.selected_col = self.selected_col.min(table.columns.len().saturating_sub(1));
        self.table_state.select(Some(self.selected_row));
    }
}

fn handle_key(state: &mut QueryTableState, table: &QueryTable, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
        KeyCode::Up if state.selected_row > 0 => {
            state.selected_row -= 1;
        }
        KeyCode::Down if state.selected_row + 1 < table.rows.len() => {
            state.selected_row += 1;
        }
        KeyCode::PageUp => {
            state.selected_row = state.selected_row.saturating_sub(20);
        }
        KeyCode::PageDown => {
            state.selected_row = (state.selected_row + 20).min(table.rows.len().saturating_sub(1));
        }
        KeyCode::Home | KeyCode::Char('g') => state.selected_row = 0,
        KeyCode::End | KeyCode::Char('G') if !table.rows.is_empty() => {
            state.selected_row = table.rows.len() - 1;
        }
        KeyCode::Left => state.selected_col = state.selected_col.saturating_sub(1),
        KeyCode::Right => {
            state.selected_col = (state.selected_col + 1).min(table.columns.len().saturating_sub(1))
        }
        _ => {}
    }

    state.clamp_selection(table);
    false
}

fn draw_query_table(f: &mut ratatui::Frame<'_>, table: &QueryTable, state: &mut QueryTableState) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .split(size);

    draw_table_widget(f, chunks[0], table, state);
    draw_detail_widget(f, chunks[1], table, state);
    draw_footer_widget(f, chunks[2], table, state);
}

fn draw_table_widget(
    f: &mut ratatui::Frame<'_>,
    area: Rect,
    table: &QueryTable,
    state: &mut QueryTableState,
) {
    let block_title = if let Some(err) = &table.error {
        format!("Query Result (error: {err})")
    } else {
        "Query Result".to_string()
    };

    let block = Block::default().borders(Borders::ALL).title(block_title);
    let inner = block.inner(area);

    let visible_height = inner.height.saturating_sub(2) as usize; // header + at least 1 row
    if state.selected_row < state.row_offset {
        state.row_offset = state.selected_row;
    } else if state.selected_row >= state.row_offset + visible_height.saturating_sub(1) {
        state.row_offset = state
            .selected_row
            .saturating_sub(visible_height.saturating_sub(1));
    }

    let widths = compute_column_widths(table, inner);

    let header = Row::new(
        table
            .columns
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let w = widths.get(i).copied().unwrap_or(10);
                Cell::from(truncate_to_width(name, w as usize)).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>(),
    );

    let rows = table
        .rows
        .iter()
        .enumerate()
        .skip(state.row_offset)
        .take(visible_height)
        .map(|(row_index, row)| {
            let is_selected = row_index == state.selected_row;
            let base_style = if row_index % 2 == 0 {
                Style::default()
                    .bg(Color::Rgb(245, 245, 245))
                    .fg(Color::Black)
            } else {
                Style::default().bg(Color::White).fg(Color::Black)
            };
            let style = if is_selected {
                base_style
                    .bg(Color::Rgb(255, 240, 200))
                    .add_modifier(Modifier::BOLD)
            } else {
                base_style
            };
            Row::new(
                table
                    .columns
                    .iter()
                    .enumerate()
                    .map(|(col_index, _)| {
                        let text = row.get(col_index).cloned().unwrap_or_default();
                        let w = widths.get(col_index).copied().unwrap_or(10);
                        let mut cell_style = style;
                        if is_selected && col_index == state.selected_col {
                            cell_style = cell_style
                                .bg(Color::Rgb(255, 220, 140))
                                .add_modifier(Modifier::BOLD);
                        }
                        Cell::from(truncate_to_width(&text, w as usize)).style(cell_style)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();

    let constraints = widths
        .iter()
        .map(|w| Constraint::Length(*w))
        .collect::<Vec<_>>();

    let widget = Table::new(rows, constraints)
        .header(header)
        .block(block)
        .column_spacing(1)
        .row_highlight_style(Style::default());

    f.render_stateful_widget(widget, area, &mut state.table_state);
}

fn draw_detail_widget(
    f: &mut ratatui::Frame<'_>,
    area: Rect,
    table: &QueryTable,
    state: &QueryTableState,
) {
    let block = Block::default().borders(Borders::ALL).title("Cell");
    let mut lines = Vec::new();
    if table.rows.is_empty() {
        lines.push(Line::from("No rows"));
    } else {
        let row_idx = state.selected_row;
        let col_idx = state.selected_col;
        let col_name = table.columns.get(col_idx).cloned().unwrap_or_default();
        let cell_value = table
            .rows
            .get(row_idx)
            .and_then(|r| r.get(col_idx))
            .cloned()
            .unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(
                format!("row {}/{}  ", row_idx + 1, table.rows.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("col "),
            Span::styled(
                format!("{}/{} ", col_idx + 1, table.columns.len()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("("),
            Span::styled(col_name, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(")"),
        ]));
        lines.push(Line::from(""));
        for line in cell_value.lines() {
            lines.push(Line::from(line.to_string()));
        }
        if lines.len() == 2 {
            lines.push(Line::from("(empty)"));
        }
    }

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn draw_footer_widget(
    f: &mut ratatui::Frame<'_>,
    area: Rect,
    table: &QueryTable,
    state: &QueryTableState,
) {
    let left = format!(
        "rows {}  cols {}  affected {}",
        table.rows.len(),
        table.columns.len(),
        table.affected_rows
    );
    let right = if table.rows.is_empty() {
        "q/Esc quit".to_string()
    } else {
        format!(
            "↑↓ PgUp/PgDn scroll  ←→ col  q/Esc quit   (row {}, col {})",
            state.selected_row + 1,
            state.selected_col + 1
        )
    };

    let line = Line::from(vec![
        Span::styled(left, Style::default().fg(Color::DarkGray)),
        Span::raw("    "),
        Span::styled(right, Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn compute_column_widths(table: &QueryTable, inner: Rect) -> Vec<u16> {
    let max_sample = 200usize;
    let mut natural: Vec<usize> = table
        .columns
        .iter()
        .map(|c| UnicodeWidthStr::width(c.as_str()).max(3))
        .collect();
    for row in table.rows.iter().take(max_sample) {
        for (i, cell) in row.iter().enumerate() {
            if i >= natural.len() {
                break;
            }
            let w = UnicodeWidthStr::width(cell.as_str());
            natural[i] = natural[i].max(w);
        }
    }

    // Borders take 2 columns; spacing takes (n-1) columns.
    let cols = table.columns.len();
    let available = inner
        .width
        .saturating_sub(1 + cols.saturating_sub(1) as u16) as usize;

    let mut widths = natural
        .into_iter()
        .map(|w| w.clamp(3, 60))
        .collect::<Vec<_>>();

    let mut total: usize = widths.iter().sum();
    if total > available && total > 0 {
        // Shrink widest columns first.
        let mut idxs = (0..widths.len()).collect::<Vec<_>>();
        idxs.sort_by_key(|&i| std::cmp::Reverse(widths[i]));
        for i in idxs {
            while total > available && widths[i] > 3 {
                widths[i] -= 1;
                total -= 1;
            }
            if total <= available {
                break;
            }
        }
    }

    widths.into_iter().map(|w| w as u16).collect()
}

fn truncate_to_width(s: &str, width: usize) -> String {
    if width <= 1 {
        return "…".to_string();
    }
    if UnicodeWidthStr::width(s) <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if w + cw + 1 > width {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}
