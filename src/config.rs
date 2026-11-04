//! Persistence: a human-readable TOML config file.

use crate::enigma::{
    Machine, Reflector, Rotor, RotorSpec, SpecError, builtin_rotor, letter_to_idx, validate_wiring,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Default path: `$XDG_CONFIG_HOME/renigma/config.toml` or `~/.config/renigma/config.toml`.
pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("renigma").join("config.toml")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct RotorConfig {
    pub count: u8,
    /// Rotor names, left to right.
    pub order: Vec<String>,
    pub positions: Vec<String>,
    pub rings: Vec<String>,
}

impl Default for RotorConfig {
    fn default() -> Self {
        Self {
            count: 3,
            order: vec!["I".into(), "II".into(), "III".into()],
            positions: vec!["A".into(), "A".into(), "A".into()],
            rings: vec!["A".into(), "A".into(), "A".into()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub reflector: String,
    pub rotors: RotorConfig,
    pub plugboard: Vec<String>,
    /// User-defined rotors; selectable anywhere a built-in rotor is.
    pub custom_rotors: Vec<RotorSpec>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "catppuccin-mocha".into(),
            reflector: "B".into(),
            rotors: RotorConfig::default(),
            plugboard: Vec::new(),
            custom_rotors: Vec::new(),
        }
    }
}

impl Config {
    /// All rotor definitions available to the machine: historical first,
    /// then the user's custom rotors.
    pub fn available_rotors(&self) -> Vec<RotorSpec> {
        let mut specs: Vec<RotorSpec> = crate::enigma::BUILTIN_ROTORS
            .iter()
            .map(|(n, w, notches)| RotorSpec {
                name: (*n).to_string(),
                wiring: (*w).to_string(),
                notches: (*notches).to_string(),
            })
            .collect();
        for spec in &self.custom_rotors {
            if validate_wiring(&spec.wiring).is_ok() && !spec.name.trim().is_empty() {
                specs.push(spec.clone());
            }
        }
        specs
    }

    pub fn find_rotor(&self, name: &str) -> Option<RotorSpec> {
        builtin_rotor(name).or_else(|| {
            self.custom_rotors
                .iter()
                .find(|r| r.name == name)
                .filter(|r| validate_wiring(&r.wiring).is_ok())
                .cloned()
        })
    }

    pub fn is_available_rotor(&self, name: &str) -> bool {
        self.find_rotor(name).is_some()
    }

    pub fn plug_pairs(&self) -> Vec<(u8, u8)> {
        self.plugboard
            .iter()
            .filter_map(|pair| {
                let mut chars = pair.chars().filter(|c| c.is_ascii_alphabetic());
                let a = letter_to_idx(chars.next()?)?;
                let b = letter_to_idx(chars.next()?)?;
                (a != b).then_some((a, b))
            })
            .collect()
    }

    /// Clamp anything that does not make sense into something the machine can
    /// actually build. Returns human-readable notes about what was changed.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut notes = Vec::new();

        if self.rotor_count() != 3 && self.rotor_count() != 4 {
            notes.push("rotor count must be 3 or 4; using 3".into());
            self.rotors.count = 3;
        }
        let count = self.rotors.count as usize;

        // Duplicate custom names would be ambiguous.
        let mut seen_names = std::collections::HashSet::new();
        self.custom_rotors.retain(|r| {
            validate_wiring(&r.wiring).is_ok()
                && !r.name.trim().is_empty()
                && seen_names.insert(r.name.clone())
        });

        // Fix the rotor order: drop unknown/duplicate entries, then pad.
        let mut order: Vec<String> = Vec::new();
        for name in self.rotors.order.iter() {
            if order.len() < count && !order.contains(name) && self.is_available_rotor(name) {
                order.push(name.clone());
            }
        }
        if order.len() != self.rotors.order.len().min(count) {
            notes.push("rotor order contained unknown or repeated rotors".into());
        }
        let all = self.available_rotors();
        while order.len() < count {
            if let Some(spec) = all.iter().find(|s| !order.contains(&s.name)) {
                order.push(spec.name.clone());
            } else {
                break;
            }
        }
        if order.len() != count {
            notes.push(format!("could only find {count} usable rotors"));
        }
        self.rotors.order = order;

        // Greek rotors (Beta/Gamma) may only sit in the leftmost slot of an M4.
        if count == 4 {
            for (i, name) in self.rotors.order.iter().enumerate() {
                if is_greek(name) && i != 0 {
                    // Swap the greek rotor into the leftmost slot.
                    self.rotors.order.swap(0, i);
                    notes.push("Beta/Gamma moved to the leftmost slot".into());
                    break;
                }
            }
            if self.rotors.order.iter().skip(1).any(|n| is_greek(n)) {
                notes.push("Beta/Gamma can only be used in an M4's leftmost slot".into());
                self.rotors.order.retain(|n| !is_greek(n));
                let all = self.available_rotors();
                while self.rotors.order.len() < count {
                    if let Some(spec) = all
                        .iter()
                        .find(|s| !is_greek(&s.name) && !self.rotors.order.contains(&s.name))
                    {
                        self.rotors.order.push(spec.name.clone());
                    } else {
                        break;
                    }
                }
            }
        } else if self.rotors.order.iter().any(|n| is_greek(n)) {
            notes.push("Beta/Gamma require the 4-rotor machine".into());
            let all = self.available_rotors();
            self.rotors.order.retain(|n| !is_greek(n));
            while self.rotors.order.len() < count {
                if let Some(spec) = all
                    .iter()
                    .find(|s| !is_greek(&s.name) && !self.rotors.order.contains(&s.name))
                {
                    self.rotors.order.push(spec.name.clone());
                } else {
                    break;
                }
            }
        }

        // Positions and rings: pad / trim, coerce to single letters.
        normalize_letters(&mut self.rotors.positions, count);
        normalize_letters(&mut self.rotors.rings, count);

        // Reflector must match the machine generation.
        let thin = self.reflector.ends_with("-Thin");
        if count == 4 && !thin {
            notes.push("M4 requires a thin reflector; using B-Thin".into());
            self.reflector = "B-Thin".into();
        } else if count == 3 && thin {
            notes.push("thin reflectors require 4 rotors; using B".into());
            self.reflector = "B".into();
        }
        if Reflector::from_name(&self.reflector).is_none() {
            notes.push(format!("unknown reflector '{}'; using B", self.reflector));
            self.reflector = if count == 4 {
                "B-Thin".into()
            } else {
                "B".into()
            };
        }

        // Plugboard: at most 13 pairs, each letter used once.
        let mut used = [false; 26];
        let mut pairs = Vec::new();
        for pair in self.plugboard.iter() {
            let letters: Vec<u8> = pair
                .chars()
                .filter_map(letter_to_idx)
                .filter(|i| !used[*i as usize])
                .collect();
            if letters.len() == 2 && letters[0] != letters[1] && !used[letters[1] as usize] {
                used[letters[0] as usize] = true;
                used[letters[1] as usize] = true;
                pairs.push([letters[0], letters[1]]);
            }
        }
        if pairs.len() != self.plugboard.len() {
            notes.push("plugboard trimmed to valid, non-conflicting pairs".into());
        }
        self.plugboard = pairs
            .iter()
            .map(|&[a, b]| {
                format!(
                    "{}{}",
                    crate::enigma::idx_to_letter(a),
                    crate::enigma::idx_to_letter(b)
                )
            })
            .collect();
        if self.plugboard.len() > 13 {
            self.plugboard.truncate(13);
        }

        if crate::theme::lookup(&self.theme).is_none() {
            notes.push(format!("unknown theme '{}'; using default", self.theme));
            self.theme = "default".into();
        }

        notes
    }

    pub fn rotor_count(&self) -> usize {
        self.rotors.count as usize
    }

    pub fn build_machine(&self) -> Result<Machine, SpecError> {
        let mut rotors = Vec::new();
        for (i, name) in self
            .rotors
            .order
            .iter()
            .take(self.rotor_count())
            .enumerate()
        {
            let spec = self
                .find_rotor(name)
                .ok_or_else(|| SpecError(format!("unknown rotor '{name}'")))?;
            let pos = self
                .rotors
                .positions
                .get(i)
                .and_then(|s| s.chars().next())
                .and_then(letter_to_idx)
                .unwrap_or(0);
            let ring = self
                .rotors
                .rings
                .get(i)
                .and_then(|s| s.chars().next())
                .and_then(letter_to_idx)
                .unwrap_or(0);
            rotors.push(Rotor::new(spec, pos, ring));
        }
        let reflector = Reflector::from_name(&self.reflector)
            .ok_or_else(|| SpecError(format!("unknown reflector '{}'", self.reflector)))?;
        Machine::new(rotors, reflector, self.plug_pairs())
    }

    /// Readable TOML, including a short header comment. Written by hand so
    /// arrays stay on one line (the generic serialiser explodes them).
    pub fn to_toml(&self) -> String {
        fn quoted(value: &str) -> String {
            format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        }
        fn inline_list(items: &[String]) -> String {
            items
                .iter()
                .map(|i| quoted(i))
                .collect::<Vec<_>>()
                .join(", ")
        }

        let mut out = String::from(
            "# Renigma configuration\n\
             # Rotor order is left to right; the rightmost rotor is the fast one.\n\
             # Positions and rings are single letters A-Z.\n\
             # Plugboard pairs are two letters each, at most 13 pairs.\n\
             # Add a custom rotor with:\n\
             # [[custom_rotors]]\n\
             # name = \"My rotor\"\n\
             # wiring = \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\"\n\
             # notches = \"Q\"\n",
        );
        out.push_str(&format!("theme = {}\n", quoted(&self.theme)));
        out.push_str(&format!("reflector = {}\n", quoted(&self.reflector)));
        out.push_str(&format!("plugboard = [{}]\n", inline_list(&self.plugboard)));
        out.push_str("\n[rotors]\n");
        out.push_str(&format!("count = {}\n", self.rotors.count));
        out.push_str(&format!("order = [{}]\n", inline_list(&self.rotors.order)));
        out.push_str(&format!(
            "positions = [{}]\n",
            inline_list(&self.rotors.positions)
        ));
        out.push_str(&format!("rings = [{}]\n", inline_list(&self.rotors.rings)));

        for spec in &self.custom_rotors {
            out.push_str("\n[[custom_rotors]]\n");
            out.push_str(&format!("name = {}\n", quoted(&spec.name)));
            out.push_str(&format!("wiring = {}\n", quoted(&spec.wiring)));
            out.push_str(&format!("notches = {}\n", quoted(&spec.notches)));
        }
        out
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, self.to_toml())
    }

    /// Load a config file. A missing file yields defaults (and is reported as
    /// such); a malformed file yields defaults plus a warning string.
    pub fn load(path: &Path) -> (Self, Vec<String>) {
        match fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(mut cfg) => {
                    let notes = cfg.normalize();
                    (cfg, notes)
                }
                Err(e) => (
                    Config::default(),
                    vec![format!("could not parse {}: {e}", path.display())],
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), Vec::new()),
            Err(e) => (
                Config::default(),
                vec![format!("could not read {}: {e}", path.display())],
            ),
        }
    }
}

fn normalize_letters(list: &mut Vec<String>, count: usize) {
    for item in list.iter_mut() {
        let c = item
            .chars()
            .find(|c| c.is_ascii_alphabetic())
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('A');
        *item = c.to_string();
    }
    while list.len() < count {
        list.push("A".into());
    }
    list.truncate(count);
}

pub fn is_greek(name: &str) -> bool {
    matches!(name, "Beta" | "Gamma")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let mut cfg = Config::default();
        cfg.rotors.count = 4;
        cfg.rotors.order = vec!["Beta".into(), "I".into(), "IV".into(), "VIII".into()];
        cfg.rotors.positions = vec!["A".into(), "B".into(), "C".into(), "D".into()];
        cfg.rotors.rings = vec!["B".into(), "B".into(), "B".into(), "B".into()];
        cfg.reflector = "B-Thin".into();
        cfg.plugboard = vec!["AV".into(), "BS".into()];
        cfg.custom_rotors.push(RotorSpec {
            name: "Custom".into(),
            wiring: "BACDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            notches: "Q".into(),
        });
        let text = cfg.to_toml();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.rotors, cfg.rotors);
        assert_eq!(parsed.reflector, cfg.reflector);
        assert_eq!(parsed.plugboard, cfg.plugboard);
        assert_eq!(parsed.custom_rotors, cfg.custom_rotors);
        let mut parsed = parsed;
        assert!(parsed.normalize().is_empty());
        parsed.build_machine().unwrap();
    }

    #[test]
    fn normalizes_bad_input() {
        let mut cfg = Config::default();
        cfg.rotors.count = 4;
        cfg.rotors.order = vec!["I".into(), "I".into(), "Nope".into()];
        cfg.rotors.positions = vec!["a".into()];
        cfg.plugboard = vec!["AA".into(), "AV".into(), "VA".into(), "X".into()];
        let notes = cfg.normalize();
        assert!(!notes.is_empty());
        assert_eq!(cfg.rotors.order.len(), 4);
        assert_eq!(cfg.rotors.order[0], "I");
        assert_eq!(cfg.rotors.positions.len(), 4);
        assert_eq!(cfg.reflector, "B-Thin");
        assert_eq!(cfg.plugboard, vec!["AV".to_string()]);
        cfg.build_machine().unwrap();
    }

    #[test]
    fn greek_rotor_only_leftmost() {
        let mut cfg = Config::default();
        cfg.rotors.count = 4;
        cfg.rotors.order = vec!["I".into(), "Beta".into(), "II".into(), "III".into()];
        cfg.reflector = "C-Thin".into();
        cfg.normalize();
        assert_eq!(cfg.rotors.order[0], "Beta");
        assert_eq!(cfg.rotors.order.len(), 4);
        cfg.build_machine().unwrap();
    }

    #[test]
    fn greek_rotor_rejected_for_three() {
        let mut cfg = Config::default();
        cfg.rotors.order = vec!["Beta".into(), "I".into(), "II".into()];
        cfg.normalize();
        assert!(!cfg.rotors.order.iter().any(|n| is_greek(n)));
        cfg.build_machine().unwrap();
    }

    #[test]
    fn missing_file_is_defaults() {
        let (cfg, notes) = Config::load(Path::new("/nonexistent/renigma/config.toml"));
        assert_eq!(cfg, Config::default());
        assert!(notes.is_empty());
    }

    #[test]
    fn custom_rotor_is_selectable() {
        let mut cfg = Config::default();
        cfg.custom_rotors.push(RotorSpec {
            name: "Custom".into(),
            wiring: "BACDEFGHIJKLMNOPQRSTUVWXYZ".into(),
            notches: "Q".into(),
        });
        cfg.rotors.order = vec!["Custom".into(), "I".into(), "II".into()];
        cfg.normalize();
        assert!(cfg.is_available_rotor("Custom"));
        assert_eq!(cfg.build_machine().unwrap().rotors[0].name(), "Custom");
    }

    #[test]
    fn unknown_theme_falls_back() {
        let mut cfg = Config {
            theme: "hotdog-stand".into(),
            ..Config::default()
        };
        cfg.normalize();
        assert_eq!(cfg.theme, "default");
    }
}
