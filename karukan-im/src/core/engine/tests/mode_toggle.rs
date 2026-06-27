use super::*;

// --- Mode toggle key tests (one-way: alphabet → hiragana) ---

#[test]
fn test_mode_toggle_key_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A (composition still active — committing
    // now auto-reverts to Hiragana, so test the mid-composition switch instead).
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Alt_R press → switch to hiragana mode mid-composition.
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);

    // Type 'a' → appended as hiragana after the existing "A".
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "Aあ");
}

#[test]
fn test_hiragana_key_switches_katakana_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Type "ai" → "あい", then Ctrl+K → katakana mode.
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert!(engine.input_mode == InputMode::Katakana);
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    // The JIS かな key returns to hiragana, baking the katakana in.
    let result = engine.process_key(&press_key(Keysym::HIRAGANA));
    assert!(result.consumed);
    assert!(engine.input_mode == InputMode::Hiragana);
    assert_eq!(engine.input_buf.text, "アイ");
}

#[test]
fn test_hiragana_key_noop_in_hiragana_is_consumed() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.input_mode == InputMode::Hiragana);

    // The かな key while already in hiragana is swallowed (no stray keysym
    // reaches the app) and leaves the mode unchanged.
    let result = engine.process_key(&press_key(Keysym::HIRAGANA));
    assert!(result.consumed);
    assert!(engine.input_mode == InputMode::Hiragana);
}

#[test]
fn test_mode_toggle_key_noop_in_hiragana() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.input_mode != InputMode::Alphabet);

    // Alt_R press in hiragana mode → not consumed, no mode change
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(!result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);

    // Type 'a' → still hiragana
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_mode_toggle_key_during_alphabet_input() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A and type "b"
    engine.process_key(&press_shift('A'));
    engine.process_key(&press('b'));
    assert_eq!(engine.preedit().unwrap().text(), "Ab");
    assert!(engine.input_mode == InputMode::Alphabet);

    // Alt_R → switch to hiragana
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);

    // Continue typing → hiragana
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "Abか");
}

#[test]
fn test_super_r_also_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Super_R press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::SUPER_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);
}

#[test]
fn test_meta_r_also_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);

    // Meta_R press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::META_R));
    assert!(result.consumed);
    assert!(engine.input_mode != InputMode::Alphabet);
}
