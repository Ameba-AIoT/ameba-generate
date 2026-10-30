/// Full-screen TUI for interactive option selection.
///
/// Flow:
///   1. Prompt for project name (simple readline, before entering raw mode)
///   2. Full-screen ratatui menu — navigate options, toggle with Space/Enter
///   3. Press `g` / `s` to generate, `q` / Esc to quit
use crate::template::{Category, GeneratorOption, Opt, Template};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::io::{self, Write};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub struct TuiResult {
    pub project_name: String,
    pub sdk_path: String,
    pub selected: Vec<String>,
}

/// Run the interactive TUI.  Returns `None` if the user pressed `q`/Esc.
///
/// `default_sdk` is pre-filled when the caller already has a `--sdk` value
/// (e.g. `ameba-generate --sdk /path/to/rust`); the user can still edit it.
pub fn run(tmpl: &Template, default_sdk: Option<&str>) -> Result<Option<TuiResult>> {
    // Step 1: prompt for project name and SDK path in normal terminal mode.
    let project_name = prompt_line("Project name: ")?;
    if project_name.is_empty() {
        return Ok(None);
    }

    // Show the pre-filled SDK path so the user knows it was picked up.
    let prompt = match default_sdk {
        Some(s) => format!("SDK path [{}]: ", s),
        None => "SDK path:     ".to_string(),
    };
    let sdk_input = prompt_line(&prompt)?;
    let sdk_path = if sdk_input.is_empty() {
        match default_sdk {
            Some(s) => s.to_string(),
            None => return Ok(None),
        }
    } else {
        sdk_input
    };

    // Step 2: full-screen option menu.
    let selected = run_menu(tmpl)?;
    match selected {
        Some(sel) => Ok(Some(TuiResult {
            project_name,
            sdk_path,
            selected: sel,
        })),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Name prompt
// ---------------------------------------------------------------------------

fn prompt_line(label: &str) -> Result<String> {
    print!("{}", label);
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

// ---------------------------------------------------------------------------
// Menu state
// ---------------------------------------------------------------------------

/// A flat list entry shown in the TUI list widget.
#[derive(Clone)]
struct MenuItem {
    /// Display depth (for indentation)
    depth: usize,
    /// Whether this is a category header (not selectable)
    is_category: bool,
    /// The option name (empty for category headers)
    name: String,
    display_name: String,
    help: String,
    selection_group: Option<String>,
    requires: Vec<String>,
}

fn flatten_options(options: &[GeneratorOption]) -> Vec<MenuItem> {
    let mut items = Vec::new();
    flatten_recursive(options, 0, &mut items);
    items
}

fn flatten_recursive(options: &[GeneratorOption], depth: usize, out: &mut Vec<MenuItem>) {
    for opt in options {
        match opt {
            GeneratorOption::Category(cat) => {
                out.push(category_item(cat, depth));
                flatten_recursive(&cat.options, depth + 1, out);
            }
            GeneratorOption::Option(o) => {
                out.push(option_item(o, depth));
            }
        }
    }
}

fn category_item(cat: &Category, depth: usize) -> MenuItem {
    MenuItem {
        depth,
        is_category: true,
        name: String::new(),
        display_name: cat.display_name.clone(),
        help: cat.help.clone(),
        selection_group: None,
        requires: vec![],
    }
}

fn option_item(o: &Opt, depth: usize) -> MenuItem {
    MenuItem {
        depth,
        is_category: false,
        name: o.name.clone(),
        display_name: o.display_name.clone(),
        help: o.help.clone(),
        selection_group: o.selection_group.clone(),
        requires: o.requires.clone(),
    }
}

// ---------------------------------------------------------------------------
// Full-screen menu
// ---------------------------------------------------------------------------

struct MenuApp {
    items: Vec<MenuItem>,
    list_state: ListState,
    selected: Vec<String>,
    status: String,
}

impl MenuApp {
    fn new(tmpl: &Template) -> Self {
        let items = flatten_options(&tmpl.options);
        let mut state = ListState::default();
        // Start cursor on first selectable item.
        let first = items.iter().position(|i| !i.is_category).unwrap_or(0);
        state.select(Some(first));
        Self {
            items,
            list_state: state,
            selected: Vec::new(),
            status: String::new(),
        }
    }

    fn cursor(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    fn move_up(&mut self) {
        let cur = self.cursor();
        // Find previous selectable item.
        for i in (0..cur).rev() {
            if !self.items[i].is_category {
                self.list_state.select(Some(i));
                return;
            }
        }
    }

    fn move_down(&mut self) {
        let cur = self.cursor();
        for i in (cur + 1)..self.items.len() {
            if !self.items[i].is_category {
                self.list_state.select(Some(i));
                return;
            }
        }
    }

    fn toggle(&mut self) {
        let cur = self.cursor();
        let item = &self.items[cur];
        if item.is_category || item.name.is_empty() {
            return;
        }
        let name = item.name.clone();
        let group = item.selection_group.clone();

        if self.selected.contains(&name) {
            self.selected.retain(|s| s != &name);
            self.cascade_deselect(&name);
        } else {
            // Deselect other items in the same selection_group.
            if let Some(ref g) = group {
                let to_remove: Vec<String> = self
                    .items
                    .iter()
                    .filter(|i| {
                        i.selection_group.as_deref() == Some(g.as_str())
                            && i.name != name
                            && self.selected.contains(&i.name)
                    })
                    .map(|i| i.name.clone())
                    .collect();
                for r in to_remove {
                    self.selected.retain(|s| s != &r);
                    // Also deselect anything that required that option.
                    self.cascade_deselect(&r);
                }
            }
            self.selected.push(name);
        }
        self.status.clear();
    }

    /// Remove `deselected` and anything that `requires` it.
    fn cascade_deselect(&mut self, deselected: &str) {
        let dependents: Vec<String> = self
            .items
            .iter()
            .filter(|i| i.requires.iter().any(|r| r == deselected))
            .map(|i| i.name.clone())
            .collect();
        for dep in dependents {
            if self.selected.contains(&dep) {
                self.selected.retain(|s| s != &dep);
                let dep_clone = dep.clone();
                self.cascade_deselect(&dep_clone);
            }
        }
    }

    /// Validate and return selected options, or set status message on error.
    fn validate(&mut self, tmpl: &Template) -> Option<Vec<String>> {
        let sel_refs: Vec<&str> = self.selected.iter().map(String::as_str).collect();
        match crate::main_validate(tmpl, &sel_refs) {
            Ok(()) => Some(self.selected.clone()),
            Err(e) => {
                self.status = format!("✗  {}", e);
                None
            }
        }
    }

    fn current_help(&self) -> &str {
        let cur = self.cursor();
        self.items.get(cur).map(|i| i.help.as_str()).unwrap_or("")
    }

    fn current_requires(&self) -> String {
        let cur = self.cursor();
        if let Some(item) = self.items.get(cur)
            && !item.requires.is_empty()
        {
            return format!("requires: {}", item.requires.join(", "));
        }
        String::new()
    }
}

fn run_menu(tmpl: &Template) -> Result<Option<Vec<String>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = MenuApp::new(tmpl);
    let result;

    loop {
        terminal.draw(|f| draw(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                KeyCode::Char(' ') | KeyCode::Enter => app.toggle(),
                KeyCode::Char('g') | KeyCode::Char('s') => {
                    if let Some(sel) = app.validate(tmpl) {
                        result = Some(sel);
                        break;
                    }
                }
                KeyCode::Char('q') | KeyCode::Esc => {
                    result = None;
                    break;
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn draw(f: &mut ratatui::Frame, app: &mut MenuApp) {
    let area = f.area();

    // Layout: title bar | list | help pane | status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Min(5),    // list
            Constraint::Length(4), // help
            Constraint::Length(3), // status / keys
        ])
        .split(area);

    // --- Title ---
    let title = Paragraph::new("ameba-generate — Select options")
        .block(Block::default().borders(Borders::ALL))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(title, chunks[0]);

    // --- Option list ---
    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth);
            if item.is_category {
                ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        format!("▸ {}", item.display_name),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
            } else {
                let checked = if app.selected.contains(&item.name) {
                    Span::styled("[✓] ", Style::default().fg(Color::Green))
                } else {
                    Span::styled("[ ] ", Style::default().fg(Color::DarkGray))
                };
                ListItem::new(Line::from(vec![
                    Span::raw(indent),
                    checked,
                    Span::raw(item.display_name.clone()),
                ]))
            }
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Options "))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("► ");
    f.render_stateful_widget(list, chunks[1], &mut app.list_state);

    // --- Help pane ---
    let help_text = {
        let h = app.current_help();
        let r = app.current_requires();
        let mut lines = vec![];
        if !h.is_empty() {
            lines.push(h.to_string());
        }
        if !r.is_empty() {
            lines.push(r);
        }
        if lines.is_empty() {
            " ".to_string()
        } else {
            lines.join("\n")
        }
    };
    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: true });
    f.render_widget(help, chunks[2]);

    // --- Status / key hints ---
    let status_text = if app.status.is_empty() {
        Line::from(vec![
            Span::styled(" ↑↓ ", Style::default().fg(Color::Cyan)),
            Span::raw("navigate  "),
            Span::styled(" Space ", Style::default().fg(Color::Cyan)),
            Span::raw("toggle  "),
            Span::styled(" g ", Style::default().fg(Color::Green)),
            Span::raw("generate  "),
            Span::styled(" q ", Style::default().fg(Color::Red)),
            Span::raw("quit"),
        ])
    } else {
        Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(Color::Red),
        ))
    };
    let status = Paragraph::new(status_text).block(Block::default().borders(Borders::ALL));
    f.render_widget(status, chunks[3]);
}

// ---------------------------------------------------------------------------
// Expose validate for MenuApp::validate() to call
// ---------------------------------------------------------------------------

// (defined in main.rs, re-exported via pub(crate) — see below)
