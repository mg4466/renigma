//! Application state and key handling. The TUI in [`crate::ui`] only reads
//! this state; all decisions live here.

use crate::config::{Config, is_greek};
use crate::enigma::{Machine, RotorSpec, SpecError, idx_to_letter, is_builtin, letter_to_idx};
use crate::theme::{self, THEMES, Theme};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const TABS: [&str; 6] = [
    "Machine",
    "Rotor",
    "Reflector",
    "Plugboard",
    "Theme",
    "Custom",
];
pub const LAMP_ROWS: [&str; 3] = ["QWERTZUIO", "ASDFGHJK", "PYXCVBNML"];
const TOAST_TIME: Duration = Duration::from_millis(2600);
pub const FLASH_TIME: Duration = Duration::from_millis(220);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Encrypt,
    Config,
    Help,
    RotorForm,
}

#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub positions: Vec<char>,
    pub len: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RotorForm {
    pub name: String,
    pub wiring: String,
    pub notches: String,
    pub field: usize,
    pub error: Option<String>,
    /// Index into `custom_rotors` when editing rather than creating.
    pub editing: Option<usize>,
}

impl RotorForm {
    pub fn new(spec: Option<&RotorSpec>, editing: Option<usize>) -> Self {
        let mut form = Self {
            editing,
            ..Default::default()
        };
        if let Some(spec) = spec {
            form.name = spec.name.clone();
            form.wiring = spec.wiring.clone();
            form.notches = spec.notches.clone();
        }
        form
    }

    pub fn values(&self) -> [&String; 3] {
        [&self.name, &self.wiring, &self.notches]
    }
}

pub struct App {
    pub cfg: Config,
    pub path: PathBuf,
    pub theme: Theme,
    pub machine: Machine,
    /// Positions the machine was configured with (what Ctrl-R restores).
    pub start_positions: Vec<char>,
    pub screen: Screen,
    pub under: Screen,
    pub tab: usize,
    pub row: usize,
    pub slot: usize,
    pub plug_cursor: usize,
    pub plug_pending: Option<u8>,
    pub output: String,
    pub history: Vec<HistoryEntry>,
    pub last_pair: Option<(char, char)>,
    pub flash: Vec<Option<Instant>>,
    pub toast: Option<(String, Instant)>,
    pub dirty: bool,
    pub last_save: Instant,
    pub form: Option<RotorForm>,
    pub should_quit: bool,
}

impl App {
    pub fn new(cfg: Config, path: PathBuf) -> Self {
        let mut cfg = cfg;
        cfg.normalize();
        let machine = cfg.build_machine().unwrap_or_else(|_| {
            let fallback = Config::default();
            fallback.build_machine().expect("default machine")
        });
        let start_positions = machine.positions();
        let theme = theme::get(&cfg.theme);
        Self {
            cfg,
            path,
            theme,
            machine,
            start_positions,
            screen: Screen::Encrypt,
            under: Screen::Encrypt,
            tab: 0,
            row: 0,
            slot: 2,
            plug_cursor: 0,
            plug_pending: None,
            output: String::new(),
            history: Vec::new(),
            last_pair: None,
            flash: vec![None; 4],
            toast: None,
            dirty: false,
            last_save: Instant::now(),
            form: None,
            should_quit: false,
        }
    }

    // ---------------------------------------------------------------- helpers

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn tick(&mut self) {
        if let Some((_, at)) = &self.toast
            && at.elapsed() > TOAST_TIME {
                self.toast = None;
            }
        // Debounced autosave: at most one write every couple of seconds.
        if self.dirty && self.last_save.elapsed() > Duration::from_secs(2) {
            self.write_config();
        }
        for f in self.flash.iter_mut() {
            if let Some(at) = f
                && at.elapsed() > FLASH_TIME {
                    *f = None;
                }
        }
    }

    pub fn flashing(&self, rotor: usize) -> bool {
        self.flash
            .get(rotor)
            .and_then(|f| *f)
            .is_some_and(|at| at.elapsed() <= FLASH_TIME)
    }

    fn write_config(&mut self) -> bool {
        match self.cfg.save(&self.path) {
            Ok(()) => {
                self.dirty = false;
                self.last_save = Instant::now();
                true
            }
            Err(e) => {
                self.toast(format!("could not save: {e}"));
                false
            }
        }
    }

    pub fn save(&mut self) {
        let path = self.path.display().to_string();
        if self.write_config() {
            self.toast(format!("saved {path}"));
        }
    }

    pub fn clear_message(&mut self) {
        self.output.clear();
        self.history.clear();
        self.last_pair = None;
    }

    pub fn sync_positions(&mut self) {
        self.cfg.rotors.positions = self
            .machine
            .positions()
            .iter()
            .map(|c| c.to_string())
            .collect();
    }

    /// Rebuild the machine from the config. Structural edits discard the
    /// current message; ring/theme edits do not.
    pub fn rebuild(&mut self, structural: bool) {
        let keep = self.machine.positions();
        let notes = self.cfg.normalize();
        match self.cfg.build_machine() {
            Ok(mut m) => {
                if !structural {
                    m.set_positions(&keep);
                }
                self.machine = m;
                if structural {
                    self.clear_message();
                }
                self.sync_positions();
                self.theme = theme::get(&self.cfg.theme);
                self.dirty = true;
            }
            Err(e) => {
                self.toast(format!("invalid setup: {e}"));
                return;
            }
        }
        if let Some(note) = notes.into_iter().next() {
            self.toast(note);
        }
    }

    fn reset_defaults(&mut self) {
        let customs = std::mem::take(&mut self.cfg.custom_rotors);
        let theme_name = self.cfg.theme.clone();
        self.cfg = Config::default();
        self.cfg.custom_rotors = customs;
        self.cfg.theme = theme_name;
        self.rebuild(true);
        self.start_positions = self.machine.positions();
        self.row = 0;
        self.slot = self.cfg.rotor_count() - 1;
        self.toast("settings reset to defaults");
    }

    // ---------------------------------------------------------------- encrypt

    fn encrypt_key(&mut self, c: char) {
        let upper = c.to_ascii_uppercase();
        if letter_to_idx(upper).is_none() {
            return;
        }
        // Snapshot before stepping so Backspace can rewind the rotors.
        let positions = self.machine.positions();
        if let Some(out) = self.machine.encrypt_char(upper) {
            self.history.push(HistoryEntry {
                positions,
                len: self.output.len(),
            });
            self.output.push(out);
            self.last_pair = Some((upper, out));
            // Flash the rotors that actually moved during this key press.
            if let Some(entry) = self.history.last() {
                for (i, (old, new)) in entry
                    .positions
                    .iter()
                    .zip(self.machine.positions().iter())
                    .enumerate()
                {
                    if old != new {
                        self.flash[i] = Some(Instant::now());
                    }
                }
            }
            self.sync_positions();
            self.dirty = true;
        }
    }

    fn undo(&mut self) {
        if let Some(entry) = self.history.pop() {
            self.output.truncate(entry.len);
            self.machine.set_positions(&entry.positions);
            self.last_pair = None;
            self.sync_positions();
            self.dirty = true;
        }
    }

    fn reset_positions(&mut self) {
        let start = self.start_positions.clone();
        self.machine.set_positions(&start);
        self.clear_message();
        self.sync_positions();
        self.toast("rotors reset");
    }

    // ---------------------------------------------------------------- keys

    pub fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('q' | 'c')) {
            self.should_quit = true;
            return;
        }
        if ctrl && key.code == KeyCode::Char('s') {
            self.save();
            return;
        }
        match self.screen {
            Screen::Encrypt => self.key_encrypt(key),
            Screen::Config => self.key_config(key),
            Screen::Help => {
                self.screen = self.under;
            }
            Screen::RotorForm => self.key_form(key),
        }
    }

    fn key_encrypt(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::F(2) => {
                self.screen = Screen::Config;
                self.under = Screen::Config;
                self.clamp_row();
            }
            KeyCode::F(1) | KeyCode::Char('?') => {
                self.under = Screen::Encrypt;
                self.screen = Screen::Help;
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.reset_positions()
            }
            KeyCode::Backspace => self.undo(),
            KeyCode::Esc => self.should_quit = true,
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::ALT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.encrypt_key(c)
            }
            _ => {}
        }
    }

    fn key_config(&mut self, key: KeyEvent) {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        match key.code {
            KeyCode::Esc | KeyCode::F(2) => {
                self.screen = Screen::Encrypt;
                self.under = Screen::Encrypt;
            }
            KeyCode::F(1) | KeyCode::Char('?') => {
                self.under = Screen::Config;
                self.screen = Screen::Help;
            }
            KeyCode::Tab => self.next_tab(1),
            KeyCode::BackTab => self.next_tab(-1),
            KeyCode::Char(c @ '1'..='6') => {
                self.tab = c as usize - '1' as usize;
                self.row = 0;
                self.plug_cursor = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_row(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_row(-1),
            KeyCode::Right | KeyCode::Char('l') => self.adjust(1),
            KeyCode::Left | KeyCode::Char('h') => self.adjust(-1),
            KeyCode::Enter => self.activate(),
            KeyCode::Char('[') => {
                if self.tab == 1 {
                    self.nudge_rotor(self.row, -1, true);
                }
            }
            KeyCode::Char(']') => {
                if self.tab == 1 {
                    self.nudge_rotor(self.row, 1, true);
                }
            }
            KeyCode::Char('x') if self.tab == 3 => {
                self.cfg.plugboard.clear();
                self.plug_pending = None;
                self.rebuild(true);
                self.toast("plugboard cleared");
            }
            KeyCode::Char('n') if self.tab == 5 => self.open_form(None),
            KeyCode::Char('e') if self.tab == 5 => self.edit_selected_custom(),
            KeyCode::Char('d') if self.tab == 5 => self.delete_selected_custom(),
            KeyCode::Char('R') if shift => self.reset_defaults(),
            KeyCode::Char(c)
                if self.tab == 1
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.set_position_letter(self.row, c)
            }
            _ => {}
        }
        self.clamp_row();
    }

    fn key_form(&mut self, key: KeyEvent) {
        let Some(form) = self.form.as_mut() else {
            self.screen = Screen::Config;
            return;
        };
        match key.code {
            KeyCode::Esc => {
                self.form = None;
                self.screen = Screen::Config;
            }
            KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % 3,
            KeyCode::BackTab | KeyCode::Up => form.field = (form.field + 2) % 3,
            KeyCode::Backspace => {
                match form.field {
                    0 => {
                        form.name.pop();
                    }
                    1 => {
                        form.wiring.pop();
                    }
                    _ => {
                        form.notches.pop();
                    }
                }
                form.error = None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                form.error = None;
                let field = form.field;
                let value = match field {
                    0 if form.name.len() < 16 => &mut form.name,
                    1 if form.wiring.len() < 26 => &mut form.wiring,
                    2 if form.notches.len() < 2 => &mut form.notches,
                    _ => return,
                };
                value.push(c.to_ascii_uppercase());
            }
            KeyCode::Enter => self.commit_form(),
            _ => {}
        }
    }

    // ---------------------------------------------------------------- config

    fn next_tab(&mut self, dir: i32) {
        let n = TABS.len() as i32;
        self.tab = ((self.tab as i32 + dir).rem_euclid(n)) as usize;
        self.row = 0;
        self.plug_cursor = 0;
        self.clamp_row();
    }

    pub fn row_count(&self) -> usize {
        match self.tab {
            0 => 1 + self.cfg.rotor_count(),
            1 => self.cfg.rotor_count(),
            2 => crate::enigma::REFLECTORS.len(),
            3 => 26,
            4 => THEMES.len(),
            5 => self.cfg.custom_rotors.len().max(1),
            _ => 0,
        }
    }

    fn clamp_row(&mut self) {
        if self.tab == 3 {
            self.plug_cursor = self.plug_cursor.min(25);
        } else {
            let n = self.row_count();
            if self.row >= n {
                self.row = n.saturating_sub(1);
            }
        }
    }

    fn move_row(&mut self, dir: i32) {
        if self.tab == 3 {
            let step = if dir.abs() == 13 { 13 } else { 1 };
            let _ = step;
        }
        match self.tab {
            3 => {
                let c = self.plug_cursor as i32;
                self.plug_cursor = (c + dir * 13).rem_euclid(26) as usize;
            }
            _ => {
                let n = self.row_count() as i32;
                if n == 0 {
                    return;
                }
                self.row = ((self.row as i32 + dir).rem_euclid(n)) as usize;
                if self.tab == 0 && self.row >= 1 {
                    self.slot = self.row - 1;
                }
                if self.tab == 4 {
                    self.apply_theme_row();
                }
            }
        }
    }

    fn adjust(&mut self, dir: i32) {
        match self.tab {
            0 => {
                if self.row == 0 {
                    let count = if dir > 0 { 4 } else { 3 };
                    if count != self.cfg.rotor_count() {
                        self.set_count(count as u8);
                    }
                } else {
                    self.cycle_slot(self.row - 1, dir);
                }
            }
            1 => {
                self.nudge_rotor(self.row, dir, false);
            }
            2 => {
                let n = crate::enigma::REFLECTORS.len() as i32;
                let next = ((self.row as i32 + dir).rem_euclid(n)) as usize;
                self.row = next;
                self.select_reflector(next);
            }
            3 => {
                self.plug_cursor = ((self.plug_cursor as i32 + dir).rem_euclid(26)) as usize;
            }
            4 => {
                let n = THEMES.len() as i32;
                self.row = ((self.row as i32 + dir).rem_euclid(n)) as usize;
                self.apply_theme_row();
            }
            5 => {}
            _ => {}
        }
    }

    fn activate(&mut self) {
        match self.tab {
            0 => {
                if self.row > 0 {
                    self.cycle_slot(self.row - 1, 1);
                }
            }
            1 => {}
            2 => self.select_reflector(self.row),
            3 => self.toggle_plug(self.plug_cursor as u8),
            4 => self.apply_theme_row(),
            5 => self.assign_selected_custom(),
            _ => {}
        }
    }

    fn set_count(&mut self, count: u8) {
        self.cfg.rotors.count = count;
        if count == 4 {
            let has_greek = self.cfg.rotors.order.iter().any(|n| is_greek(n));
            if !has_greek && self.cfg.is_available_rotor("Beta") {
                self.cfg.rotors.order.insert(0, "Beta".into());
            }
        }
        self.rebuild(true);
        self.start_positions = self.machine.positions();
        self.slot = self.slot.min(self.cfg.rotor_count() - 1);
        self.toast(format!("{count}-rotor machine"));
    }

    fn cycle_slot(&mut self, slot: usize, dir: i32) {
        self.slot = slot;
        let count = self.cfg.rotor_count();
        let leftmost = count == 4 && slot == 0;
        let used: Vec<String> = self
            .cfg
            .rotors
            .order
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != slot)
            .map(|(_, n)| n.clone())
            .collect();
        let candidates: Vec<String> = self
            .cfg
            .available_rotors()
            .into_iter()
            .filter(|s| !used.contains(&s.name) && (leftmost || !is_greek(&s.name)))
            .map(|s| s.name)
            .collect();
        if candidates.is_empty() {
            return;
        }
        let current = self.cfg.rotors.order.get(slot).cloned();
        let index = candidates
            .iter()
            .position(|n| Some(n) == current.as_ref())
            .unwrap_or(0);
        let next = if dir >= 0 {
            (index + 1) % candidates.len()
        } else {
            (index + candidates.len() - 1) % candidates.len()
        };
        if self.cfg.rotors.order.len() <= slot {
            self.cfg.rotors.order.resize(slot + 1, String::new());
        }
        self.cfg.rotors.order[slot] = candidates[next].clone();
        self.rebuild(true);
        if self.cfg.rotors.order.first() != Some(&candidates[next]) || slot == 0 {
            self.start_positions = self.machine.positions();
        }
    }

    /// Move a rotor window (or ring) by one letter.
    pub fn nudge_rotor(&mut self, slot: usize, dir: i32, ring: bool) {
        let count = self.cfg.rotor_count();
        if slot >= count {
            return;
        }
        let step = if dir >= 0 { 1i32 } else { -1 };
        if ring {
            let cur = letter_to_idx(
                self.cfg
                    .rotors
                    .rings
                    .get(slot)
                    .and_then(|s| s.chars().next())
                    .unwrap_or('A'),
            )
            .unwrap_or(0) as i32;
            let next = (cur + step).rem_euclid(26) as u8;
            self.cfg.rotors.rings[slot] = idx_to_letter(next).to_string();
            self.rebuild(false);
            self.start_positions = self.machine.positions();
        } else {
            let cur = self.machine.rotors[slot].position as i32;
            let next = (cur + step).rem_euclid(26) as u8;
            self.machine.rotors[slot].position = next;
            self.sync_positions();
            self.start_positions = self.machine.positions();
            self.dirty = true;
        }
    }

    pub fn set_position_letter(&mut self, slot: usize, c: char) {
        if let Some(i) = letter_to_idx(c)
            && let Some(rotor) = self.machine.rotors.get_mut(slot) {
                rotor.position = i;
                self.sync_positions();
                self.start_positions = self.machine.positions();
                self.dirty = true;
            }
    }

    pub fn set_ring_letter(&mut self, slot: usize, c: char) {
        if let Some(i) = letter_to_idx(c)
            && slot < self.cfg.rotor_count() {
                self.cfg.rotors.rings[slot] = idx_to_letter(i).to_string();
                self.rebuild(false);
                self.start_positions = self.machine.positions();
            }
    }

    fn select_reflector(&mut self, index: usize) {
        let name = crate::enigma::REFLECTORS[index].0;
        // Auto-adjust the rotor count so the choice is valid instead of
        // silently ignoring the key.
        let thin = name.ends_with("-Thin");
        if thin && self.cfg.rotor_count() != 4 {
            self.toast(format!("{name} needs 4 rotors — switching"));
            self.cfg.rotors.count = 4;
            if !self.cfg.rotors.order.iter().any(|n| is_greek(n)) {
                self.cfg.rotors.order.insert(0, "Beta".into());
            }
        }
        self.cfg.reflector = name.to_string();
        self.rebuild(true);
        self.start_positions = self.machine.positions();
        self.toast(format!("reflector {name}"));
    }

    fn apply_theme_row(&mut self) {
        if let Some(theme) = THEMES.get(self.row) {
            self.cfg.theme = theme.name.to_string();
            self.theme = *theme;
            self.dirty = true;
        }
    }

    pub fn pair_index(&self, letter: u8) -> Option<usize> {
        self.cfg
            .plug_pairs()
            .iter()
            .position(|(a, b)| *a == letter || *b == letter)
    }

    fn toggle_plug(&mut self, letter: u8) {
        if let Some(partner) = self.plug_pending {
            if partner == letter {
                self.plug_pending = None;
                return;
            }
            if self.cfg.plug_pairs().len() >= 13 && self.pair_index(letter).is_none() {
                self.toast("plugboard full (13 pairs)");
                self.plug_pending = None;
                return;
            }
            // Free both letters from existing pairs, then connect.
            self.unplug(letter);
            self.unplug(partner);
            self.cfg.plugboard.push(format!(
                "{}{}",
                idx_to_letter(partner),
                idx_to_letter(letter)
            ));
            self.plug_pending = None;
            self.rebuild(true);
            self.toast(format!(
                "plugged {} ↔ {}",
                idx_to_letter(partner),
                idx_to_letter(letter)
            ));
        } else if self.pair_index(letter).is_some() {
            self.unplug(letter);
            self.rebuild(true);
            self.toast(format!("unplugged {}", idx_to_letter(letter)));
        } else {
            self.plug_pending = Some(letter);
        }
    }

    fn unplug(&mut self, letter: u8) {
        let before = self.cfg.plugboard.len();
        self.cfg
            .plugboard
            .retain(|pair| !pair.chars().any(|c| letter_to_idx(c) == Some(letter)));
        if before != self.cfg.plugboard.len() {
            self.dirty = true;
        }
    }

    // ---------------------------------------------------------------- custom

    fn open_form(&mut self, editing: Option<usize>) {
        let spec = editing.and_then(|i| self.cfg.custom_rotors.get(i)).cloned();
        self.form = Some(RotorForm::new(spec.as_ref(), editing));
        self.under = Screen::Config;
        self.screen = Screen::RotorForm;
    }

    fn edit_selected_custom(&mut self) {
        if self.tab != 5 {
            return;
        }
        if self.row < self.cfg.custom_rotors.len() {
            self.open_form(Some(self.row));
        }
    }

    fn delete_selected_custom(&mut self) {
        if self.tab != 5 || self.row >= self.cfg.custom_rotors.len() {
            return;
        }
        let removed = self.cfg.custom_rotors.remove(self.row);
        self.rebuild(true);
        self.start_positions = self.machine.positions();
        self.toast(format!("deleted rotor {}", removed.name));
        self.clamp_row();
    }

    fn commit_form(&mut self) {
        let Some(form) = self.form.clone() else {
            return;
        };
        match RotorSpec::from_parts(form.name.clone(), &form.wiring, &form.notches) {
            Ok(spec) => {
                if spec.name.starts_with("Beta") || spec.name == "Gamma" {
                    if let Some(f) = self.form.as_mut() {
                        f.error = Some("those names are reserved for the historical rotors".into());
                    }
                    return;
                }
                if is_builtin(&spec.name) {
                    if let Some(f) = self.form.as_mut() {
                        f.error = Some(format!("'{}' is a built-in rotor", spec.name));
                    }
                    return;
                }
                match form.editing {
                    Some(i) if i < self.cfg.custom_rotors.len() => {
                        self.cfg.custom_rotors[i] = spec.clone();
                    }
                    _ => self.cfg.custom_rotors.push(spec.clone()),
                }
                self.form = None;
                self.screen = Screen::Config;
                self.tab = 5;
                self.rebuild(true);
                self.start_positions = self.machine.positions();
                self.toast(format!("rotor '{}' saved", spec.name));
            }
            Err(SpecError(msg)) => {
                if let Some(f) = self.form.as_mut() {
                    f.error = Some(msg);
                }
            }
        }
    }

    fn assign_selected_custom(&mut self) {
        if self.tab != 5 || self.row >= self.cfg.custom_rotors.len() {
            return;
        }
        let name = self.cfg.custom_rotors[self.row].name.clone();
        let count = self.cfg.rotor_count();
        let mut slot = self.slot.min(count - 1);
        if count == 4 && !is_greek(&name) {
            // Any non-greek rotor may sit anywhere except the greek slot.
        }
        if let Some(pos) = self.cfg.rotors.order.iter().position(|n| *n == name) {
            self.cfg.rotors.order.swap(pos, slot);
        } else {
            self.cfg.rotors.order[slot] = name.clone();
        }
        self.rebuild(true);
        self.start_positions = self.machine.positions();
        // `rebuild` may have shuffled a duplicate away; report what happened.
        let placed = self
            .cfg
            .rotors
            .order
            .iter()
            .position(|n| *n == name)
            .map(|i| i + 1);
        match placed {
            Some(i) => self.toast(format!("'{name}' is now rotor {i} (left to right)")),
            None => self.toast(format!("could not place '{name}'")),
        }
        slot = slot.min(count - 1);
        self.slot = slot;
    }

    // ---------------------------------------------------------------- misc

    pub fn theme_name(&self) -> &'static str {
        self.theme.label
    }

    pub fn mode_label(&self) -> &'static str {
        match self.screen {
            Screen::Encrypt => "ENCRYPT",
            Screen::Config | Screen::RotorForm => "CONFIGURE",
            Screen::Help => "HELP",
        }
    }

    pub fn selected_reflector(&self) -> &str {
        crate::enigma::REFLECTORS[self.row.min(crate::enigma::REFLECTORS.len() - 1)].0
    }

    pub fn plug_letter_at(&self, idx: usize) -> char {
        idx_to_letter(idx as u8)
    }
}
