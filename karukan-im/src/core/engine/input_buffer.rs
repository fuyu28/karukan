//! InputBuffer: composed input as a sequence of `Unit { raw, kana }`.
//!
//! Each [`Unit`] pairs the kana it contributes to the preedit with the *raw*
//! keystrokes that produced it (e.g. `Unit { raw: "shi", kana: "し" }`). This
//! lets the F9/F10 function-key conversions recover the originally-typed romaji
//! even after edits (see `convert_to_shape`). A literal insert (alphabet/emoji/
//! space) stores `raw == kana`.
//!
//! `cursor_pos` is a **kana-char index** (not a unit index, not bytes) so the
//! display and cursor contract is unchanged. Structural edits (`insert` /
//! `remove_*`) first split whichever unit the operation boundary bisects into
//! singleton (`raw == kana`) units, so an edit only ever loses raw recovery for
//! the specific mora it actually splits; pure cursor movement never mutates
//! units and so never loses raw.

/// One input unit: the kana shown in the preedit plus the raw keystrokes that
/// produced it. For literal input (alphabet/emoji/space) `raw == kana`.
struct Unit {
    // Wired in P2 (converter raw tracking) / P3 (F9/F10 raw recovery).
    #[allow(dead_code)]
    raw: String,
    kana: String,
}

impl Unit {
    fn singleton(c: char) -> Self {
        Self {
            raw: c.to_string(),
            kana: c.to_string(),
        }
    }
}

/// Composed input buffer with cursor.
pub(super) struct InputBuffer {
    /// Source of truth: the ordered units. `text()`/`raw_text()` derive from it.
    units: Vec<Unit>,
    /// Cursor position (in kana characters, not bytes, not unit indices).
    pub cursor_pos: usize,
}

impl InputBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            units: Vec::new(),
            cursor_pos: 0,
        }
    }

    /// Clear the buffer (units, cursor).
    pub fn clear(&mut self) {
        self.units.clear();
        self.cursor_pos = 0;
    }

    /// Composed kana text (concatenation of every unit's kana). This is the
    /// "source of truth" the rest of the engine reads; it replaces the former
    /// `text` field.
    pub fn text(&self) -> String {
        self.units.iter().map(|u| u.kana.as_str()).collect()
    }

    /// Concatenation of every unit's raw keystrokes — the originally-typed
    /// input, used by F9/F10 to recover romaji. (Wired in P3.)
    #[allow(dead_code)]
    pub fn raw_text(&self) -> String {
        self.units.iter().map(|u| u.raw.as_str()).collect()
    }

    /// Total kana-char length.
    fn kana_len(&self) -> usize {
        self.units.iter().map(|u| u.kana.chars().count()).sum()
    }

    /// Insert literal text at the cursor (raw == kana, one unit per char).
    /// Used by the alphabet/emoji/space paths and tests.
    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let idx = self.split_at_kana_boundary(self.cursor_pos);
        let new: Vec<Unit> = text.chars().map(Unit::singleton).collect();
        let count = new.len();
        self.units.splice(idx..idx, new);
        self.cursor_pos += count;
    }

    /// Insert a single unit (raw → kana) at the cursor. `kana` may be multiple
    /// characters (e.g. a youon like `きゃ`); it stays one atomic unit.
    pub fn insert_unit(&mut self, raw: &str, kana: &str) {
        if kana.is_empty() {
            return;
        }
        let idx = self.split_at_kana_boundary(self.cursor_pos);
        self.units.insert(
            idx,
            Unit {
                raw: raw.to_string(),
                kana: kana.to_string(),
            },
        );
        self.cursor_pos += kana.chars().count();
    }

    /// Remove the kana character at the given kana-char position.
    pub fn remove_char_at(&mut self, char_pos: usize) -> Option<char> {
        if char_pos >= self.kana_len() {
            return None;
        }
        // Isolate the single kana char at `char_pos` into its own unit, then
        // drop it. The splits degrade any straddled multi-char unit to
        // singletons (raw lost for that mora only).
        self.split_at_kana_boundary(char_pos + 1);
        let idx = self.split_at_kana_boundary(char_pos);
        Some(self.units.remove(idx).kana.chars().next().unwrap())
    }

    /// Remove the character before the cursor.
    pub fn remove_char_before_cursor(&mut self) -> Option<char> {
        if self.cursor_pos == 0 {
            return None;
        }
        self.cursor_pos -= 1;
        self.remove_char_at(self.cursor_pos)
    }

    /// Remove the character at the cursor position (delete key).
    pub fn remove_char_at_cursor(&mut self) -> Option<char> {
        self.remove_char_at(self.cursor_pos)
    }

    /// Ensure a unit boundary exists at kana-char position `kana_pos`, splitting
    /// (and degrading to singleton `raw == kana` units) whichever unit it
    /// bisects. Returns the units-vec index where `kana_pos` begins.
    fn split_at_kana_boundary(&mut self, kana_pos: usize) -> usize {
        let mut acc = 0; // kana chars before unit i
        let mut i = 0;
        while i < self.units.len() {
            if kana_pos == acc {
                return i;
            }
            let len = self.units[i].kana.chars().count();
            if kana_pos < acc + len {
                // `kana_pos` bisects unit i — degrade it into singletons so a
                // boundary exists at every kana char.
                let unit = self.units.remove(i);
                for (k, c) in unit.kana.chars().enumerate() {
                    self.units.insert(i + k, Unit::singleton(c));
                }
                return i + (kana_pos - acc);
            }
            acc += len;
            i += 1;
        }
        // At or beyond the end.
        self.units.len()
    }
}
