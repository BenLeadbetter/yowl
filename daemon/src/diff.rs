//! Text diffing and tracking for streaming transcription.
//!
//! This module handles the complexity of tracking text as it evolves during
//! live transcription with a rolling audio buffer. Key challenges:
//!
//! 1. **Text revision**: Whisper may revise recent text as more audio context arrives
//! 2. **Text aging**: As audio ages out of the rolling buffer, that text disappears
//!    from the transcript and must be "committed" (locked in)
//!
//! The solution uses two-tier tracking:
//! - `committed`: Text that has aged out - never revised via backspaces
//! - `provisional`: Text we've sent but may still revise

/// Result of computing a diff between old and new text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    /// Number of characters to backspace/erase
    pub backspaces: usize,
    /// New characters to append after backspacing
    pub new_text: String,
}

/// Tracks text state for streaming transcription output.
#[derive(Debug, Default)]
pub struct TextTracker {
    /// Text that has aged out of the rolling buffer - locked in, never backspace into this
    committed: String,
    /// Text we've sent but may still revise via backspaces
    provisional: String,
}

impl TextTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset all state (call when starting a new recording).
    pub fn reset(&mut self) {
        self.committed.clear();
        self.provisional.clear();
    }

    /// Update with a new transcript and compute the diff to send.
    ///
    /// Returns `None` if no output is needed (empty transcript, no changes).
    pub fn update(&mut self, new_transcript: &str) -> Option<DiffResult> {
        if new_transcript.is_empty() && self.provisional.is_empty() {
            return None;
        }

        // Step 1: Detect aging - find where new_transcript "picks up" in our provisional text
        let aging_point = self.find_aging_point(new_transcript);

        if aging_point > 0 {
            // Text before aging_point has aged out - commit it
            let to_commit: String = self.provisional.chars().take(aging_point).collect();
            self.committed.push_str(&to_commit);
            self.provisional = self.provisional.chars().skip(aging_point).collect();
        }

        // Step 2: Diff new_transcript against remaining provisional text
        let common_len = self
            .provisional
            .chars()
            .zip(new_transcript.chars())
            .take_while(|(a, b)| a == b)
            .count();

        let backspaces = self.provisional.chars().count() - common_len;
        let new_text: String = new_transcript.chars().skip(common_len).collect();

        // Step 3: Update provisional to match new transcript
        self.provisional = new_transcript.to_string();

        // Only return a result if there's something to do
        if backspaces > 0 || !new_text.is_empty() {
            Some(DiffResult {
                backspaces,
                new_text,
            })
        } else {
            None
        }
    }

    /// Get the full text that has been output (committed + provisional).
    pub fn full_text(&self) -> String {
        format!("{}{}", self.committed, self.provisional)
    }

    /// Get just the committed (locked-in) text.
    pub fn committed(&self) -> &str {
        &self.committed
    }

    /// Get just the provisional (revisable) text.
    pub fn provisional(&self) -> &str {
        &self.provisional
    }

    /// Find how many characters from the start of provisional have "aged out".
    ///
    /// AGING vs REVISION:
    /// - AGING: Audio buffer shifted forward, new transcript starts mid-way in our text
    /// - REVISION: Whisper just changed its mind about what was said
    ///
    /// We only detect aging when we have HIGH CONFIDENCE that the start of
    /// new_transcript matches somewhere in provisional. Otherwise, we treat it
    /// as a revision (return 0, let the diff handle it with backspaces).
    fn find_aging_point(&self, new_transcript: &str) -> usize {
        if self.provisional.is_empty() || new_transcript.is_empty() {
            return 0;
        }

        // If texts share a common prefix, nothing has aged
        if new_transcript.starts_with(&self.provisional)
            || self.provisional.starts_with(new_transcript)
        {
            return 0;
        }

        // For aging detection, we need the START of new_transcript to appear
        // somewhere AFTER the start of provisional. We require a long match
        // to be confident this is aging vs just similar words.
        let new_chars: Vec<char> = new_transcript.chars().collect();
        let min_match_len = 15; // Require at least 15 chars to match

        if new_chars.len() < min_match_len {
            // New transcript too short to confidently detect aging
            return 0;
        }

        // Try different prefix lengths of new_transcript
        for key_len in (min_match_len..=new_chars.len().min(40)).rev() {
            let search_key: String = new_chars[..key_len].iter().collect();

            if let Some(byte_pos) = self.provisional.find(&search_key) {
                if byte_pos > 0 {
                    // Found a match after the start - this is aging
                    // Everything before the match point has aged out
                    return self.provisional[..byte_pos].chars().count();
                }
            }
        }

        // No confident aging detected - treat as revision
        // The diff will handle it with backspaces
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_text() {
        let mut tracker = TextTracker::new();

        let result = tracker.update("Hello").unwrap();
        assert_eq!(result.backspaces, 0);
        assert_eq!(result.new_text, "Hello");
        assert_eq!(tracker.full_text(), "Hello");
    }

    #[test]
    fn test_append_text() {
        let mut tracker = TextTracker::new();

        tracker.update("Hello").unwrap();
        let result = tracker.update("Hello world").unwrap();

        assert_eq!(result.backspaces, 0);
        assert_eq!(result.new_text, " world");
        assert_eq!(tracker.full_text(), "Hello world");
    }

    #[test]
    fn test_revise_end() {
        let mut tracker = TextTracker::new();

        tracker.update("Hello worl").unwrap();
        let result = tracker.update("Hello world").unwrap();

        // Should backspace 0 and add "d" since "Hello worl" is prefix of "Hello world"
        assert_eq!(result.backspaces, 0);
        assert_eq!(result.new_text, "d");
    }

    #[test]
    fn test_revise_with_backspace() {
        let mut tracker = TextTracker::new();

        tracker.update("Hello word").unwrap();
        let result = tracker.update("Hello world").unwrap();

        // "Hello wor" is common, need to backspace "d" and add "ld"
        assert_eq!(result.backspaces, 1);
        assert_eq!(result.new_text, "ld");
    }

    #[test]
    fn test_revision_not_aging() {
        // When Whisper completely changes its mind, we should revise (backspace)
        // not commit the old garbage
        let mut tracker = TextTracker::new();

        tracker.update("The three billi-e-outs.").unwrap();
        let result = tracker.update("The Three Billy Goats Gruff.").unwrap();

        // These are different transcriptions of the same audio
        // We should backspace and replace, NOT commit the old text
        // Common prefix is "The " (4 chars)
        // Backspace count should be len("three billi-e-outs.") = 19
        assert!(
            result.backspaces > 0,
            "Should backspace the wrong text, got backspaces={}",
            result.backspaces
        );

        // After this, full_text should only contain ONE attempt, not both
        assert!(
            !tracker.full_text().contains("billi-e-outs"),
            "Should not contain old incorrect text. Got: {}",
            tracker.full_text()
        );
    }

    #[test]
    fn test_no_change() {
        let mut tracker = TextTracker::new();

        tracker.update("Hello").unwrap();
        let result = tracker.update("Hello");

        assert!(result.is_none());
    }

    #[test]
    fn test_empty_to_empty() {
        let mut tracker = TextTracker::new();
        let result = tracker.update("");
        assert!(result.is_none());
    }

    #[test]
    fn test_reset() {
        let mut tracker = TextTracker::new();

        tracker.update("Hello world").unwrap();
        tracker.reset();

        assert_eq!(tracker.full_text(), "");
        assert_eq!(tracker.committed(), "");
        assert_eq!(tracker.provisional(), "");
    }

    // Tests for aging behavior
    #[test]
    fn test_simple_aging() {
        let mut tracker = TextTracker::new();

        // Initial: "Once upon a time there was"
        tracker.update("Once upon a time there was").unwrap();

        // Now the buffer aged and we only get "a time there was a king"
        // The "Once upon " should be committed
        let result = tracker.update("a time there was a king").unwrap();

        // "Once upon " (11 chars) aged out and should be committed
        // We should see backspaces for what changed and new text
        assert!(tracker.committed().starts_with("Once upon "));

        // The output should make sense
        println!("Committed: {:?}", tracker.committed());
        println!("Provisional: {:?}", tracker.provisional());
        println!("Result: {:?}", result);
    }

    #[test]
    fn test_aging_preserves_head() {
        let mut tracker = TextTracker::new();

        // Build up text over several updates
        tracker.update("The three").unwrap();
        tracker.update("The three billy").unwrap();
        tracker.update("The three billy goats").unwrap();
        tracker.update("The three billy goats gruff").unwrap();

        // Now simulate aging: buffer only has latter part
        tracker.update("billy goats gruff once upon").unwrap();

        // "The three " should be committed
        assert!(
            tracker.committed().contains("The three"),
            "Should preserve 'The three' in committed. Got: {:?}",
            tracker.committed()
        );

        // Full text should still have it
        assert!(
            tracker.full_text().contains("The three"),
            "Full text should contain 'The three'. Got: {:?}",
            tracker.full_text()
        );
    }

    #[test]
    fn test_gradual_aging() {
        let mut tracker = TextTracker::new();

        // Simulate a real transcription session with gradual aging
        let updates = vec![
            "The",
            "The three",
            "The three billy",
            "The three billy goats",
            "The three billy goats gruff",
            "The three billy goats gruff.",
            "three billy goats gruff. Once", // "The " aged out
            "billy goats gruff. Once upon",  // "three " aged out
            "goats gruff. Once upon a",      // "billy " aged out
            "gruff. Once upon a time",       // "goats " aged out
        ];

        let mut terminal_text = String::new();

        for update in updates {
            if let Some(result) = tracker.update(update) {
                // Simulate terminal: backspace then append
                for _ in 0..result.backspaces {
                    terminal_text.pop();
                }
                terminal_text.push_str(&result.new_text);
            }
            println!(
                "After '{}': terminal='{}', committed='{}'",
                update,
                terminal_text,
                tracker.committed()
            );
        }

        // The terminal should have the complete text
        assert!(
            terminal_text.contains("The"),
            "Terminal should still have 'The'. Got: {}",
            terminal_text
        );
        assert!(
            terminal_text.contains("gruff. Once upon a time"),
            "Terminal should have ending. Got: {}",
            terminal_text
        );
    }

    #[test]
    fn test_whisper_style_revisions() {
        let mut tracker = TextTracker::new();

        // Simulate Whisper's tendency to add periods then revise
        let updates = vec![
            "Once upon a time.",
            "Once upon a time there.",
            "Once upon a time there was.",
            "Once upon a time there was a", // period removed, text added
            "Once upon a time there was a bridge",
        ];

        let mut terminal_text = String::new();

        for update in updates {
            if let Some(result) = tracker.update(update) {
                for _ in 0..result.backspaces {
                    terminal_text.pop();
                }
                terminal_text.push_str(&result.new_text);
                println!("bs={}, new='{}' -> '{}'", result.backspaces, result.new_text, terminal_text);
            }
        }

        assert_eq!(terminal_text, "Once upon a time there was a bridge");
    }

    #[test]
    fn test_no_duplicate_output() {
        let mut tracker = TextTracker::new();

        // This reproduces a bug pattern from the user's output
        let updates = vec![
            "The three billi",
            "The three billy",
            "The three billy goats",
            "The three billy goats gruff",
            "three billy goats gruff. Once",      // aging
            "three billy goats gruff. Once upon", // more text
        ];

        let mut terminal_text = String::new();

        for update in updates {
            if let Some(result) = tracker.update(update) {
                for _ in 0..result.backspaces {
                    terminal_text.pop();
                }
                terminal_text.push_str(&result.new_text);
            }
        }

        // Should not have duplicate "three billy goats gruff"
        let count = terminal_text.matches("billy goats gruff").count();
        assert_eq!(
            count, 1,
            "Should have exactly one 'billy goats gruff'. Got {} in: {}",
            count, terminal_text
        );
    }

    #[test]
    fn test_whisper_inconsistent_transcripts() {
        // This reproduces the user's actual bug - Whisper gives inconsistent
        // transcriptions as it struggles with the audio
        let mut tracker = TextTracker::new();

        let updates = vec![
            "The three billi-e-outs.",
            "The Three Billy Oats Gruff.",
            "The three billiote's gruff.",
            "The three billiote's gruff. Once upon a time there was a bridge",
            "billiote's gruff. Once upon a time there was a bridge and beneath that bridge", // aging
            "gruff. Once upon a time there was a bridge and beneath that bridge lived", // more aging
        ];

        let mut terminal_text = String::new();

        for update in &updates {
            if let Some(result) = tracker.update(update) {
                for _ in 0..result.backspaces {
                    terminal_text.pop();
                }
                terminal_text.push_str(&result.new_text);
            }
            println!(
                "Update: '{}'\n  -> Terminal: '{}'\n  -> Committed: '{}'\n",
                update,
                terminal_text,
                tracker.committed()
            );
        }

        // Should NOT have duplicate fragments
        let count = terminal_text.matches("Once upon a time").count();
        assert_eq!(
            count, 1,
            "Should have exactly one 'Once upon a time'. Got {} in:\n{}",
            count, terminal_text
        );
    }

    #[test]
    fn test_short_string_revision() {
        // Fix for the test_complete_revision failure
        let mut tracker = TextTracker::new();

        tracker.update("Helo").unwrap();
        let result = tracker.update("Hello").unwrap();

        // "Hel" is common, backspace "o", add "lo"
        assert_eq!(result.backspaces, 1, "Should backspace the wrong 'o'");
        assert_eq!(result.new_text, "lo", "Should add 'lo'");
    }
}
