//! Cross-checked against Py-Enigma 1.0.2 (an independent implementation).
//!
//! Each line: rotors, reflector, rings, positions, plugboard, plaintext, ciphertext
//! (fields separated by tabs, rotors left to right).

use renigma::enigma::{Machine, Reflector, Rotor, builtin_rotor, letter_to_idx};

fn letter(s: &str, i: usize) -> u8 {
    letter_to_idx(s.split_whitespace().nth(i).unwrap().chars().next().unwrap()).unwrap()
}

#[test]
fn matches_py_enigma_vectors() {
    let data = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/vectors.tsv"));
    let mut checked = 0;
    for (lineno, line) in data.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 7, "line {}: bad field count", lineno + 1);
        let rotor_names: Vec<&str> = f[0].split_whitespace().collect();
        let mut rotors = Vec::new();
        for (i, name) in rotor_names.iter().enumerate() {
            let spec = builtin_rotor(name).unwrap_or_else(|| panic!("unknown rotor {name}"));
            rotors.push(Rotor::new(spec, letter(f[3], i), letter(f[2], i)));
        }
        let reflector = Reflector::from_name(f[1]).unwrap();
        let plugs: Vec<(u8, u8)> = f[4]
            .split_whitespace()
            .map(|p| {
                let mut c = p.chars();
                (
                    letter_to_idx(c.next().unwrap()).unwrap(),
                    letter_to_idx(c.next().unwrap()).unwrap(),
                )
            })
            .collect();
        let mut machine = Machine::new(rotors, reflector, plugs).unwrap();
        let got = machine.encrypt_text(f[5]);
        assert_eq!(got, f[6], "line {}: {} {}", lineno + 1, f[0], f[1]);
        checked += 1;
    }
    assert!(checked >= 19, "expected all vectors, checked {checked}");
}
