//! Pure Enigma logic: rotors, reflector, plugboard and machine stepping.
//!
//! Nothing in this module depends on the TUI, so it can be tested in isolation
//! and reused by other front ends.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const ALPHABET: usize = 26;
pub const LETTERS: [char; ALPHABET] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

pub fn letter_to_idx(c: char) -> Option<u8> {
    if c.is_ascii_alphabetic() {
        Some(c.to_ascii_uppercase() as u8 - b'A')
    } else {
        None
    }
}

pub fn idx_to_letter(i: u8) -> char {
    LETTERS[(i as usize) % ALPHABET]
}

/// Historical rotor wirings (A to Z order) and their turnover notches.
pub const BUILTIN_ROTORS: &[(&str, &str, &str)] = &[
    ("I", "EKMFLGDQVZNTOWYHXUSPAIBRCJ", "Q"),
    ("II", "AJDKSIRUXBLHWTMCQGZNPYFVOE", "E"),
    ("III", "BDFHJLCPRTXVZNYEIWGAKMUSQO", "V"),
    ("IV", "ESOVPZJAYQUIRHXLNFTGKDCMWB", "J"),
    ("V", "VZBRGITYUPSDNHLXAWMJQOFECK", "Z"),
    ("VI", "JPGVOUMFYQBENHZRDKASXLICTW", "ZM"),
    ("VII", "NZJHGRCXMYSWBOUFAIVLPEKQDT", "ZM"),
    ("VIII", "FKQHTLXOCBJSPDZRAMEWNIUYGV", "ZM"),
    ("Beta", "LEYJVCNIXWPBQMDRTAKZGFUHOS", ""),
    ("Gamma", "FSOKANUERHMBTIYCWLQPZXVGJD", ""),
];

/// Reflector wirings. `B`/`C` are the standard (3-rotor) reflectors, the
/// `-Thin` variants belong to the 4-rotor M4 machine.
pub const REFLECTORS: &[(&str, &str)] = &[
    ("B", "YRUHQSLDPXNGOKMIEBFZCWVJAT"),
    ("C", "FVPJIAOYEDRZXWGCTKUQSBNMHL"),
    ("B-Thin", "ENKQAUYWJICOPBLMDXZVFTHRGS"),
    ("C-Thin", "RDOBJNTKVEHMLFCWZAXGYIPSUQ"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecError(pub String);

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SpecError {}

/// A rotor definition: name, 26-letter wiring and turnover notch letters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotorSpec {
    pub name: String,
    pub wiring: String,
    #[serde(default)]
    pub notches: String,
}

impl RotorSpec {
    pub fn from_parts(
        name: impl Into<String>,
        wiring: &str,
        notches: &str,
    ) -> Result<Self, SpecError> {
        let name = name.into();
        let wiring = wiring.trim().to_ascii_uppercase();
        let notches = notches.trim().to_ascii_uppercase();
        if name.trim().is_empty() {
            return Err(SpecError("rotor name must not be empty".into()));
        }
        validate_wiring(&wiring)?;
        if notches.chars().any(|c| !c.is_ascii_alphabetic()) {
            return Err(SpecError(format!("bad notches '{notches}'")));
        }
        Ok(Self {
            name,
            wiring,
            notches,
        })
    }

    pub fn wiring_chars(&self) -> Vec<char> {
        self.wiring.chars().collect()
    }

    /// Compact notch display for the UI, e.g. `Q` or `Z M`.
    pub fn notch_display(&self) -> String {
        if self.notches.is_empty() {
            "none".to_string()
        } else {
            self.notches
                .chars()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

pub fn validate_wiring(wiring: &str) -> Result<(), SpecError> {
    let chars: Vec<char> = wiring.chars().collect();
    if chars.len() != ALPHABET {
        return Err(SpecError(format!(
            "wiring must be exactly 26 letters, got {}",
            chars.len()
        )));
    }
    let mut seen = [false; ALPHABET];
    for c in chars {
        let Some(i) = letter_to_idx(c) else {
            return Err(SpecError(format!("wiring contains non-letter '{c}'")));
        };
        if seen[i as usize] {
            return Err(SpecError(format!("wiring repeats letter {c}")));
        }
        seen[i as usize] = true;
    }
    Ok(())
}

pub fn builtin_rotor(name: &str) -> Option<RotorSpec> {
    BUILTIN_ROTORS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(n, w, notches)| RotorSpec {
            name: (*n).to_string(),
            wiring: (*w).to_string(),
            notches: (*notches).to_string(),
        })
}

pub fn is_builtin(name: &str) -> bool {
    BUILTIN_ROTORS.iter().any(|(n, _, _)| *n == name)
}

/// A rotor in the machine: a spec plus its live window position and ring setting.
#[derive(Debug, Clone)]
pub struct Rotor {
    pub spec: RotorSpec,
    fwd: [u8; ALPHABET],
    bwd: [u8; ALPHABET],
    notch_idxs: Vec<u8>,
    pub position: u8,
    pub ring: u8,
}

impl Rotor {
    pub fn new(spec: RotorSpec, position: u8, ring: u8) -> Self {
        let mut fwd = [0u8; ALPHABET];
        let mut bwd = [0u8; ALPHABET];
        for (i, c) in spec.wiring.chars().enumerate() {
            let o = letter_to_idx(c).expect("validated wiring") as usize;
            fwd[i] = o as u8;
            bwd[o] = i as u8;
        }
        let notch_idxs = spec
            .notches
            .chars()
            .filter_map(letter_to_idx)
            .collect::<Vec<_>>();
        Self {
            spec,
            fwd,
            bwd,
            notch_idxs,
            position: position % 26,
            ring: ring % 26,
        }
    }

    pub fn name(&self) -> &str {
        &self.spec.name
    }

    /// True when the window is sitting on a turnover notch (checked *before*
    /// the key press, which is what produces the historical double step).
    pub fn at_notch(&self) -> bool {
        self.notch_idxs.contains(&self.position)
    }

    pub fn step(&mut self) {
        self.position = (self.position + 1) % 26;
    }

    pub fn window_letter(&self) -> char {
        idx_to_letter(self.position)
    }

    pub fn ring_letter(&self) -> char {
        idx_to_letter(self.ring)
    }

    fn forward(&self, input: u8) -> u8 {
        let shift = (self.position + 26 - self.ring) % 26;
        let idx = (input + shift) % 26;
        (self.fwd[idx as usize] + 26 - shift) % 26
    }

    fn backward(&self, input: u8) -> u8 {
        let shift = (self.position + 26 - self.ring) % 26;
        let idx = (input + shift) % 26;
        (self.bwd[idx as usize] + 26 - shift) % 26
    }
}

#[derive(Debug, Clone)]
pub struct Reflector {
    pub name: String,
    wiring: [u8; ALPHABET],
}

impl Reflector {
    pub fn from_name(name: &str) -> Option<Self> {
        let (n, w) = REFLECTORS.iter().find(|(n, _)| *n == name)?;
        let mut wiring = [0u8; ALPHABET];
        for (i, c) in w.chars().enumerate() {
            wiring[i] = letter_to_idx(c)?;
        }
        Some(Self {
            name: (*n).to_string(),
            wiring,
        })
    }

    pub fn is_thin(&self) -> bool {
        self.name.ends_with("-Thin")
    }

    fn reflect(&self, input: u8) -> u8 {
        self.wiring[input as usize]
    }

    pub fn wiring_string(&self) -> String {
        self.wiring.iter().map(|&i| idx_to_letter(i)).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepEvent {
    /// Index of the rotor that moved (0 = leftmost).
    pub rotor: usize,
    /// The rotor that stepped a second time in the same key press, if any.
    pub double: Option<usize>,
}

/// A configured Enigma machine. Rotor indices run left → right, with the
/// rightmost rotor being the fast one.
#[derive(Debug, Clone)]
pub struct Machine {
    pub rotors: Vec<Rotor>,
    pub reflector: Reflector,
    /// Plugboard substitution table (identity for unplugged letters).
    plug: [u8; ALPHABET],
    pub plug_pairs: Vec<(u8, u8)>,
}

impl Machine {
    pub fn new(
        rotors: Vec<Rotor>,
        reflector: Reflector,
        plug_pairs: Vec<(u8, u8)>,
    ) -> Result<Self, SpecError> {
        if rotors.len() < 2 || rotors.len() > 4 {
            return Err(SpecError("machine needs 3 or 4 rotors".into()));
        }
        if rotors.len() == 2 {
            return Err(SpecError("machine needs at least 3 rotors".into()));
        }
        if rotors.len() == 4 && !reflector.is_thin() {
            return Err(SpecError(
                "the 4-rotor M4 uses the thin B or C reflector".into(),
            ));
        }
        if rotors.len() == 3 && reflector.is_thin() {
            return Err(SpecError(
                "thin reflectors require the 4-rotor machine".into(),
            ));
        }
        let mut machine = Self {
            rotors,
            reflector,
            plug: std::array::from_fn(|i| i as u8),
            plug_pairs: Vec::new(),
        };
        for (a, b) in plug_pairs {
            if a == b || a >= 26 || b >= 26 {
                continue;
            }
            machine.plug[a as usize] = b;
            machine.plug[b as usize] = a;
            machine.plug_pairs.push((a, b));
        }
        machine.plug_pairs.sort_unstable();
        Ok(machine)
    }

    /// Advance the rotors before encryption, following the real machine's
    /// stepping rules (including the middle-rotor double step).
    pub fn step(&mut self) -> Option<StepEvent> {
        let n = self.rotors.len();
        if n < 2 {
            return None;
        }
        let right = n - 1;
        let mid = n - 2;
        let mut double = None;

        if self.rotors[mid].at_notch() {
            self.rotors[mid].step();
            self.rotors[mid - 1].step();
            double = Some(mid - 1);
        } else if self.rotors[right].at_notch() {
            self.rotors[mid].step();
        }
        self.rotors[right].step();
        Some(StepEvent {
            rotor: right,
            double,
        })
    }

    /// Encrypt (or decrypt — the machine is reciprocal) one character.
    /// Non-letters are rejected by returning `None`.
    pub fn encrypt_char(&mut self, c: char) -> Option<char> {
        let input = letter_to_idx(c)?;
        self.step();
        Some(idx_to_letter(self.encrypt_letter_no_step(input)))
    }

    /// Encrypt a letter with the current rotor positions, without stepping.
    fn encrypt_letter_no_step(&self, input: u8) -> u8 {
        let mut x = self.plug[input as usize];
        for rotor in self.rotors.iter().rev() {
            x = rotor.forward(x);
        }
        x = self.reflector.reflect(x);
        for rotor in self.rotors.iter() {
            x = rotor.backward(x);
        }
        self.plug[x as usize]
    }

    /// Encrypt a whole string, ignoring non-letters.
    pub fn encrypt_text(&mut self, text: &str) -> String {
        text.chars().filter_map(|c| self.encrypt_char(c)).collect()
    }

    pub fn positions(&self) -> Vec<char> {
        self.rotors.iter().map(|r| r.window_letter()).collect()
    }

    pub fn set_positions(&mut self, positions: &[char]) {
        for (rotor, &c) in self.rotors.iter_mut().zip(positions.iter()) {
            if let Some(i) = letter_to_idx(c) {
                rotor.position = i;
            }
        }
    }

    pub fn reset_positions(&mut self, positions: &[char]) {
        self.set_positions(positions);
    }

    /// Which rotors will move on the next key press (for the UI).
    pub fn armed(&self) -> Vec<bool> {
        let n = self.rotors.len();
        let mut armed = vec![false; n];
        if n < 2 {
            return armed;
        }
        armed[n - 1] = true;
        if self.rotors[n - 2].at_notch() || self.rotors[n - 1].at_notch() {
            armed[n - 2] = true;
        }
        if n >= 3 && self.rotors[n - 2].at_notch() {
            armed[n - 3] = true;
        }
        armed
    }

    pub fn plug_for(&self, letter: u8) -> Option<u8> {
        let out = self.plug[letter as usize];
        (out != letter).then_some(out)
    }

    /// Which plugboard pairs a letter participates in, if any.
    pub fn pair_of(&self, letter: u8) -> Option<(u8, u8)> {
        self.plug_pairs
            .iter()
            .copied()
            .find(|(a, b)| *a == letter || *b == letter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotor(name: &str, pos: char, ring: char) -> Rotor {
        Rotor::new(
            builtin_rotor(name).unwrap(),
            letter_to_idx(pos).unwrap(),
            letter_to_idx(ring).unwrap(),
        )
    }

    fn machine(rotors: &[&str], reflector: &str, plug: &[(char, char)]) -> Machine {
        let rotors = rotors
            .iter()
            .map(|n| rotor(n, 'A', 'A'))
            .collect::<Vec<_>>();
        let pairs = plug
            .iter()
            .map(|(a, b)| (letter_to_idx(*a).unwrap(), letter_to_idx(*b).unwrap()))
            .collect();
        Machine::new(rotors, Reflector::from_name(reflector).unwrap(), pairs).unwrap()
    }

    #[test]
    fn wiring_is_a_permutation() {
        for (name, wiring, _) in BUILTIN_ROTORS {
            validate_wiring(wiring).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        for (name, wiring) in REFLECTORS {
            validate_wiring(wiring).unwrap_or_else(|e| panic!("{name}: {e}"));
            // Reflectors must be involutions (self-inverse).
            let chars: Vec<char> = wiring.chars().collect();
            for (i, c) in chars.iter().enumerate() {
                let j = letter_to_idx(*c).unwrap() as usize;
                assert_eq!(
                    chars[j],
                    idx_to_letter(i as u8),
                    "reflector {name} is not reciprocal"
                );
            }
        }
    }

    #[test]
    fn known_vector_aaaaa() {
        // Classic published example: Enigma I, rotors I-II-III, reflector B,
        // rings A-A-A, windows A-A-A, no plugs -> "AAAAA" gives "BDZGO".
        let mut m = machine(&["I", "II", "III"], "B", &[]);
        assert_eq!(m.encrypt_text("AAAAA"), "BDZGO");
    }

    #[test]
    fn round_trip_is_reciprocal() {
        for reflector in ["B", "C"] {
            for trio in [
                ["I", "II", "III"],
                ["IV", "V", "VI"],
                ["VII", "VIII", "I"],
                ["Beta", "Gamma", "V"],
            ] {
                let mut m = machine(&trio, reflector, &[('A', 'V'), ('C', 'M')]);
                let plain = "THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG";
                let cipher = m.encrypt_text(plain);
                m.set_positions(&['A', 'A', 'A']);
                assert_eq!(m.encrypt_text(&cipher), plain);
            }
        }
    }

    #[test]
    fn round_trip_four_rotor() {
        for reflector in ["B-Thin", "C-Thin"] {
            let mut m = machine(&["Beta", "V", "VI", "VIII"], reflector, &[('B', 'Q')]);
            let cipher = m.encrypt_text("ENIGMAINTHETERMINAL");
            m.set_positions(&['A', 'A', 'A', 'A']);
            assert_eq!(m.encrypt_text(&cipher), "ENIGMAINTHETERMINAL");
        }
    }

    #[test]
    fn right_rotor_steps_every_letter() {
        let mut m = machine(&["I", "II", "III"], "B", &[]);
        assert_eq!(m.positions(), vec!['A', 'A', 'A']);
        m.encrypt_char('A');
        assert_eq!(m.positions(), vec!['A', 'A', 'B']);
        m.encrypt_char('A');
        assert_eq!(m.positions(), vec!['A', 'A', 'C']);
    }

    #[test]
    fn double_step_sequence() {
        // Rotor III (notch V) is fast; rotor II (notch E) drives the step of
        // rotor I. Starting A-D-U the middle rotor steps twice in a row when
        // the fast rotor wraps, which is the historical double step.
        let mut m = machine(&["I", "II", "III"], "B", &[]);
        m.set_positions(&['A', 'D', 'U']);
        m.encrypt_char('A'); // A-D-V
        assert_eq!(m.positions(), vec!['A', 'D', 'V']);
        m.encrypt_char('A'); // III at notch -> II steps: A-E-W
        assert_eq!(m.positions(), vec!['A', 'E', 'W']);
        m.encrypt_char('A'); // II at notch -> II and I step: B-F-X
        assert_eq!(m.positions(), vec!['B', 'F', 'X']);
        m.encrypt_char('A'); // II no longer at notch: B-F-Y
        assert_eq!(m.positions(), vec!['B', 'F', 'Y']);
    }

    #[test]
    fn four_rotor_greek_never_steps() {
        let mut m = machine(&["Beta", "I", "II", "III"], "B-Thin", &[]);
        m.set_positions(&['A', 'D', 'U', 'V']);
        let greek = m.rotors[0].window_letter();
        for _ in 0..200 {
            m.encrypt_char('A');
        }
        assert_eq!(m.rotors[0].window_letter(), greek);
    }

    #[test]
    fn ring_setting_shifts_wiring() {
        // With ring B and window A the rotor behaves like the A/A rotor with
        // its window one notch further along.
        let mut a = Machine::new(
            vec![
                rotor("I", 'A', 'A'),
                rotor("II", 'A', 'A'),
                rotor("III", 'A', 'A'),
            ],
            Reflector::from_name("B").unwrap(),
            vec![],
        )
        .unwrap();
        let mut b = Machine::new(
            vec![
                rotor("I", 'B', 'B'),
                rotor("II", 'B', 'B'),
                rotor("III", 'B', 'B'),
            ],
            Reflector::from_name("B").unwrap(),
            vec![],
        )
        .unwrap();
        // Both have offset 0 between window and ring, so the substitution is
        // identical even though the windows differ.
        assert_eq!(a.encrypt_char('Q'), b.encrypt_char('Q'));
        assert_ne!(a.positions(), b.positions());
    }

    #[test]
    fn plugboard_swaps_both_ways() {
        let m = machine(&["I", "II", "III"], "B", &[('A', 'B')]);
        assert_eq!(m.plug_for(0), Some(1));
        assert_eq!(m.plug_for(1), Some(0));
        assert_eq!(m.plug_for(2), None);
        assert_eq!(m.pair_of(1), Some((0, 1)));
    }

    #[test]
    fn rejects_invalid_specs() {
        assert!(RotorSpec::from_parts("X", "ABC", "").is_err());
        assert!(RotorSpec::from_parts("X", "AABCDEFGHIJKLMNOPQRSTUVWXY", "").is_err());
        assert!(RotorSpec::from_parts("", "ABCDEFGHIJKLMNOPQRSTUVWXYZ", "").is_err());
        assert!(RotorSpec::from_parts("X", "ABCDEFGHIJKLMNOPQRSTUVWXYZ", "Q").is_ok());
    }

    #[test]
    fn thin_reflector_requires_four_rotors() {
        let three = || {
            vec![
                rotor("I", 'A', 'A'),
                rotor("II", 'A', 'A'),
                rotor("III", 'A', 'A'),
            ]
        };
        assert!(Machine::new(three(), Reflector::from_name("B-Thin").unwrap(), vec![]).is_err());
        let four = || {
            vec![
                rotor("Beta", 'A', 'A'),
                rotor("I", 'A', 'A'),
                rotor("II", 'A', 'A'),
                rotor("III", 'A', 'A'),
            ]
        };
        assert!(Machine::new(four(), Reflector::from_name("B").unwrap(), vec![]).is_err());
        assert!(Machine::new(four(), Reflector::from_name("B-Thin").unwrap(), vec![]).is_ok());
    }
}
