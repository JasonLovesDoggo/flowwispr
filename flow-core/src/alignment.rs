//! Wispr-style word alignment using Needleman-Wunsch with custom scoring
//!
//! This module implements the exact alignment algorithm used by Wispr Flow
//! to detect user edits and extract correction candidates.

use serde::{Deserialize, Serialize};

/// Word edit labels (matches Wispr's edit vector encoding)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WordLabel {
    /// M - exact match (words are identical)
    Match,
    /// S - substitution (different word)
    Substitution,
    /// I - insertion (word added in edited text)
    Insert,
    /// D - deletion (word removed from original)
    Delete,
    /// C - casing difference only (same word, different case)
    Casing,
    /// Z - empty/whitespace-only
    None,
    /// E - edge case detection error (boundary artifacts)
    EditCaptureError,
}

impl WordLabel {
    /// Convert to single-character representation for edit vector
    pub fn as_char(&self) -> char {
        match self {
            Self::Match => 'M',
            Self::Substitution => 'S',
            Self::Insert => 'I',
            Self::Delete => 'D',
            Self::Casing => 'C',
            Self::None => 'Z',
            Self::EditCaptureError => 'E',
        }
    }

    /// Parse from single character
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'M' => Some(Self::Match),
            'S' => Some(Self::Substitution),
            'I' => Some(Self::Insert),
            'D' => Some(Self::Delete),
            'C' => Some(Self::Casing),
            'Z' => Some(Self::None),
            'E' => Some(Self::EditCaptureError),
            _ => None,
        }
    }
}

/// A single step in the alignment result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentStep {
    /// Label for the word comparison
    pub word_label: WordLabel,
    /// Label for punctuation comparison
    pub punct_label: WordLabel,
    /// Original word (empty for insertions)
    pub original_word: String,
    /// Edited word (empty for deletions)
    pub edited_word: String,
}

/// Result of alignment operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignmentResult {
    /// All alignment steps
    pub steps: Vec<AlignmentStep>,
    /// Word edit vector string (e.g., "MMSMMD")
    pub word_edit_vector: String,
    /// Punctuation edit vector string
    pub punct_edit_vector: String,
    /// Extracted correction candidates (original, corrected)
    pub corrections: Vec<(String, String)>,
}

/// Strip punctuation from word, keeping only alphanumeric + spaces
fn strip_punctuation(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

/// Extract only punctuation from word
fn extract_punctuation(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_alphanumeric() && !c.is_whitespace())
        .collect()
}

/// Strip leading/trailing punctuation from word
fn strip_leading_trailing_punct(s: &str) -> String {
    s.trim_matches(|c: char| !c.is_alphanumeric() && !c.is_whitespace())
        .to_string()
}

/// Compute word label comparing original vs edited word
fn compute_word_label(original: Option<&str>, edited: Option<&str>) -> WordLabel {
    let orig = strip_punctuation(original.unwrap_or(""));
    let edit = strip_punctuation(edited.unwrap_or(""));

    if orig == edit {
        if orig.is_empty() {
            WordLabel::None
        } else {
            WordLabel::Match
        }
    } else if orig.to_lowercase() == edit.to_lowercase() {
        WordLabel::Casing
    } else if orig.is_empty() && !edit.is_empty() {
        WordLabel::Insert
    } else if !orig.is_empty() && edit.is_empty() {
        WordLabel::Delete
    } else {
        WordLabel::Substitution
    }
}

/// Compute punctuation label comparing original vs edited
fn compute_punct_label(original: Option<&str>, edited: Option<&str>) -> WordLabel {
    let orig = extract_punctuation(original.unwrap_or(""));
    let edit = extract_punctuation(edited.unwrap_or(""));

    if orig == edit {
        if orig.is_empty() {
            WordLabel::None
        } else {
            WordLabel::Match
        }
    } else if !orig.is_empty() && edit.is_empty() {
        WordLabel::Delete
    } else if orig.is_empty() && !edit.is_empty() {
        WordLabel::Insert
    } else {
        WordLabel::Substitution
    }
}

/// Normalized Levenshtein distance (0.0 = identical, 1.0 = completely different)
fn normalized_edit_distance(a: &str, b: &str) -> f64 {
    if a == b {
        return 0.0;
    }
    if a.is_empty() || b.is_empty() {
        return 1.0;
    }

    // Ensure a is the longer string for efficiency
    let (a, b) = if a.len() < b.len() { (b, a) } else { (a, b) };
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0; b_chars.len() + 1];

    for i in 1..=a_chars.len() {
        curr[0] = i;
        for j in 1..=b_chars.len() {
            curr[j] = if a_chars[i - 1] == b_chars[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j].min(curr[j - 1]).min(prev[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_chars.len()] as f64 / a_chars.len().max(b_chars.len()) as f64
}

/// Build the linear score matrix (Needleman-Wunsch with word-level edit distance)
///
/// Wispr uses substitution_cost = 4 * normalized_edit_distance
/// This makes substitution more expensive than ins/del for dissimilar words.
pub fn linear_score_matrix(
    original: &str,
    edited: &str,
    sub_cost_multiplier: f64,
) -> Vec<Vec<f64>> {
    let orig_words: Vec<&str> = original.split_whitespace().collect();
    let edit_words: Vec<&str> = edited.split_whitespace().collect();

    let orig_stripped: Vec<String> = orig_words
        .iter()
        .map(|w| strip_punctuation(w).to_lowercase())
        .collect();
    let edit_stripped: Vec<String> = edit_words
        .iter()
        .map(|w| strip_punctuation(w).to_lowercase())
        .collect();

    let m = orig_stripped.len();
    let n = edit_stripped.len();

    let mut matrix = vec![vec![0.0; n + 1]; m + 1];

    // Initialize first column (deletions)
    for (i, row) in matrix.iter_mut().enumerate().take(m + 1) {
        row[0] = i as f64;
    }
    // Initialize first row (insertions)
    for (j, val) in matrix[0].iter_mut().enumerate() {
        *val = j as f64;
    }

    // Fill matrix using dynamic programming
    for i in 1..=m {
        for j in 1..=n {
            if orig_stripped[i - 1] == edit_stripped[j - 1] {
                // Exact match (case-insensitive, punctuation-stripped)
                matrix[i][j] = matrix[i - 1][j - 1];
            } else {
                // Substitution cost scales with how different the words are
                let sub_cost =
                    normalized_edit_distance(&orig_stripped[i - 1], &edit_stripped[j - 1])
                        * sub_cost_multiplier;
                matrix[i][j] = (matrix[i - 1][j] + 1.0) // deletion
                    .min(matrix[i][j - 1] + 1.0) // insertion
                    .min(matrix[i - 1][j - 1] + sub_cost); // substitution
            }
        }
    }

    matrix
}

/// Backtrack through score matrix to get detailed alignment steps
pub fn backtrack_alignment(
    matrix: &[Vec<f64>],
    original: &str,
    edited: &str,
) -> Vec<AlignmentStep> {
    let orig_words: Vec<&str> = original.split_whitespace().collect();
    let edit_words: Vec<&str> = edited.split_whitespace().collect();

    let m = orig_words.len();
    let n = edit_words.len();

    let mut steps = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && strip_punctuation(orig_words[i - 1]).to_lowercase()
                == strip_punctuation(edit_words[j - 1]).to_lowercase()
        {
            // Match or casing difference
            let word_label = compute_word_label(Some(orig_words[i - 1]), Some(edit_words[j - 1]));
            let punct_label = compute_punct_label(Some(orig_words[i - 1]), Some(edit_words[j - 1]));
            steps.push(AlignmentStep {
                word_label,
                punct_label,
                original_word: strip_leading_trailing_punct(orig_words[i - 1]),
                edited_word: strip_leading_trailing_punct(edit_words[j - 1]),
            });
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || matrix[i][j - 1] < matrix[i - 1][j]) {
            // Insertion (word added in edited)
            let word_label = compute_word_label(None, Some(edit_words[j - 1]));
            let punct_label = compute_punct_label(None, Some(edit_words[j - 1]));
            steps.push(AlignmentStep {
                word_label,
                punct_label,
                original_word: String::new(),
                edited_word: strip_leading_trailing_punct(edit_words[j - 1]),
            });
            j -= 1;
        } else if i > 0 && (j == 0 || matrix[i - 1][j] < matrix[i][j - 1]) {
            // Deletion (word removed from original)
            let word_label = compute_word_label(Some(orig_words[i - 1]), None);
            let punct_label = compute_punct_label(Some(orig_words[i - 1]), None);
            steps.push(AlignmentStep {
                word_label,
                punct_label,
                original_word: strip_leading_trailing_punct(orig_words[i - 1]),
                edited_word: String::new(),
            });
            i -= 1;
        } else if i > 0 && j > 0 {
            // Substitution
            let mut word_label =
                compute_word_label(Some(orig_words[i - 1]), Some(edit_words[j - 1]));
            let punct_label = compute_punct_label(Some(orig_words[i - 1]), Some(edit_words[j - 1]));

            // Wispr edge case: single-char substitution at boundaries might be capture error
            if (i == m || i == 1)
                && word_label == WordLabel::Substitution
                && edit_words[j - 1].len() == 1
            {
                let orig = orig_words[i - 1];
                let edit = edit_words[j - 1];
                if orig.starts_with(edit) || orig.ends_with(edit) {
                    word_label = WordLabel::EditCaptureError;
                }
            }

            steps.push(AlignmentStep {
                word_label,
                punct_label,
                original_word: strip_leading_trailing_punct(orig_words[i - 1]),
                edited_word: strip_leading_trailing_punct(edit_words[j - 1]),
            });
            i -= 1;
            j -= 1;
        } else {
            // Shouldn't reach here, but handle gracefully
            break;
        }
    }

    steps.reverse();
    steps
}

/// Generate edit vector string from alignment steps
pub fn edit_vector(steps: &[AlignmentStep]) -> String {
    steps.iter().map(|s| s.word_label.as_char()).collect()
}

/// Generate punctuation edit vector string from alignment steps
pub fn punct_edit_vector(steps: &[AlignmentStep]) -> String {
    steps.iter().map(|s| s.punct_label.as_char()).collect()
}

// Pattern matching for substitution detection (replaces Wispr regex)
// We use simple iteration since Rust's regex doesn't support lookahead

/// Check if a character is a "context" character (M, C, or Z)
fn is_context_char(c: char) -> bool {
    matches!(c, 'M' | 'C' | 'Z')
}

/// Find isolated single substitutions (user corrected one word)
///
/// Matches Wispr's exact pattern: /(?=([CMZ]S[CMZ]|^S[CMZ]|[CMZ]S$))/g
/// - [CMZ]S[CMZ] - substitution surrounded by context chars
/// - ^S[CMZ] - substitution at start, requires context char after
/// - [CMZ]S$ - substitution at end, requires context char before
///
/// Note: Does NOT match lone S (^S$) - requires at least one context char for confidence
pub fn find_isolated_substitutions(edit_vector: &str, steps: &[AlignmentStep]) -> Vec<usize> {
    let chars: Vec<char> = edit_vector.chars().collect();
    let len = chars.len();
    let mut indices = Vec::new();

    for (i, &c) in chars.iter().enumerate() {
        if c != 'S' {
            continue;
        }

        // Check for isolated substitution patterns (must have at least one context char)
        let has_prev_context = i > 0 && is_context_char(chars[i - 1]);
        let has_next_context = i + 1 < len && is_context_char(chars[i + 1]);
        let at_start = i == 0;
        let at_end = i == len - 1;

        // Match: [CMZ]S[CMZ] (surrounded by context)
        // Match: ^S[CMZ] (start + context after)
        // Match: [CMZ]S$ (context before + end)
        // Does NOT match: ^S$ (no context at all)
        //
        // Simplified: need context on at least one side, and if at boundary,
        // the non-boundary side must have context
        let is_isolated = match (at_start, at_end) {
            (true, true) => false,             // ^S$ - no context, reject
            (true, false) => has_next_context, // ^S... - need context after
            (false, true) => has_prev_context, // ...S$ - need context before
            (false, false) => has_prev_context && has_next_context, // ...S... - need both
        };

        if is_isolated && i < steps.len() {
            indices.push(i);
        }
    }

    indices
}

/// Find deletion-substitution patterns (merged/split words)
///
/// Matches patterns like:
/// - [CMZ](DS|SD)[CMZ] - deletion+substitution surrounded by context
/// - ^(DS|SD)[CMZ] - del+sub at start
/// - [CMZ](DS|SD)$ - del+sub at end
pub fn find_deletion_substitutions(edit_vector: &str, steps: &[AlignmentStep]) -> Vec<usize> {
    let chars: Vec<char> = edit_vector.chars().collect();
    let len = chars.len();
    let mut indices = Vec::new();

    for i in 0..len {
        // Look for DS pattern
        if chars[i] == 'D' && i + 1 < len && chars[i + 1] == 'S' {
            let prev_ok = i == 0 || is_context_char(chars[i - 1]);
            let next_ok = i + 2 >= len || is_context_char(chars[i + 2]);

            if prev_ok && next_ok && i + 1 < steps.len() {
                indices.push(i + 1); // Return the S index
            }
        }
        // Look for SD pattern
        else if chars[i] == 'S' && i + 1 < len && chars[i + 1] == 'D' {
            let prev_ok = i == 0 || is_context_char(chars[i - 1]);
            let next_ok = i + 2 >= len || is_context_char(chars[i + 2]);

            if prev_ok && next_ok && i < steps.len() {
                indices.push(i); // Return the S index
            }
        }
    }

    indices
}

/// Extract correction candidates from alignment
pub fn extract_corrections(steps: &[AlignmentStep]) -> Vec<(String, String)> {
    let vector = edit_vector(steps);

    let mut corrections = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Isolated substitutions (highest confidence)
    for idx in find_isolated_substitutions(&vector, steps) {
        let step = &steps[idx];
        let key = step.edited_word.to_lowercase();
        if !seen.contains(&key) && !step.edited_word.is_empty() && !step.original_word.is_empty() {
            seen.insert(key);
            corrections.push((step.original_word.clone(), step.edited_word.clone()));
        }
    }

    // Deletion-substitution patterns
    for idx in find_deletion_substitutions(&vector, steps) {
        let step = &steps[idx];
        let key = step.edited_word.to_lowercase();
        if !seen.contains(&key) && !step.edited_word.is_empty() && !step.original_word.is_empty() {
            seen.insert(key);
            corrections.push((step.original_word.clone(), step.edited_word.clone()));
        }
    }

    corrections
}

/// Main entry point: Parse alignment steps (matches Wispr's parseAlignmentSteps)
pub fn parse_alignment_steps(original: &str, edited: &str) -> AlignmentResult {
    // Wispr uses substitution cost multiplier of 4
    let matrix = linear_score_matrix(original, edited, 4.0);
    let steps = backtrack_alignment(&matrix, original, edited);
    let word_vec = edit_vector(&steps);
    let punct_vec = punct_edit_vector(&steps);
    let corrections = extract_corrections(&steps);

    AlignmentResult {
        steps,
        word_edit_vector: word_vec,
        punct_edit_vector: punct_vec,
        corrections,
    }
}

/// Align two texts and return the result as JSON (for FFI)
pub fn align_and_extract_corrections_json(original: &str, edited: &str) -> String {
    let result = parse_alignment_steps(original, edited);
    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_substitution() {
        let result = parse_alignment_steps("I work at anthorpic", "I work at Anthropic");

        assert_eq!(result.word_edit_vector, "MMMS");
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].0, "anthorpic");
        assert_eq!(result.corrections[0].1, "Anthropic");
    }

    #[test]
    fn test_multiple_substitutions() {
        let result = parse_alignment_steps("I recieve teh mail", "I receive the mail");

        assert_eq!(result.word_edit_vector, "MSSM");
        // Adjacent substitutions (SS) are not "isolated" - they need context chars on both sides
        // This is intentional: we want high-confidence single corrections, not bulk changes
        assert_eq!(result.corrections.len(), 0);
    }

    #[test]
    fn test_insertion() {
        let result = parse_alignment_steps("hello world", "hello beautiful world");

        assert!(result.word_edit_vector.contains('I'));
    }

    #[test]
    fn test_deletion() {
        let result = parse_alignment_steps("hello big world", "hello world");

        assert!(result.word_edit_vector.contains('D'));
    }

    #[test]
    fn test_casing_only() {
        let result = parse_alignment_steps("hello world", "Hello World");

        assert_eq!(result.word_edit_vector, "CC");
        assert!(result.corrections.is_empty()); // Casing changes aren't corrections
    }

    #[test]
    fn test_no_changes() {
        let result = parse_alignment_steps("hello world", "hello world");

        assert_eq!(result.word_edit_vector, "MM");
        assert!(result.corrections.is_empty());
    }

    #[test]
    fn test_punctuation_tracking() {
        let result = parse_alignment_steps("hello world", "hello, world!");

        // Words should match, punctuation should show changes
        assert_eq!(result.word_edit_vector, "MM");
    }

    #[test]
    fn test_normalized_edit_distance() {
        assert_eq!(normalized_edit_distance("hello", "hello"), 0.0);
        assert_eq!(normalized_edit_distance("", "hello"), 1.0);
        assert!(normalized_edit_distance("hello", "hallo") < 0.5);
        assert!(normalized_edit_distance("cat", "dog") > 0.5);
    }

    #[test]
    fn test_isolated_substitution_pattern() {
        // Pattern: word before, substitution, word after
        let result = parse_alignment_steps("the quikc fox", "the quick fox");

        assert_eq!(result.word_edit_vector, "MSM");
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].0, "quikc");
        assert_eq!(result.corrections[0].1, "quick");
    }

    #[test]
    fn test_json_output() {
        let json = align_and_extract_corrections_json("teh cat", "the cat");
        let parsed: AlignmentResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.word_edit_vector, "SM");
        assert_eq!(parsed.corrections.len(), 1);
    }

    #[test]
    fn test_empty_input() {
        let result = parse_alignment_steps("", "hello");
        assert_eq!(result.word_edit_vector, "I");

        let result = parse_alignment_steps("hello", "");
        assert_eq!(result.word_edit_vector, "D");

        let result = parse_alignment_steps("", "");
        assert!(result.word_edit_vector.is_empty());
    }

    #[test]
    fn test_proper_noun_correction() {
        // Classic Wispr use case: misspelled proper noun
        let result =
            parse_alignment_steps("I talked to john yesterday", "I talked to John yesterday");

        assert_eq!(result.word_edit_vector, "MMMCM");
        // Casing changes are NOT extracted as corrections (they're intentional style)
        assert!(result.corrections.is_empty());
    }

    #[test]
    fn test_company_name_correction() {
        // Misspelled company name should be detected
        let result = parse_alignment_steps("I use chatgtp daily", "I use ChatGPT daily");

        assert_eq!(result.word_edit_vector, "MMSM");
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].0, "chatgtp");
        assert_eq!(result.corrections[0].1, "ChatGPT");
    }

    #[test]
    fn test_deduplication() {
        // Same correction appearing multiple times should be deduped
        let result = parse_alignment_steps("teh cat and teh dog", "the cat and the dog");

        // Both "teh" -> "the" should be detected but deduped
        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].1, "the");
    }

    #[test]
    fn test_unicode_words() {
        let result = parse_alignment_steps("café résumé", "cafe resume");

        // Should handle accented characters gracefully
        assert_eq!(result.word_edit_vector, "SS");
    }

    #[test]
    fn test_hyphenated_words() {
        let result = parse_alignment_steps("self employed", "self-employed");

        // Hyphenation changes
        assert!(!result.word_edit_vector.is_empty());
    }

    #[test]
    fn test_contraction_expansion() {
        let result = parse_alignment_steps("I cant go", "I can't go");

        // "cant" and "can't" are treated as matches because punctuation is stripped
        // Both become "cant" after strip_punctuation(), so they match
        assert_eq!(result.word_edit_vector, "MMM");
        // Punctuation change is tracked in the punct_edit_vector
    }

    #[test]
    fn test_long_sentence() {
        let original = "The quick brown fox jumps over the laxy dog and runs away quickly";
        let edited = "The quick brown fox jumps over the lazy dog and runs away quickly";

        let result = parse_alignment_steps(original, edited);

        assert_eq!(result.corrections.len(), 1);
        assert_eq!(result.corrections[0].0, "laxy");
        assert_eq!(result.corrections[0].1, "lazy");
    }

    #[test]
    fn test_word_label_conversion() {
        assert_eq!(WordLabel::Match.as_char(), 'M');
        assert_eq!(WordLabel::Substitution.as_char(), 'S');
        assert_eq!(WordLabel::Insert.as_char(), 'I');
        assert_eq!(WordLabel::Delete.as_char(), 'D');
        assert_eq!(WordLabel::Casing.as_char(), 'C');
        assert_eq!(WordLabel::None.as_char(), 'Z');
        assert_eq!(WordLabel::EditCaptureError.as_char(), 'E');

        assert_eq!(WordLabel::from_char('M'), Some(WordLabel::Match));
        assert_eq!(WordLabel::from_char('X'), None);
    }

    #[test]
    fn test_substitution_at_start() {
        // Substitution at the beginning of text
        let result = parse_alignment_steps("teh quick fox", "the quick fox");

        assert_eq!(result.word_edit_vector, "SMM");
        assert_eq!(result.corrections.len(), 1);
    }

    #[test]
    fn test_substitution_at_end() {
        // Substitution at the end of text
        let result = parse_alignment_steps("the quick fxo", "the quick fox");

        assert_eq!(result.word_edit_vector, "MMS");
        assert_eq!(result.corrections.len(), 1);
    }

    #[test]
    fn test_strip_punctuation_helper() {
        assert_eq!(strip_punctuation("hello,"), "hello");
        assert_eq!(strip_punctuation("'world'"), "world");
        assert_eq!(strip_punctuation("test!?"), "test");
        assert_eq!(strip_punctuation("..."), "");
    }

    #[test]
    fn test_extract_punctuation_helper() {
        assert_eq!(extract_punctuation("hello,"), ",");
        assert_eq!(extract_punctuation("'world'"), "''");
        assert_eq!(extract_punctuation("test"), "");
    }

    #[test]
    fn test_isolated_substitution_regex_patterns() {
        // Test the regex pattern matching directly
        let steps = vec![
            AlignmentStep {
                word_label: WordLabel::Match,
                punct_label: WordLabel::None,
                original_word: "the".to_string(),
                edited_word: "the".to_string(),
            },
            AlignmentStep {
                word_label: WordLabel::Substitution,
                punct_label: WordLabel::None,
                original_word: "quikc".to_string(),
                edited_word: "quick".to_string(),
            },
            AlignmentStep {
                word_label: WordLabel::Match,
                punct_label: WordLabel::None,
                original_word: "fox".to_string(),
                edited_word: "fox".to_string(),
            },
        ];

        let vector = edit_vector(&steps);
        assert_eq!(vector, "MSM");

        let indices = find_isolated_substitutions(&vector, &steps);
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn test_multiple_insertions() {
        let result = parse_alignment_steps("hello world", "hello beautiful amazing world");

        // Should detect two insertions
        assert!(result.word_edit_vector.matches('I').count() == 2);
    }

    #[test]
    fn test_multiple_deletions() {
        let result = parse_alignment_steps("hello very big world", "hello world");

        // Should detect two deletions
        assert!(result.word_edit_vector.matches('D').count() == 2);
    }
}
