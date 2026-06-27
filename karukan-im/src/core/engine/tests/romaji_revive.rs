//! Tests for reviving a stranded romaji prefix.
//!
//! When a consonant can't combine with the next key (`kp`) it is "baked" into
//! the buffer as a literal `k`; after editing back to that bare `k`, typing a
//! vowel must still complete it (`か`), not leave it stranded (`kあ`).

use super::*;

fn commit_text(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.clone()),
        _ => None,
    })
}

#[test]
fn test_stranded_consonant_combines_with_next_vowel() {
    let mut engine = InputMethodEngine::new();
    // `kpa` bakes the un-combinable `k` to a literal: preedit `kぱ`.
    engine.process_key(&press('k'));
    engine.process_key(&press('p'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "kぱ");

    // Delete `ぱ`, leaving the bare `k`.
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().unwrap().text(), "k");

    // Typing `a` now completes the revived `k` → `か` (was `kあ`).
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "か");

    // And it commits cleanly as `か`.
    let commit = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&commit).as_deref(), Some("か"));
}

#[test]
fn test_stranded_consonant_revives_into_youon_prefix() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('k'));
    engine.process_key(&press('p'));
    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().unwrap().text(), "k");

    // `k` then `y` rebuilds the `ky` prefix, then `a` → `きゃ`.
    engine.process_key(&press('y'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "きゃ");
}

#[test]
fn test_normal_typing_is_unaffected_by_revive() {
    // Plain syllable.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "か");

    // Youon in one go.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('k'));
    engine.process_key(&press('y'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "きゃ");

    // A short sentence still converts to the expected kana.
    let mut engine = InputMethodEngine::new();
    for c in "watashi".chars() {
        engine.process_key(&press(c));
    }
    assert_eq!(engine.preedit().unwrap().text(), "わたし");
}

#[test]
fn test_revive_does_not_disturb_digits() {
    // A digit before the cursor is not an ASCII letter, so it is never revived;
    // the following syllable still converts normally.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('2'));
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "2か");
}
