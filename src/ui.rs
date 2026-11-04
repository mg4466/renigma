//! All drawing. Reads [`App`] state and turns it into ratatui widgets.

use crate::app::{App, LAMP_ROWS, Screen, TABS};
use crate::enigma::{REFLECTORS, idx_to_letter};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Clear, Padding, Paragraph, Wrap};

/// Trim a string to `width` columns, marking truncation with an ellipsis.
fn fit(text: &str, width: usize) -> String {
    if text.chars().count() <= width || width == 0 {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

fn base(t: Theme) -> Style {
    Style::new().bg(t.bg).fg(t.fg)
}

fn panel(t: Theme, title: impl Into<String>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(t.border))
        .title(Span::styled(
            format!(" {} ", title.into()),
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
        .style(base(t))
}

fn focused_panel(t: Theme, title: impl Into<String>) -> Block<'static> {
    panel(t, title).border_style(Style::new().fg(t.border_focus))
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let t = app.theme;
    frame.render_widget(Block::new().style(base(t)), frame.area());
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, app, header);
    match app.screen {
        Screen::Encrypt => draw_encrypt(frame, app, body),
        Screen::Config => draw_config(frame, app, body),
        Screen::Help => {
            if app.under == Screen::Config {
                draw_config(frame, app, body);
            } else {
                draw_encrypt(frame, app, body);
            }
            draw_help(frame, app);
        }
        Screen::RotorForm => {
            draw_config(frame, app, body);
            draw_form(frame, app);
        }
    }
    draw_footer(frame, app, footer);
}

// ------------------------------------------------------------------ header

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let mut spans = vec![
        Span::styled(
            " RENIGMA ",
            Style::new()
                .fg(t.bg)
                .bg(t.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    for (i, label) in ["ENCRYPT", "CONFIGURE"].iter().enumerate() {
        let active = (i == 0 && matches!(app.screen, Screen::Encrypt | Screen::Help))
            || (i == 1 && matches!(app.screen, Screen::Config | Screen::RotorForm));
        let style = if active {
            Style::new()
                .fg(t.sel_fg)
                .bg(t.sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.dim)
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!("{}-rotor", app.machine.rotors.len()),
        Style::new().fg(t.dim),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(base(t)), area);

    let right = Line::from(vec![
        Span::styled(app.theme_name(), Style::new().fg(t.accent_alt)),
        Span::raw("  "),
        Span::styled(
            if app.dirty {
                "● unsaved"
            } else {
                "● saved"
            },
            Style::new().fg(if app.dirty { t.warn } else { t.good }),
        ),
        Span::raw(" "),
    ]);
    frame.render_widget(
        Paragraph::new(right)
            .alignment(Alignment::Right)
            .style(base(t)),
        area,
    );
}

// ----------------------------------------------------------------- encrypt

fn draw_encrypt(frame: &mut Frame, app: &mut App, area: Rect) {
    let [refl, rotors, mid, msg] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(8),
        Constraint::Length(6),
        Constraint::Min(5),
    ])
    .areas(area);
    let [plug, lamp] =
        Layout::horizontal([Constraint::Percentage(56), Constraint::Percentage(44)]).areas(mid);

    draw_reflector(frame, app, refl);
    draw_rotors(frame, app, rotors);
    draw_plugboard(frame, app, plug);
    draw_lampboard(frame, app, lamp);
    draw_message(frame, app, msg);
}

fn draw_reflector(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let b = panel(t, format!("Reflector {}", app.machine.reflector.name));
    let inner = b.inner(area);
    frame.render_widget(b, area);

    let thumb = if app.machine.reflector.is_thin() {
        "  ·  thin (M4)"
    } else {
        ""
    };
    let line = Line::from(vec![
        Span::styled(
            app.machine.reflector.name.to_string(),
            Style::new().fg(t.accent_alt).add_modifier(Modifier::BOLD),
        ),
        Span::styled(thumb, Style::new().fg(t.dim)),
        Span::styled("   ", Style::new().fg(t.dim)),
        Span::styled(
            app.machine.reflector.wiring_string(),
            Style::new().fg(t.dim),
        ),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(Paragraph::new(line).style(base(t)), inner);
}

fn draw_rotors(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let n = app.machine.rotors.len();
    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    let cols = Layout::horizontal(constraints).split(area);
    let armed = app.machine.armed();

    for i in 0..n {
        let rotor = &app.machine.rotors[i];
        let flashing = app.flashing(i);
        let is_armed = armed.get(i).copied().unwrap_or(false);
        let border = if flashing {
            t.lamp
        } else if is_armed {
            t.accent_alt
        } else {
            t.border
        };
        let role = if n == 4 && i == 0 {
            "greek"
        } else if i == n - 1 {
            "fast"
        } else {
            "middle"
        };
        let b = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(border))
            .title(Span::styled(
                format!(" {} · {} ", i + 1, rotor.name()),
                Style::new()
                    .fg(if flashing { t.lamp } else { t.accent })
                    .add_modifier(Modifier::BOLD),
            ))
            .title_alignment(Alignment::Center)
            .style(base(t));
        let inner = b.inner(cols[i]);
        frame.render_widget(b, cols[i]);

        let window_style = if flashing {
            Style::new()
                .fg(t.bg)
                .bg(t.lamp)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.window).add_modifier(Modifier::BOLD)
        };
        let frame_style = Style::new().fg(if flashing { t.lamp } else { t.border });
        let letter = rotor.window_letter();

        let lines = vec![
            Line::from(vec![
                Span::styled("ring ", Style::new().fg(t.dim)),
                Span::styled(
                    rotor.ring_letter().to_string(),
                    Style::new().fg(t.accent_alt).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  notch ", Style::new().fg(t.dim)),
                Span::styled(rotor.spec.notch_display(), Style::new().fg(t.dim)),
            ])
            .alignment(Alignment::Center),
            Line::from(Span::styled("┌─────┐", frame_style)).alignment(Alignment::Center),
            Line::from(vec![
                Span::styled("│", frame_style),
                Span::styled(format!("  {letter}  "), window_style),
                Span::styled("│", frame_style),
            ])
            .alignment(Alignment::Center),
            Line::from(Span::styled("└─────┘", frame_style)).alignment(Alignment::Center),
            Line::from(vec![
                Span::styled(role, Style::new().fg(t.dim)),
                Span::styled(" · ", Style::new().fg(t.dim)),
                Span::styled(
                    if is_armed { "steps" } else { "holds" },
                    Style::new().fg(if is_armed { t.accent_alt } else { t.dim }),
                ),
            ])
            .alignment(Alignment::Center),
            Line::from(Span::styled(
                fit(&rotor.spec.wiring, inner.width as usize),
                Style::new().fg(t.dim),
            ))
            .alignment(Alignment::Center),
        ];
        frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
    }
}

fn pair_color(t: Theme, index: usize) -> ratatui::style::Color {
    [t.accent, t.accent_alt, t.good, t.warn, t.bad, t.lamp][index % 6]
}

fn draw_plugboard(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let pairs = app.cfg.plug_pairs();
    let max_h = area.height;
    let b = panel(t, format!("Plugboard · {} pairs", pairs.len()));
    let inner = b.inner(area);
    frame.render_widget(b, area);

    let mut lines = Vec::new();
    let mut row_spans: Vec<Span> = Vec::new();
    for i in 0..26usize {
        let letter = idx_to_letter(i as u8);
        let style = if let Some(p) = pairs
            .iter()
            .position(|(a, b)| *a == i as u8 || *b == i as u8)
        {
            Style::new()
                .fg(pair_color(t, p))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.dim)
        };
        row_spans.push(Span::styled(format!("{letter} "), style));
        if i == 12 {
            lines.push(Line::from(std::mem::take(&mut row_spans)));
        }
    }
    lines.push(Line::from(row_spans));
    lines.push(Line::from(""));

    let pair_text = if pairs.is_empty() {
        "no plugs · signal goes straight through".to_string()
    } else {
        pairs
            .iter()
            .map(|(a, b)| format!("{}↔{}", idx_to_letter(*a), idx_to_letter(*b)))
            .collect::<Vec<_>>()
            .join("  ")
    };
    lines.push(
        Line::from(Span::styled(pair_text, Style::new().fg(t.fg))).alignment(Alignment::Center),
    );

    let visible: Vec<Line> = lines
        .into_iter()
        .take(max_h.saturating_sub(1) as usize)
        .collect();
    frame.render_widget(Paragraph::new(visible).style(base(t)), inner);
}

fn draw_lampboard(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let b = panel(t, "Lampboard");
    let inner = b.inner(area);
    frame.render_widget(b, area);

    let lit = app.last_pair.map(|(_, out)| out);
    let mut lines = Vec::new();
    let pair_line = match app.last_pair {
        Some((input, out)) => Line::from(vec![
            Span::styled(
                input.to_string(),
                Style::new().fg(t.dim).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" → ", Style::new().fg(t.dim)),
            Span::styled(
                out.to_string(),
                Style::new()
                    .fg(t.bg)
                    .bg(t.lamp)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        None => Line::from(Span::styled("press a letter", Style::new().fg(t.dim))),
    };
    lines.push(pair_line.alignment(Alignment::Center));

    for (row, letters) in LAMP_ROWS.iter().enumerate() {
        let pad = if row == 1 { " " } else { "" };
        let mut spans = vec![Span::raw(pad)];
        for ch in letters.chars() {
            let style = if Some(ch) == lit {
                Style::new()
                    .fg(t.bg)
                    .bg(t.lamp)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.fg)
            };
            spans.push(Span::styled(format!(" {ch} "), style));
        }
        lines.push(Line::from(spans).alignment(Alignment::Center));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(base(t)),
        inner,
    );
}

pub fn group_five(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(5)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn draw_message(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let b = panel(t, format!("Message · {} letters", app.output.len()));
    let inner = b.inner(area);
    frame.render_widget(b, area);

    let body = if app.output.is_empty() {
        Text::from(vec![
            Line::from(Span::styled(
                "typing encrypts and steps the rotors",
                Style::new().fg(t.dim),
            )),
            Line::from(Span::styled(
                "the machine is reciprocal: same settings decrypt",
                Style::new().fg(t.dim),
            )),
        ])
    } else {
        Text::from(Line::from(Span::styled(
            group_five(&app.output),
            Style::new().fg(t.fg).add_modifier(Modifier::BOLD),
        )))
    };
    frame.render_widget(
        Paragraph::new(body)
            .style(base(t))
            .wrap(Wrap { trim: false })
            .block(Block::new().padding(Padding::new(1, 1, 0, 0))),
        inner,
    );
}

// ------------------------------------------------------------------ config

fn draw_config(frame: &mut Frame, app: &mut App, area: Rect) {
    let t = app.theme;
    let [tabs, content, preview] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(4),
    ])
    .areas(area);

    // Section tabs.
    let mut spans = vec![Span::raw(" ")];
    for (i, name) in TABS.iter().enumerate() {
        let active = i == app.tab;
        let style = if active {
            Style::new()
                .fg(t.sel_fg)
                .bg(t.sel_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.dim)
        };
        spans.push(Span::styled(format!(" {} {} ", i + 1, name), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).style(base(t)), tabs);

    let [list_area, detail_area] =
        Layout::horizontal([Constraint::Percentage(46), Constraint::Percentage(54)]).areas(content);

    match app.tab {
        0 => draw_machine_tab(frame, app, list_area, detail_area),
        1 => draw_rotor_tab(frame, app, list_area, detail_area),
        2 => draw_reflector_tab(frame, app, list_area, detail_area),
        3 => draw_plug_tab(frame, app, list_area, detail_area),
        4 => draw_theme_tab(frame, app, list_area, detail_area),
        5 => draw_custom_tab(frame, app, list_area, detail_area),
        _ => {}
    }

    draw_preview(frame, app, preview);
}

fn selected_line(
    t: Theme,
    text: impl Into<String>,
    selected: bool,
    right: Option<String>,
) -> Line<'static> {
    let style = if selected {
        Style::new().fg(t.sel_fg).bg(t.sel_bg)
    } else {
        Style::new().fg(t.fg)
    };
    let mut spans = vec![
        Span::styled(if selected { " ▸ " } else { "   " }, style),
        Span::styled(text.into(), style),
    ];
    if let Some(right) = right {
        let pad = 46usize;
        let _ = pad;
        spans.push(Span::styled(format!("  {right}"), style.fg(t.dim)));
    }
    Line::from(spans)
}

fn draw_machine_tab(frame: &mut Frame, app: &App, list_area: Rect, detail: Rect) {
    let t = app.theme;
    let b = focused_panel(t, "Machine");
    let inner = b.inner(list_area);
    frame.render_widget(b, list_area);

    let count = app.cfg.rotor_count();
    let mut lines = Vec::new();
    lines.push(selected_line(
        t,
        "Rotors",
        app.row == 0,
        Some(format!("◀ {count} ▶")),
    ));
    for slot in 0..count {
        let name = app.cfg.rotors.order.get(slot).cloned().unwrap_or_default();
        let role = if count == 4 && slot == 0 {
            "greek"
        } else if slot == count - 1 {
            "fast"
        } else {
            "middle"
        };
        lines.push(selected_line(
            t,
            format!("Slot {} · {role}", slot + 1),
            app.row == slot + 1,
            Some(format!("◀ {name} ▶")),
        ));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Rotors I–VIII, Beta and Gamma.",
        Style::new().fg(t.dim),
    )));
    lines.push(Line::from(Span::styled(
        "Beta/Gamma: M4 leftmost slot only.",
        Style::new().fg(t.dim),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);

    // Detail: the selected rotor's wiring.
    let slot = if app.row == 0 { 0 } else { app.row - 1 };
    let name = app.cfg.rotors.order.get(slot).cloned().unwrap_or_default();
    let b = panel(t, format!("Rotor {name}"));
    let inner = b.inner(detail);
    frame.render_widget(b, detail);
    if let Some(spec) = app.cfg.find_rotor(&name) {
        let lines = vec![
            Line::from(Span::styled(
                format!("wiring  {}", spec.wiring),
                Style::new().fg(t.accent_alt),
            )),
            Line::from(Span::styled(
                format!("notch   {}", spec.notch_display()),
                Style::new().fg(t.fg),
            )),
            Line::from(""),
            Line::from(Span::styled(
                if let Some((builtin_name, _, _)) = crate::enigma::BUILTIN_ROTORS
                    .iter()
                    .find(|(n, _, _)| *n == spec.name)
                {
                    format!("historical rotor {builtin_name}")
                } else {
                    "custom rotor".to_string()
                },
                Style::new().fg(t.dim),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "← → change the rotor in the selected slot",
                Style::new().fg(t.dim),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
    }
}

fn draw_rotor_tab(frame: &mut Frame, app: &App, list_area: Rect, detail: Rect) {
    let t = app.theme;
    let b = focused_panel(t, "Windows & rings");
    let inner = b.inner(list_area);
    frame.render_widget(b, list_area);

    let mut lines = vec![Line::from(vec![Span::styled(
        "   #  rotor   window   ring",
        Style::new().fg(t.dim),
    )])];
    for (i, name) in app.cfg.rotors.order.iter().enumerate() {
        let window = app
            .machine
            .rotors
            .get(i)
            .map(|r| r.window_letter())
            .unwrap_or('A');
        let ring = app
            .cfg
            .rotors
            .rings
            .get(i)
            .cloned()
            .unwrap_or_else(|| "A".into());
        let notch = app
            .machine
            .rotors
            .get(i)
            .map(|r| r.at_notch())
            .unwrap_or(false);
        let style = if app.row == i {
            Style::new().fg(t.sel_fg).bg(t.sel_bg)
        } else {
            Style::new().fg(t.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{}  ", if app.row == i { "▸" } else { " " }), style),
            Span::styled(format!("{:<2} {:<6}", i + 1, name), style),
            Span::styled(
                format!("   {}", window),
                if app.row == i {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    Style::new().fg(t.window).add_modifier(Modifier::BOLD)
                },
            ),
            Span::styled(
                format!("       {}", ring),
                if app.row == i {
                    style
                } else {
                    Style::new().fg(t.accent_alt)
                },
            ),
            Span::styled(
                if notch { "  ● notch" } else { "" },
                Style::new().fg(t.warn),
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "◀ ▶ window    [ ] ring    a–z set window",
        Style::new().fg(t.dim),
    )));
    lines.push(Line::from(Span::styled(
        "Ring changes shift the internal alphabet.",
        Style::new().fg(t.dim),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);

    // Detail: rotor window preview with the notch highlighted.
    let b = panel(t, "Heads up");
    let inner = b.inner(detail);
    frame.render_widget(b, detail);
    let lines = vec![
        Line::from(Span::styled(
            "Window letter is what the operator reads;",
            Style::new().fg(t.fg),
        )),
        Line::from(Span::styled(
            "the ring setting rotates the wiring under it.",
            Style::new().fg(t.fg),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Changing a window does not clear the message.",
            Style::new().fg(t.dim),
        )),
        Line::from(Span::styled(
            "Turning a ring does the same to the substitution.",
            Style::new().fg(t.dim),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
}

fn draw_reflector_tab(frame: &mut Frame, app: &App, list_area: Rect, detail: Rect) {
    let t = app.theme;
    let b = focused_panel(t, "Reflector");
    let inner = b.inner(list_area);
    frame.render_widget(b, list_area);

    let mut lines = Vec::new();
    for (i, (name, _)) in REFLECTORS.iter().enumerate() {
        let thin = name.ends_with("-Thin");
        let enabled = thin == (app.cfg.rotor_count() == 4);
        let mut style = if app.row == i {
            Style::new().fg(t.sel_fg).bg(t.sel_bg)
        } else {
            Style::new().fg(t.fg)
        };
        if !enabled {
            style = style.fg(if app.row == i { t.warn } else { t.dim });
        }
        let marker = if *name == app.cfg.reflector {
            "●"
        } else if enabled {
            "○"
        } else {
            "·"
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", if app.row == i { "▸" } else { " " }), style),
            Span::styled(format!("{marker} {name:<8}"), style),
            Span::styled(if thin { "M4 only" } else { "3-rotor" }, style.fg(t.dim)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "ENTER or ← → applies a reflector.",
        Style::new().fg(t.dim),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);

    let name = app.selected_reflector();
    let b = panel(t, format!("Reflector {name}"));
    let inner = b.inner(detail);
    frame.render_widget(b, detail);
    if let Some((_, wiring)) = REFLECTORS.iter().find(|(n, _)| *n == name) {
        let mut lines = vec![
            Line::from(Span::styled(
                format!("wiring  {wiring}"),
                Style::new().fg(t.accent_alt),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "The reflector is a reciprocal plugboard: it sends",
                Style::new().fg(t.fg),
            )),
            Line::from(Span::styled(
                "the signal back through the rotors in reverse.",
                Style::new().fg(t.fg),
            )),
        ];
        if name.ends_with("-Thin") {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Thin reflectors pair with the Beta/Gamma rotor;",
                Style::new().fg(t.dim),
            )));
            lines.push(Line::from(Span::styled(
                "B-Thin is Beta · B · Beta⁻¹.",
                Style::new().fg(t.dim),
            )));
        }
        frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
    }
}

fn draw_plug_tab(frame: &mut Frame, app: &App, list_area: Rect, detail: Rect) {
    let t = app.theme;
    let b = focused_panel(t, "Plugboard");
    let inner = b.inner(list_area);
    frame.render_widget(b, list_area);

    let pairs = app.cfg.plug_pairs();
    let mut lines = Vec::new();
    for row in 0..2 {
        let mut spans = vec![Span::raw(" ")];
        for col in 0..13 {
            let idx = row * 13 + col;
            let letter = idx_to_letter(idx as u8);
            let cursor = app.plug_cursor == idx;
            let pending = app.plug_pending == Some(idx as u8);
            let pair = pairs
                .iter()
                .position(|(a, b)| *a == idx as u8 || *b == idx as u8);
            let mut style = if let Some(p) = pair {
                Style::new()
                    .fg(pair_color(t, p))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(t.dim)
            };
            if pending {
                style = Style::new().fg(t.sel_fg).bg(t.accent);
            } else if cursor {
                style = style.add_modifier(Modifier::REVERSED);
            }
            spans.push(Span::styled(format!(" {letter} "), style));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        if let Some(p) = app.plug_pending {
            format!("now pick the partner for {}", idx_to_letter(p))
        } else if let Some((a, b)) = pairs
            .iter()
            .find(|(a, b)| *a == app.plug_cursor as u8 || *b == app.plug_cursor as u8)
        {
            format!(
                "{} is plugged to {} — ENTER removes it",
                idx_to_letter(*a),
                idx_to_letter(*b)
            )
        } else {
            "ENTER picks a letter, then its partner".to_string()
        },
        Style::new().fg(t.accent_alt),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);

    let b = panel(t, format!("Plugs · {}/13", pairs.len()));
    let inner = b.inner(detail);
    frame.render_widget(b, detail);
    let mut lines = Vec::new();
    if pairs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No plugs installed.",
            Style::new().fg(t.dim),
        )));
    } else {
        for (i, (a, b)) in pairs.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {:>2}. ", i + 1), Style::new().fg(t.dim)),
                Span::styled(
                    format!("{} ↔ {}", idx_to_letter(*a), idx_to_letter(*b)),
                    Style::new()
                        .fg(pair_color(t, i))
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "x clears the whole board",
        Style::new().fg(t.dim),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
}

fn draw_theme_tab(frame: &mut Frame, app: &App, list_area: Rect, detail: Rect) {
    let t = app.theme;
    let b = focused_panel(t, "Theme");
    let inner = b.inner(list_area);
    frame.render_widget(b, list_area);

    let mut lines = Vec::new();
    for (i, theme) in crate::theme::THEMES.iter().enumerate() {
        let selected = app.row == i;
        let style = if selected {
            Style::new().fg(t.sel_fg).bg(t.sel_bg)
        } else {
            Style::new().fg(t.fg)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", if selected { "▸" } else { " " }), style),
            Span::styled(
                if *theme.name == app.cfg.theme {
                    "● "
                } else {
                    "○ "
                },
                style.fg(t.accent),
            ),
            Span::styled(format!("{:<20}", theme.label), style),
            Span::styled("███", Style::new().fg(theme.accent)),
            Span::styled("██", Style::new().fg(theme.accent_alt)),
            Span::styled("██", Style::new().fg(theme.lamp)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Themes apply immediately and recolor the whole UI.",
        Style::new().fg(t.dim),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);

    let b = panel(t, app.theme.label.to_string());
    let inner = b.inner(detail);
    frame.render_widget(b, detail);
    let lines = vec![
        Line::from(Span::styled(
            "Borders, text, highlights, lamps and the rotor",
            Style::new().fg(t.fg),
        )),
        Line::from(Span::styled(
            "windows all come from the active theme.",
            Style::new().fg(t.fg),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("accent  ", Style::new().fg(t.dim)),
            Span::styled("██████", Style::new().fg(t.accent)),
            Span::raw("  "),
            Span::styled("alt  ", Style::new().fg(t.dim)),
            Span::styled("██████", Style::new().fg(t.accent_alt)),
            Span::raw("  "),
            Span::styled("lamp  ", Style::new().fg(t.dim)),
            Span::styled("██████", Style::new().fg(t.lamp)),
        ]),
        Line::from(vec![
            Span::styled("good  ", Style::new().fg(t.dim)),
            Span::styled("██████", Style::new().fg(t.good)),
            Span::raw("  "),
            Span::styled("warn  ", Style::new().fg(t.dim)),
            Span::styled("██████", Style::new().fg(t.warn)),
            Span::raw("  "),
            Span::styled("bad  ", Style::new().fg(t.dim)),
            Span::styled("██████", Style::new().fg(t.bad)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
}

fn draw_custom_tab(frame: &mut Frame, app: &App, list_area: Rect, detail: Rect) {
    let t = app.theme;
    let b = focused_panel(
        t,
        format!("Custom rotors · {}", app.cfg.custom_rotors.len()),
    );
    let inner = b.inner(list_area);
    frame.render_widget(b, list_area);

    let mut lines = Vec::new();
    if app.cfg.custom_rotors.is_empty() {
        lines.push(Line::from(Span::styled(
            "No custom rotors yet. Press n to build one:",
            Style::new().fg(t.dim),
        )));
        lines.push(Line::from(Span::styled(
            "name, a 26-letter wiring, and notch letters.",
            Style::new().fg(t.dim),
        )));
    }
    for (i, spec) in app.cfg.custom_rotors.iter().enumerate() {
        let selected = app.row == i;
        let style = if selected {
            Style::new().fg(t.sel_fg).bg(t.sel_bg)
        } else {
            Style::new().fg(t.fg)
        };
        let used = app.cfg.rotors.order.contains(&spec.name);
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", if selected { "▸" } else { " " }), style),
            Span::styled(format!("{:<14}", spec.name), style),
            Span::styled(if used { "in machine" } else { "" }, style.fg(t.good)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "n new    e edit    d delete    ENTER put in slot",
        Style::new().fg(t.dim),
    )));
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);

    let b = panel(t, "Definition");
    let inner = b.inner(detail);
    frame.render_widget(b, detail);
    let lines = if let Some(spec) = app.cfg.custom_rotors.get(app.row) {
        vec![
            Line::from(Span::styled(
                format!("name    {}", spec.name),
                Style::new().fg(t.fg),
            )),
            Line::from(Span::styled(
                format!("wiring  {}", spec.wiring),
                Style::new().fg(t.accent_alt),
            )),
            Line::from(Span::styled(
                format!("notches {}", spec.notch_display()),
                Style::new().fg(t.fg),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Saved to the config file under [[custom_rotors]].",
                Style::new().fg(t.dim),
            )),
            Line::from(Span::styled(
                "Enter places it in the slot chosen on the Machine tab.",
                Style::new().fg(t.dim),
            )),
        ]
    } else {
        vec![
            Line::from(Span::styled(
                "Custom rotors let you invent wirings and notches.",
                Style::new().fg(t.fg),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Wiring must use each letter A–Z exactly once.",
                Style::new().fg(t.dim),
            )),
            Line::from(Span::styled(
                "Notches are 0–2 letters where the rotor carries.",
                Style::new().fg(t.dim),
            )),
        ]
    };
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let b = panel(t, "Machine state");
    let inner = b.inner(area);
    frame.render_widget(b, area);
    let mut spans = Vec::new();
    for (i, rotor) in app.machine.rotors.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(t.dim)));
        }
        spans.push(Span::styled(
            rotor.name().to_string(),
            Style::new().fg(t.accent),
        ));
        spans.push(Span::styled(
            format!(" {}", rotor.window_letter()),
            Style::new().fg(t.window).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled("   ▸   ", Style::new().fg(t.dim)));
    spans.push(Span::styled(
        format!("Reflector {}", app.machine.reflector.name),
        Style::new().fg(t.accent_alt),
    ));
    spans.push(Span::styled(
        format!("   ·   {} plugs", app.cfg.plug_pairs().len()),
        Style::new().fg(t.fg),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)).style(base(t)), inner);
}

// ------------------------------------------------------------------ footer

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let t = app.theme;
    let hint = match app.screen {
        Screen::Encrypt => {
            "A–Z encrypt   ⌫ undo   TAB configure   ? help   ^R reset   ^S save   ^Q quit"
        }
        Screen::Config => {
            "TAB/1–6 section   ↑↓ select   ←→ change   ENTER apply   n new rotor   ^S save   ESC encrypt"
        }
        Screen::Help => "any key closes help",
        Screen::RotorForm => "TAB next field   ENTER save   ESC cancel",
    };
    let line = if let Some((msg, _)) = &app.toast {
        Line::from(Span::styled(
            format!(" {msg} "),
            Style::new()
                .fg(t.bg)
                .bg(t.warn)
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled(format!(" {hint}"), Style::new().fg(t.dim)))
    };
    frame.render_widget(Paragraph::new(line).style(base(t)), area);
}

// ------------------------------------------------------------------ overlays

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn draw_help(frame: &mut Frame, app: &App) {
    let t = app.theme;
    let area = centered(frame.area(), 76, 23);
    frame.render_widget(Clear, area);
    let b = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::new().fg(t.accent))
        .title(Span::styled(
            " Renigma help ",
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
        .style(base(t))
        .padding(Padding::new(2, 2, 0, 0));
    let inner = b.inner(area);
    frame.render_widget(b, area);

    let key = |k: &'static str, d: &'static str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {k:<14}"), Style::new().fg(t.accent_alt)),
            Span::styled(d, Style::new().fg(t.fg)),
        ])
    };
    let head = |s: &'static str| -> Line<'static> {
        Line::from(Span::styled(
            s,
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
    };
    let lines = vec![
        head("Encryption"),
        key("A – Z", "encrypt a letter: steps rotors, lights the lamp"),
        key("BACKSPACE", "undo the last letter and rewind the rotors"),
        key("CTRL-R", "reset the rotors to the configured start"),
        key("TAB / F2", "open the configuration screen"),
        Line::from(""),
        head("Configuration"),
        key("TAB, 1–6", "switch section"),
        key("↑ ↓ / k j", "select a row"),
        key("← → / h l", "change the selected value"),
        key("ENTER", "apply / connect a plug"),
        key("n / e / d", "new / edit / delete a custom rotor"),
        key("[  ]", "ring setting down / up"),
        key("x", "clear the plugboard (plugboard tab)"),
        key("SHIFT-R", "reset all machine settings"),
        Line::from(""),
        head("Anywhere"),
        key("CTRL-S", "save the config file now"),
        key("CTRL-Q / ESC", "quit (settings are saved on exit)"),
        Line::from(""),
        Line::from(Span::styled(
            "  The machine is reciprocal: type ciphertext back with the same",
            Style::new().fg(t.dim),
        )),
        Line::from(Span::styled(
            "  settings to recover the plaintext.",
            Style::new().fg(t.dim),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  press any key to close",
            Style::new().fg(t.dim).add_modifier(Modifier::ITALIC),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
}

fn draw_form(frame: &mut Frame, app: &App) {
    let t = app.theme;
    let Some(form) = app.form.as_ref() else {
        return;
    };
    let area = centered(frame.area(), 76, 10);
    frame.render_widget(Clear, area);
    let title = if form.editing.is_some() {
        "Edit custom rotor"
    } else {
        "New custom rotor"
    };
    let b = Block::bordered()
        .border_type(BorderType::Thick)
        .border_style(Style::new().fg(t.accent))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD),
        ))
        .style(base(t))
        .padding(Padding::new(2, 2, 0, 0));
    let inner = b.inner(area);
    frame.render_widget(b, area);

    let mut lines = Vec::new();
    for (i, (label, value)) in [
        ("Name", &form.name),
        ("Wiring", &form.wiring),
        ("Notches", &form.notches),
    ]
    .iter()
    .enumerate()
    {
        let selected = form.field == i;
        let marker = if selected { "▸" } else { " " };
        let style = if selected {
            Style::new().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(t.dim)
        };
        let padded = format!("{label:<8}");
        lines.push(Line::from(vec![
            Span::styled(format!("  {marker} "), style),
            Span::styled(padded, style),
            Span::styled(
                format!("{value}{}", if selected { "_" } else { "" }),
                Style::new().fg(if selected { t.sel_fg } else { t.fg }),
            ),
        ]));
        if i == 1 {
            lines.push(Line::from(Span::styled(
                "            26 letters, each used once",
                Style::new().fg(t.dim),
            )));
        }
    }
    lines.push(Line::from(""));
    if let Some(err) = &form.error {
        lines.push(Line::from(Span::styled(
            format!(" ✗ {err}"),
            Style::new().fg(t.bad),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "  TAB next field · ENTER save · ESC cancel",
            Style::new().fg(t.dim),
        )));
    }
    frame.render_widget(Paragraph::new(lines).style(base(t)), inner);
}
