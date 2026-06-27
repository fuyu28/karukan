//! Tests for the F6–F10 function-key conversions
//! (ひらがな / 全角カナ / 半角カナ / 全角英数 / 半角英数).
//!
//! These reshape the *reading* with pure rule-based transforms (no model
//! call), so they exercise only the engine state machine.

use super::*;

/// Extract the committed text from a result, if any.
fn commit_text(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.clone()),
        _ => None,
    })
}

/// Description shown on the selected candidate (the mozc-style right-side
/// comment), if any.
fn selected_description(engine: &InputMethodEngine) -> Option<String> {
    engine
        .candidates()?
        .selected()
        .and_then(|c| c.description.clone())
}

#[test]
fn test_f7_converts_composing_reading_to_full_katakana() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.preedit().unwrap().text(), "あい");

    // F7 → full-width katakana, entering Conversion state (instant, no commit).
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(result.consumed);
    assert!(
        commit_text(&result).is_none(),
        "F7 must not commit by itself"
    );
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "アイ");
    assert_eq!(
        selected_description(&engine).as_deref(),
        Some("[全]カタカナ")
    );

    // Enter commits the katakana form.
    let commit = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&commit).as_deref(), Some("アイ"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_f8_converts_to_half_width_katakana() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    engine.process_key(&press_key(Keysym::F8));
    assert_eq!(engine.preedit().unwrap().text(), "ｱｲ");
    assert_eq!(
        selected_description(&engine).as_deref(),
        Some("[半]カタカナ")
    );
}

#[test]
fn test_f6_reshapes_reading_back_to_hiragana() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // F7 → katakana, then F6 → hiragana again. F-keys always reshape the
    // preserved reading, not the previously selected form.
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    engine.process_key(&press_key(Keysym::F6));
    assert_eq!(engine.preedit().unwrap().text(), "あい");
    assert_eq!(
        selected_description(&engine).as_deref(),
        Some("[全]ひらがな")
    );
}

#[test]
fn test_f_keys_work_from_conversion_state() {
    // F7 enters Conversion; a second F-key (F8) is dispatched by the
    // conversion-state handler and reshapes the same reading.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    engine.process_key(&press_key(Keysym::F7));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    engine.process_key(&press_key(Keysym::F8));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "ｱｲ");
}

#[test]
fn test_f7_candidate_list_holds_all_forms_navigable_by_space() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    engine.process_key(&press_key(Keysym::F7));
    // The list is the distinct F6→F10 forms in order; for a kana reading the
    // alnum shapes are no-ops and dedup away, leaving hiragana/full/half kana.
    let texts: Vec<String> = engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert_eq!(texts, vec!["あい", "アイ", "ｱｲ"]);
    // F7's form (full katakana) is selected, not the first entry.
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    // Space steps to the next form (half-width katakana).
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(engine.preedit().unwrap().text(), "ｱｲ");
}

#[test]
fn test_f9_converts_alphabet_reading_to_full_width_then_commits() {
    let mut engine = InputMethodEngine::new();
    // Alphabet mode: Shift+I capitalizes, then lower-case s, o → "Iso".
    engine.process_key(&press_shift('I'));
    engine.process_key(&press('s'));
    engine.process_key(&press('o'));
    assert!(engine.input_mode == InputMode::Alphabet);
    assert_eq!(engine.preedit().unwrap().text(), "Iso");

    // F9 → full-width alphanumerics.
    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(engine.preedit().unwrap().text(), "Ｉｓｏ");
    assert_eq!(selected_description(&engine).as_deref(), Some("[全]英数"));

    // Commit reverts the transient alphabet mode back to hiragana.
    let commit = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&commit).as_deref(), Some("Ｉｓｏ"));
    assert!(engine.input_mode != InputMode::Alphabet);
}

#[test]
fn test_f10_reshapes_reading_to_half_width_alnum() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('I'));
    engine.process_key(&press('s'));
    engine.process_key(&press('o'));

    // F9 → full-width, then F10 reshapes the *reading* ("Iso") back to
    // half-width (the F9 result is not the source — the reading is).
    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(engine.preedit().unwrap().text(), "Ｉｓｏ");

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().unwrap().text(), "Iso");
    assert_eq!(selected_description(&engine).as_deref(), Some("[半]英数"));
}

#[test]
fn test_f_key_in_empty_state_is_not_consumed() {
    let mut engine = InputMethodEngine::new();
    // Nothing to convert → pass the key through to the application.
    let result = engine.process_key(&press_key(Keysym::F7));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_escape_after_f_key_on_consonant_ending_alphabet_reading() {
    // Regression: F-key → Conversion → Escape must restore the latin reading
    // verbatim. `cancel_conversion` re-feeds the reading through the romaji
    // converter; for a latin acronym ending in a consonant ("Jis") that
    // buffers the trailing "s" and corrupts the preedit/commit into "Jiss".
    // F9/F10 make this path (alphabet acronym → Conversion → Escape) routine.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('J'));
    engine.process_key(&press('i'));
    engine.process_key(&press('s'));
    assert_eq!(engine.preedit().unwrap().text(), "Jis");

    engine.process_key(&press_key(Keysym::F9));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Escape returns to composing with the reading intact (not "Jiss").
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "Jis");

    // And committing yields the original reading, uncorrupted.
    let commit = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&commit).as_deref(), Some("Jis"));
}
