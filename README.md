# Renigma

A terminal Enigma machine simulator in Rust — historical rotor wirings,
correct stepping (including the middle-rotor double step), a live lampboard,
and a plugboard you can wire up from the keyboard.

```
RENIGMA    ENCRYPT   CONFIGURE  3-rotor                       Catppuccin Mocha  ● saved
╭ Reflector B ────────────────────────────────────────────────────────────────────────────╮
│                                  B   YRUHQSLDPXNGOKMIEBFZCWVJAT                         │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
╭──────── 1 · I ────────╮╭─────── 2 · II ────────╮╭─────── 3 · III ────────╮
│    ring A  notch Q    ││    ring A  notch E    ││    ring A  notch V     │
│       ┌─────┐         ││       ┌─────┐         ││       ┌─────┐          │
│       │  A  │         ││       │  A  │         ││       │  K  │          │
│       └─────┘         ││       └─────┘         ││       └─────┘          │
│    middle · holds     ││    middle · holds     ││      fast · steps      │
│ EKMFLGDQVZNTOWYHXUSPAIBRCJ ││ AJDKSIRUXBLHWTMCQGZNPYFVOE ││ BDFHJLCPRTXVZNYEIWGAKMUSQO │
╰───────────────────────╯╰───────────────────────╯╰────────────────────────╯
```

## Build and run

```sh
cargo build --release
./target/release/renigma
```

Options: `--config <path>` to use a different config file, `--help`, `--version`.

Requires a true-colour terminal (24-bit RGB themes); it works in a plain
80×24 terminal but looks best a bit wider.

## Keys

Encryption mode (the default):

| Key | Action |
| --- | --- |
| `A`–`Z` | Encipher the letter: the rotors step, the lamp lights |
| `Backspace` | Undo the last letter and rewind the rotors exactly |
| `Ctrl-R` | Reset the rotors to the configured starting positions |
| `Tab` / `F2` | Open configuration |
| `?` / `F1` | Help overlay |
| `Ctrl-S` | Save the config now |
| `Ctrl-Q`, `Esc` | Quit (settings are saved on exit) |

Configuration mode:

| Key | Action |
| --- | --- |
| `Tab`, `Shift-Tab`, `1`–`6` | Switch section |
| `↑` `↓` / `k` `j` | Select a row |
| `←` `→` / `h` `l` | Change the selected value |
| `Enter` | Apply, or connect a plug |
| `n` / `e` / `d` | New / edit / delete a custom rotor |
| `[` `]` | Ring setting down / up (Rotor section) |
| `a`–`z` | Set the window letter directly (Rotor section) |
| `x` | Clear the plugboard (Plugboard section) |
| `Shift-R` | Reset all machine settings to defaults |
| `Esc` | Back to encryption |

Changes are written to the config file automatically (debounced by two
seconds) and on exit, so a session survives a reboot.

## Configuration

Default location: `$XDG_CONFIG_HOME/renigma/config.toml`, else
`~/.config/renigma/config.toml`. A first run writes a commented default.

```toml
theme = "catppuccin-mocha"
reflector = "B"
plugboard = ["AV", "BS", "CG"]

[rotors]
count = 3
order = ["I", "II", "III"]    # left to right, rightmost is fast
positions = ["A", "A", "A"]   # window letters
rings = ["A", "A", "A"]       # ring settings

[[custom_rotors]]
name = "My rotor"
wiring = "BACDEFGHIJKLMNOPQRSTUVWXYZ"
notches = "Q"
```

* `order` may hold any of `I`–`VIII`, `Beta`, `Gamma`, or a custom rotor name.
* `Beta`/`Gamma` are the non-stepping Greek rotors: they are only valid in the
  leftmost slot of a 4-rotor M4, which also requires the `B-Thin`/`C-Thin`
  reflector. A 3-rotor machine uses `B` or `C`.
* `plugboard` holds up to 13 pairs; every letter may appear at most once.
* `count` must be 3 or 4.

Invalid or conflicting values are repaired at load time (and reported in the
status line) rather than crashing: unknown rotors are replaced, duplicate
letters in the plugboard dropped, mismatched generators corrected.

### Themes

Neutral (default), Catppuccin Mocha, Catppuccin Latte, Dracula, Nord, Gruvbox
Dark. A theme recolours the entire UI — borders, text, highlights, lamps and
rotor windows.

### Custom rotors

Two ways: create one in the UI (`6 Custom` → `n`) or add a `[[custom_rotors]]`
block to the config. The wiring must be a permutation of A–Z written in
A→Z order (position 0's output, position 1's output, …), and notches are 0–2
letters. They are saved back to the config as normal rotor definitions and can
be placed in any slot like a historical rotor.

## Historical accuracy

* Rotor wirings and turnover notches are the standard published ones
  (I–V for Enigma I, VI–VIII with Z/M double notches, Beta/Gamma for M4).
* Stepping follows the real pawl mechanism: the fast rotor always steps; when
  the middle rotor sits on its notch it steps itself *and* the left rotor
  (the double step); otherwise a fast-rotor notch carries the middle rotor.
  The M4's Greek rotor never steps.
* Ring settings offset the wiring inside the rotor (they do not move the
  window), exactly as on the real machine.
* Encryption is reciprocal: type the ciphertext back with the same settings to
  recover the plaintext.

The core is verified against an independent implementation (Py-Enigma 1.0.2):
`testdata/vectors.tsv` holds 19 configurations — 3- and 4-rotor, all four
reflectors, varied ring/window settings and plugboards — and
`tests/vectors.rs` asserts byte-for-byte agreement. Unit tests in
`src/enigma.rs` cover the double step, notch behaviour, reciprocity and the
A/A-A startup vector (`AAAAA` → `BDZGO`).

## Layout

```
src/enigma.rs   pure machine: rotors, reflector, plugboard, stepping
src/config.rs   TOML load/save, validation and repair
src/theme.rs    colour themes
src/app.rs      state machine and key handling
src/ui.rs       ratatui rendering
tests/          golden vectors and headless UI/flow tests
```

The core has no dependency on ratatui, so it can be driven from other front
ends (`renigma::enigma::Machine`).

## License

MIT — see [LICENSE](LICENSE). Copyright (c) 2026 Michael Graham.

