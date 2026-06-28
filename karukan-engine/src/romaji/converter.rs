use super::rules::build_rules;
use super::trie::TrieNode;
use crate::kana::hiragana_to_katakana;

/// Events that can occur during conversion
#[derive(Debug, Clone, PartialEq)]
pub enum ConversionEvent {
    /// Characters were converted to hiragana
    Converted(String),
    /// Character added to buffer, waiting for more input
    Buffered,
    /// No conversion rule found, character passed through
    PassThrough(char),
}

/// Result of a backspace operation
#[derive(Debug, Clone, PartialEq)]
pub enum BackspaceResult {
    /// Removed from output
    RemovedOutput(char),
    /// Removed from buffer
    RemovedBuffer(char),
    /// Nothing to remove
    Empty,
}

/// Romaji to Hiragana converter with state management
#[derive(Debug)]
pub struct RomajiConverter {
    trie: TrieNode,
    buffer: String,
    output: String,
    /// Per-conversion (raw, kana) pairs, accumulated in lockstep with `output`
    /// (concatenating every unit's kana always equals `output`). Lets callers
    /// recover the originally-typed romaji for each kana segment — used to build
    /// `InputBuffer` units so F9/F10 can convert back to the raw input. Read as
    /// a delta (units since a captured length), mirroring how `output` is read.
    units: Vec<(String, String)>,
}

impl RomajiConverter {
    /// Create a new converter with default rules
    pub fn new() -> Self {
        Self {
            trie: build_rules(),
            buffer: String::new(),
            output: String::new(),
            units: Vec::new(),
        }
    }

    /// Record a (raw, kana) unit and append its kana to `output` in one step,
    /// keeping the two in lockstep. `raw` is the consumed keystrokes; capture it
    /// from `buffer` *before* draining.
    fn record_unit(&mut self, raw: String, kana: &str) {
        self.output.push_str(kana);
        self.units.push((raw, kana.to_string()));
    }

    /// Push a character and attempt conversion
    pub fn push(&mut self, ch: char) -> ConversionEvent {
        // Handle uppercase by converting to lowercase
        let ch = ch.to_ascii_lowercase();

        // Add to buffer
        self.buffer.push(ch);

        // Try to convert
        self.try_convert()
    }

    /// Convert with the given hiragana and recursively process any remaining buffer.
    /// Returns a Converted event combining the hiragana with any further conversions.
    fn convert_with_remainder(&mut self, hiragana: String) -> ConversionEvent {
        if !self.buffer.is_empty()
            && let ConversionEvent::Converted(next) = self.try_convert()
        {
            return ConversionEvent::Converted(format!("{}{}", hiragana, next));
        }
        ConversionEvent::Converted(hiragana)
    }

    /// Try to convert the current buffer
    fn try_convert(&mut self) -> ConversionEvent {
        // Special case: "nn" + another character
        // "nn" is ALWAYS treated as a single ん, regardless of what follows.
        // This matches IME behavior where "nn" is the deliberate way to enter ん.
        // Examples:
        // - "nna" -> "んa" (nn -> ん, a continues in buffer)
        // - "nni" -> "んi" (nn -> ん, i continues in buffer)
        // - "nnk" -> "んk" (nn -> ん, k continues in buffer)
        let chars: Vec<char> = self.buffer.chars().collect();
        let char_count = chars.len();
        if char_count >= 3 && chars[0] == 'n' && chars[1] == 'n' {
            // "nn" is always a single ん, rest is processed separately
            self.buffer.drain(..2);
            self.record_unit("nn".to_string(), "ん");
            return self.convert_with_remainder("ん".to_string());
        }

        // Special case: 'n' before consonant -> ん
        if char_count >= 2 {
            let last = chars[char_count - 1];
            let second_last = chars[char_count - 2];

            // N before consonant rule: 'n' + consonant (including 'n') -> ん + consonant
            // Exception: exactly "nn" (length 2) should wait for next char
            if second_last == 'n'
                && !matches!(last, 'a' | 'i' | 'u' | 'e' | 'o' | 'y' | '\'')
                && !(char_count == 2 && last == 'n')
            // Exclude exactly "nn"
            {
                // Convert the 'n' at position len-2 to 'ん'
                // Keep everything before that position plus the last character
                let prefix: String = chars.iter().take(char_count - 2).collect();
                self.buffer = format!("{}{}", prefix, last);
                // Only the single `n` at position len-2 is consumed into ん.
                self.record_unit("n".to_string(), "ん");
                return self.convert_with_remainder("ん".to_string());
            }

            // Double consonant rule: same consonant twice (except 'n') -> っ + consonant
            if last == second_last && !matches!(last, 'a' | 'i' | 'u' | 'e' | 'o' | 'n') {
                // Convert to sokuon and keep the last consonant. The consumed
                // raw is the first of the doubled consonant (e.g. `kka` → っ
                // from the first `k`).
                self.buffer = last.to_string();
                self.record_unit(last.to_string(), "っ");
                return ConversionEvent::Converted("っ".to_string());
            }
        }

        // Search for longest match. Own the matched kana (and copy the small
        // fields) so the trie borrow ends here — `record_unit` needs `&mut self`.
        let search = self.trie.search_longest(&self.buffer);
        let matched_len = search.matched_len;
        let has_continuation = search.has_continuation;
        let matched_output = search.output.map(str::to_string);

        if let Some(hiragana) = matched_output {
            // Found a match
            if has_continuation && matched_len == self.buffer.len() {
                // This is a valid conversion, but there might be longer matches
                // Wait for more input unless it's "n'" or "nn"
                if self.buffer == "n'" || self.buffer == "nn" {
                    // Special case: always convert n' and nn immediately. The
                    // whole buffer is consumed as this unit's raw.
                    let raw = self.buffer.clone();
                    self.record_unit(raw, &hiragana);
                    self.buffer.clear();
                    return ConversionEvent::Converted(hiragana);
                }
                // Otherwise, wait for more input
                return ConversionEvent::Buffered;
            } else {
                // Convert and keep remainder in buffer. The consumed raw is the
                // matched prefix (romaji is ASCII, so bytes == chars).
                let raw = self.buffer[..matched_len].to_string();
                self.record_unit(raw, &hiragana);
                self.buffer.drain(..matched_len);
                return self.convert_with_remainder(hiragana);
            }
        } else if matched_len == 0 {
            // No match at all
            // Check if the first character could start a valid conversion
            let Some(first_char) = self.buffer.chars().next() else {
                return ConversionEvent::Buffered;
            };
            let first_char_has_children = self.trie.children.contains_key(&first_char);

            if first_char_has_children {
                // Check if the current buffer could still lead to a match
                // by walking the trie to see if we're on a valid path
                let mut node = &self.trie;
                let mut on_valid_path = true;
                for ch in self.buffer.chars() {
                    if let Some(child) = node.children.get(&ch) {
                        node = child;
                    } else {
                        on_valid_path = false;
                        break;
                    }
                }

                if on_valid_path {
                    // We're on a valid path in the trie, keep buffering
                    return ConversionEvent::Buffered;
                }
            }

            // First character doesn't start any rule, or buffer is not on valid path
            let first_search = self.trie.search_longest(&first_char.to_string());
            let first_len = first_search.matched_len;
            let first_output = first_search.output.map(str::to_string);

            if let Some(hiragana) = first_output {
                // First character has a valid conversion, use it
                let raw = self.buffer[..first_len].to_string();
                self.record_unit(raw, &hiragana);
                self.buffer.drain(..first_len);
                return self.convert_with_remainder(hiragana);
            } else {
                // No possible match, pass through the first character (raw == kana)
                self.buffer.remove(0);
                self.record_unit(first_char.to_string(), &first_char.to_string());

                // Try to convert remainder after pass-through
                if !self.buffer.is_empty() {
                    let next_event = self.try_convert();
                    match next_event {
                        ConversionEvent::Converted(_) | ConversionEvent::PassThrough(_) => {
                            return next_event;
                        }
                        _ => {}
                    }
                }

                return ConversionEvent::PassThrough(first_char);
            }
        }

        ConversionEvent::Buffered
    }

    /// Flush remaining buffer by converting what we can
    pub fn flush(&mut self) -> String {
        let mut result = String::new();

        while !self.buffer.is_empty() {
            let search = self.trie.search_longest(&self.buffer);
            let matched_len = search.matched_len;
            let matched_output = search.output.map(str::to_string);

            if let Some(h) = matched_output {
                result.push_str(&h);
                let raw = self.buffer[..matched_len].to_string();
                self.record_unit(raw, &h);
                self.buffer.drain(..matched_len);
            } else if let Some(ch) = self.buffer.chars().next() {
                // No conversion rule matched. A lone `n` at flush time is the
                // moraic nasal ん: nothing more is coming that could turn it into
                // `na`/`ni`/…, so it can only be ん (this is why `hon` commits as
                // ほん, not ほn). Every other bare consonant has no standalone
                // kana and passes through literally. Raw is the original char.
                let mapped = if ch == 'n' { 'ん' } else { ch };
                result.push(mapped);
                self.record_unit(ch.to_string(), &mapped.to_string());
                self.buffer.remove(0);
            }
        }

        result
    }

    /// Handle backspace
    pub fn backspace(&mut self) -> BackspaceResult {
        if let Some(ch) = self.buffer.pop() {
            BackspaceResult::RemovedBuffer(ch)
        } else if let Some(ch) = self.output.pop() {
            // Keep `units` in lockstep with `output`: drop the last kana char
            // from the trailing unit, removing the unit if it empties. Raw for
            // that mora degrades to kana. (Unreachable from the engine, which
            // only calls backspace while the buffer is non-empty, but keeps the
            // public-API invariant `concat(units.kana) == output` intact.)
            if let Some(last) = self.units.last_mut() {
                let mut chars = last.1.chars();
                chars.next_back();
                let rest: String = chars.collect();
                if rest.is_empty() {
                    self.units.pop();
                } else {
                    last.1 = rest.clone();
                    last.0 = rest;
                }
            }
            BackspaceResult::RemovedOutput(ch)
        } else {
            BackspaceResult::Empty
        }
    }

    /// Get the current output
    pub fn output(&self) -> &str {
        &self.output
    }

    /// Get the current output converted to katakana
    pub fn output_katakana(&self) -> String {
        hiragana_to_katakana(&self.output)
    }

    /// Get the current buffer (unconverted input)
    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Accumulated (raw, kana) units. Read as a delta: capture `units().len()`
    /// before a `push`/`flush`, then take everything from that index onward.
    pub fn units(&self) -> &[(String, String)] {
        &self.units
    }

    /// Reset the converter state
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.output.clear();
        self.units.clear();
    }

    /// Get both output and buffer as a single string
    pub fn full_text(&self) -> String {
        format!("{}{}", self.output, self.buffer)
    }

    /// Get both output and buffer as a single string, with output converted to katakana
    pub fn full_text_katakana(&self) -> String {
        format!("{}{}", hiragana_to_katakana(&self.output), self.buffer)
    }
}

impl Default for RomajiConverter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collect the converter's (raw, kana) units as borrowed pairs for terse
    /// assertions.
    fn units(conv: &RomajiConverter) -> Vec<(&str, &str)> {
        conv.units()
            .iter()
            .map(|(r, k)| (r.as_str(), k.as_str()))
            .collect()
    }

    fn push_str(conv: &mut RomajiConverter, s: &str) {
        for c in s.chars() {
            conv.push(c);
        }
    }

    #[test]
    fn test_units_basic_match() {
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "ka");
        assert_eq!(units(&conv), vec![("ka", "か")]);
    }

    #[test]
    fn test_units_sokuon_raw_is_first_consonant() {
        // `kka` → っ from the first `k`, then か from `ka`.
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "kka");
        assert_eq!(units(&conv), vec![("k", "っ"), ("ka", "か")]);
        assert_eq!(conv.output(), "っか");
    }

    #[test]
    fn test_units_nn_raw_is_nn() {
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "nn");
        assert_eq!(units(&conv), vec![("nn", "ん")]);
    }

    #[test]
    fn test_units_n_before_consonant_raw_is_single_n() {
        // `nka` → ん from a single `n`, then か from `ka`.
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "nka");
        assert_eq!(units(&conv), vec![("n", "ん"), ("ka", "か")]);
        assert_eq!(conv.output(), "んか");
    }

    #[test]
    fn test_units_youon_is_one_unit() {
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "kya");
        assert_eq!(units(&conv), vec![("kya", "きゃ")]);
    }

    #[test]
    fn test_units_punctuation_keeps_typed_raw() {
        // `!` has a trie rule to full-width `！`; the typed `!` is kept as raw so
        // F10 can recover it.
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "a!");
        assert_eq!(units(&conv), vec![("a", "あ"), ("!", "！")]);
    }

    #[test]
    fn test_units_lone_n_flush_raw_is_n() {
        let mut conv = RomajiConverter::new();
        conv.push('n');
        conv.flush();
        assert_eq!(units(&conv), vec![("n", "ん")]);
    }

    #[test]
    fn test_units_kana_concat_equals_output() {
        // Invariant: concatenating every unit's kana reproduces `output`.
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "konnnichiha");
        let joined: String = conv.units().iter().map(|(_, k)| k.as_str()).collect();
        assert_eq!(joined, conv.output());
        assert_eq!(conv.output(), "こんにちは");
    }

    #[test]
    fn test_units_reset_clears() {
        let mut conv = RomajiConverter::new();
        push_str(&mut conv, "ka");
        assert!(!conv.units().is_empty());
        conv.reset();
        assert!(conv.units().is_empty());
    }

    #[test]
    fn test_basic_conversion() {
        let mut conv = RomajiConverter::new();
        conv.push('k');
        conv.push('a');
        assert_eq!(conv.output(), "か");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_buffering() {
        let mut conv = RomajiConverter::new();
        let result = conv.push('k');
        assert_eq!(result, ConversionEvent::Buffered);
        assert_eq!(conv.buffer(), "k");
    }

    #[test]
    fn test_sokuon() {
        let mut conv = RomajiConverter::new();
        conv.push('k');
        conv.push('k');
        assert_eq!(conv.output(), "っ");
        assert_eq!(conv.buffer(), "k");

        conv.push('a');
        assert_eq!(conv.output(), "っか");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_n_context() {
        let mut conv = RomajiConverter::new();
        conv.push('n');
        assert_eq!(conv.buffer(), "n"); // Wait for context

        conv.push('a');
        assert_eq!(conv.output(), "な");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_nn() {
        let mut conv = RomajiConverter::new();

        // Test "nn" - should convert immediately to ん
        conv.push('n');
        assert_eq!(conv.buffer(), "n"); // First 'n' is buffered
        conv.push('n');
        assert_eq!(conv.buffer(), ""); // Buffer cleared after conversion
        assert_eq!(conv.output(), "ん"); // Immediately converted to ん

        // Test "nni" - should produce "んい" (nn -> ん immediately, i -> い)
        conv.reset();
        "nni".chars().for_each(|c| {
            conv.push(c);
        });
        assert_eq!(conv.output(), "んい");

        // Test "nna" - should produce "んあ" (nn -> ん immediately, a -> あ)
        conv.reset();
        "nna".chars().for_each(|c| {
            conv.push(c);
        });
        assert_eq!(conv.output(), "んあ");

        // Test "nnk" - should produce "んk" (nn -> ん immediately, k buffered)
        conv.reset();
        "nnk".chars().for_each(|c| {
            conv.push(c);
        });
        assert_eq!(conv.output(), "ん");
        assert_eq!(conv.buffer(), "k");
    }

    #[test]
    fn test_youon() {
        let mut conv = RomajiConverter::new();
        "kya".chars().for_each(|c| {
            conv.push(c);
        });
        assert_eq!(conv.output(), "きゃ");
    }

    #[test]
    fn test_flush() {
        let mut conv = RomajiConverter::new();
        conv.push('k');
        assert_eq!(conv.buffer(), "k");

        let flushed = conv.flush();
        assert_eq!(flushed, "k");
        assert_eq!(conv.output(), "k");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_flush_lone_n_becomes_kana() {
        // A trailing single `n` is the moraic nasal: flushing it yields ん.
        let mut conv = RomajiConverter::new();
        conv.push('n');
        assert_eq!(conv.buffer(), "n");

        let flushed = conv.flush();
        assert_eq!(flushed, "ん");
        assert_eq!(conv.output(), "ん");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_flush_word_ending_in_n() {
        // "hon": ho -> ほ, trailing n flushes to ん -> ほん (not ほn).
        let mut conv = RomajiConverter::new();
        for c in "hon".chars() {
            conv.push(c);
        }
        conv.flush();
        assert_eq!(conv.output(), "ほん");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_flush_n_before_leftover_yoon_prefix() {
        // "ny" is an incomplete youon prefix; flushing maps n -> ん and leaves
        // the dangling y as a literal: んy.
        let mut conv = RomajiConverter::new();
        conv.push('n');
        conv.push('y');
        assert_eq!(conv.buffer(), "ny");
        let flushed = conv.flush();
        assert_eq!(flushed, "んy");
    }

    #[test]
    fn test_backspace() {
        let mut conv = RomajiConverter::new();
        conv.push('k');
        conv.push('a');
        assert_eq!(conv.output(), "か");

        conv.push('k');
        assert_eq!(conv.buffer(), "k");

        let result = conv.backspace();
        assert_eq!(result, BackspaceResult::RemovedBuffer('k'));
        assert_eq!(conv.buffer(), "");

        let result = conv.backspace();
        assert_eq!(result, BackspaceResult::RemovedOutput('か'));
    }

    #[test]
    fn test_full_sentence() {
        let mut conv = RomajiConverter::new();
        // IME style: "nn" is always ん, so こんにちは requires 3 n's: "konnnichiha"
        // (ko -> こ, nn -> ん, ni -> に, chi -> ち, ha -> は)
        let input = "konnnichiha";
        for ch in input.chars() {
            conv.push(ch);
        }
        assert_eq!(conv.output(), "こんにちは");
    }

    #[test]
    fn test_punctuation_passthrough() {
        let mut conv = RomajiConverter::new();
        // Test that punctuation passes through and conversion continues after
        let input = "kokohadoko?watashihadare?";
        for ch in input.chars() {
            conv.push(ch);
        }
        assert_eq!(conv.output(), "ここはどこ？わたしはだれ？");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_mixed_punctuation() {
        let mut conv = RomajiConverter::new();
        let input = "a!b?c";
        for ch in input.chars() {
            conv.push(ch);
        }
        // 'c' stays in buffer because it could start 'ca', 'chi', etc.
        assert_eq!(conv.output(), "あ！b？");
        assert_eq!(conv.buffer(), "c");

        // After flush, 'c' passes through
        conv.flush();
        assert_eq!(conv.output(), "あ！b？c");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_watashiha() {
        let mut conv = RomajiConverter::new();
        let input = "kokohadoko?watashiha?";
        for ch in input.chars() {
            conv.push(ch);
        }
        assert_eq!(conv.output(), "ここはどこ？わたしは？");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_punctuation_then_youon() {
        let mut conv = RomajiConverter::new();
        // a?b?cya should become あ？b？ちゃ
        // 'c' must stay in buffer after '?' until 'ya' completes 'cya'
        let input = "a?b?cya";
        for ch in input.chars() {
            conv.push(ch);
        }
        assert_eq!(conv.output(), "あ？b？ちゃ");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_output_katakana() {
        let mut conv = RomajiConverter::new();
        "watashi".chars().for_each(|c| {
            conv.push(c);
        });
        // "watash" → "わたし" with "i" still possible as part of "shi" etc.
        // Actually: w→buffered, wa→わ, t→buffered, ta→た, s→buffered, sh→buffered, shi→し
        assert_eq!(conv.output(), "わたし");
        assert_eq!(conv.output_katakana(), "ワタシ");
        assert_eq!(conv.buffer(), "");
    }

    #[test]
    fn test_full_text_katakana() {
        let mut conv = RomajiConverter::new();
        // "kak" → か + k(buffered)
        "kak".chars().for_each(|c| {
            conv.push(c);
        });
        assert_eq!(conv.output(), "か");
        assert_eq!(conv.buffer(), "k");
        assert_eq!(conv.full_text_katakana(), "カk");
    }
}
