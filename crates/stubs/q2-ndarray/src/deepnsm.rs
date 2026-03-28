//! DeepNSM semantic analysis for graph notebooks.
//!
//! Provides NSM (Natural Semantic Metalanguage) analysis for the cockpit
//! notebook system. Words are decomposed into weighted semantic primes
//! for cross-linguistic comparison and semantic reasoning.
//!
//! This module is lightweight and standalone — it does not depend on
//! the `ndarray` feature or any external crate.

/// Number of universal semantic primes in the NSM inventory.
pub const NSM_PRIME_COUNT: usize = 74;

/// The 74 universal semantic primes (NSM theory, after Wierzbicka & Goddard).
///
/// Grouped by semantic category:
///   Substantives, Determiners, Quantifiers, Evaluators, Descriptors,
///   Mental predicates, Speech, Actions/Events/Movement, Existence/Possession,
///   Life/Death, Time, Space, Logical concepts, Intensifier/Augmentor,
///   Similarity, Taxonomy/Partonomy.
pub const NSM_PRIME_NAMES: &[&str] = &[
    // Substantives (0–5)
    "I",
    "YOU",
    "SOMEONE",
    "SOMETHING",
    "THING",
    "BODY",
    // Determiners (6–7)
    "KIND",
    "PART",
    // Quantifiers / demonstratives (8–12)
    "THIS",
    "THE_SAME",
    "OTHER",
    "ELSE",
    "ANOTHER",
    // Evaluators (13–14)
    "GOOD",
    "BAD",
    // Descriptors (15–16)
    "BIG",
    "SMALL",
    // Mental predicates (17–21)
    "THINK",
    "KNOW",
    "WANT",
    "FEEL",
    "SEE",
    // Speech (22–23)
    "SAY",
    "WORDS",
    // Actions, events, movement (24–27)
    "DO",
    "HAPPEN",
    "MOVE",
    "TOUCH",
    // Existence, possession (28–30)
    "THERE_IS",
    "HAVE",
    "BE",
    // Life and death (31–32)
    "LIVE",
    "DIE",
    // Time (33–40)
    "WHEN",
    "NOW",
    "BEFORE",
    "AFTER",
    "A_LONG_TIME",
    "A_SHORT_TIME",
    "FOR_SOME_TIME",
    "MOMENT",
    // Space (41–47)
    "WHERE",
    "HERE",
    "ABOVE",
    "BELOW",
    "FAR",
    "NEAR",
    "SIDE",
    // Logical concepts (48–52)
    "NOT",
    "MAYBE",
    "CAN",
    "BECAUSE",
    "IF",
    // Intensifier / augmentor (53–54)
    "VERY",
    "MORE",
    // Similarity (55–56)
    "LIKE",
    "AS",
    // Taxonomy / partonomy (57–58)
    "ABOVE_KIND",
    "BELOW_KIND",
    // Relational (59–63)
    "ONE",
    "TWO",
    "MUCH",
    "MANY",
    "ALL",
    // Imagination / possibility (64–66)
    "TRUE",
    "INSIDE",
    "SOME",
    // Additional primes (67–73)
    "PEOPLE",
    "SOMEWHERE",
    "AT_THE_SAME_TIME",
    "WITH",
    "IN",
    "WORD",
    "WAY",
];

// Compile-time assertion that we have exactly 74 primes.
const _: () = assert!(NSM_PRIME_NAMES.len() == NSM_PRIME_COUNT);

// ── Vocabulary map ────────────────────────────────────────────────────────

/// A vocabulary entry: (word, list of (prime_index, weight) pairs).
///
/// Weights are in [0.0, 1.0] and need not sum to 1.
type VocabEntry = (&'static str, &'static [(usize, f32)]);

/// Vocabulary of common words decomposed into NSM primes.
///
/// Each word maps to a sparse list of (prime_index, weight) pairs.
/// The index refers to `NSM_PRIME_NAMES`.
const VOCAB: &[VocabEntry] = &[
    // --- Substantives / pronouns ---
    ("i", &[(0, 1.0)]),
    ("me", &[(0, 1.0)]),
    ("you", &[(1, 1.0)]),
    ("someone", &[(2, 1.0)]),
    ("person", &[(2, 0.8), (67, 0.6)]),
    ("people", &[(67, 1.0), (2, 0.5), (63, 0.4)]),
    ("something", &[(3, 1.0)]),
    ("thing", &[(4, 1.0)]),
    ("body", &[(5, 1.0)]),
    // --- Determiners ---
    ("kind", &[(6, 1.0)]),
    ("type", &[(6, 0.9)]),
    ("part", &[(7, 1.0)]),
    ("piece", &[(7, 0.8)]),
    // --- Demonstratives ---
    ("this", &[(8, 1.0)]),
    ("same", &[(9, 1.0)]),
    ("other", &[(10, 1.0)]),
    ("else", &[(11, 1.0)]),
    ("another", &[(12, 1.0)]),
    // --- Evaluators ---
    ("good", &[(13, 1.0)]),
    ("great", &[(13, 0.8), (15, 0.5), (53, 0.6)]),
    ("bad", &[(14, 1.0)]),
    ("terrible", &[(14, 0.8), (53, 0.7)]),
    ("evil", &[(14, 0.9), (20, 0.3)]),
    // --- Descriptors ---
    ("big", &[(15, 1.0)]),
    ("large", &[(15, 0.9)]),
    ("huge", &[(15, 0.9), (53, 0.7)]),
    ("small", &[(16, 1.0)]),
    ("tiny", &[(16, 0.9), (53, 0.6)]),
    // --- Mental predicates ---
    ("think", &[(17, 1.0)]),
    ("believe", &[(17, 0.8), (18, 0.4), (64, 0.3)]),
    ("know", &[(18, 1.0)]),
    ("understand", &[(18, 0.8), (17, 0.5)]),
    ("want", &[(19, 1.0)]),
    ("desire", &[(19, 0.9), (20, 0.3)]),
    ("need", &[(19, 0.8), (50, 0.4)]),
    ("feel", &[(20, 1.0)]),
    ("emotion", &[(20, 0.9)]),
    ("see", &[(21, 1.0)]),
    ("look", &[(21, 0.8), (24, 0.3)]),
    ("watch", &[(21, 0.7), (37, 0.3)]),
    // --- Speech ---
    ("say", &[(22, 1.0)]),
    ("tell", &[(22, 0.8), (1, 0.3)]),
    ("speak", &[(22, 0.7), (23, 0.5)]),
    ("words", &[(23, 1.0)]),
    ("word", &[(72, 1.0)]),
    ("language", &[(23, 0.8), (72, 0.5), (67, 0.3)]),
    // --- Actions, events, movement ---
    ("do", &[(24, 1.0)]),
    ("make", &[(24, 0.8), (28, 0.3)]),
    ("happen", &[(25, 1.0)]),
    ("event", &[(25, 0.8), (33, 0.3)]),
    ("move", &[(26, 1.0)]),
    ("go", &[(26, 0.8)]),
    ("walk", &[(26, 0.7), (5, 0.3)]),
    ("run", &[(26, 0.8), (53, 0.4)]),
    ("touch", &[(27, 1.0)]),
    // --- Existence, possession ---
    ("exist", &[(28, 1.0)]),
    ("have", &[(29, 1.0)]),
    ("own", &[(29, 0.8)]),
    ("be", &[(30, 1.0)]),
    ("is", &[(30, 0.9)]),
    // --- Life and death ---
    ("live", &[(31, 1.0)]),
    ("alive", &[(31, 0.9)]),
    ("die", &[(32, 1.0)]),
    ("dead", &[(32, 0.9)]),
    ("death", &[(32, 0.9)]),
    ("kill", &[(32, 0.7), (24, 0.5), (14, 0.3)]),
    // --- Time ---
    ("when", &[(33, 1.0)]),
    ("now", &[(34, 1.0)]),
    ("before", &[(35, 1.0)]),
    ("after", &[(36, 1.0)]),
    ("long", &[(37, 0.8)]),
    ("short", &[(38, 0.8)]),
    ("time", &[(37, 0.5), (33, 0.5)]),
    // --- Space ---
    ("where", &[(41, 1.0)]),
    ("here", &[(42, 1.0)]),
    ("above", &[(43, 1.0)]),
    ("below", &[(44, 1.0)]),
    ("far", &[(45, 1.0)]),
    ("near", &[(46, 1.0)]),
    ("close", &[(46, 0.8)]),
    ("side", &[(47, 1.0)]),
    // --- Logical ---
    ("not", &[(48, 1.0)]),
    ("maybe", &[(49, 1.0)]),
    ("perhaps", &[(49, 0.9)]),
    ("can", &[(50, 1.0)]),
    ("possible", &[(50, 0.8), (49, 0.4)]),
    ("because", &[(51, 1.0)]),
    ("if", &[(52, 1.0)]),
    // --- Intensifier ---
    ("very", &[(53, 1.0)]),
    ("more", &[(54, 1.0)]),
    // --- Similarity ---
    ("like", &[(55, 1.0)]),
    ("as", &[(56, 1.0)]),
    // --- Quantifiers ---
    ("one", &[(59, 1.0)]),
    ("two", &[(60, 1.0)]),
    ("much", &[(61, 1.0)]),
    ("many", &[(62, 1.0)]),
    ("all", &[(63, 1.0)]),
    // --- Truth / misc ---
    ("true", &[(64, 1.0)]),
    ("inside", &[(65, 1.0)]),
    ("some", &[(66, 1.0)]),
    ("with", &[(69, 1.0)]),
    ("in", &[(70, 1.0)]),
    ("way", &[(73, 1.0)]),
    // --- Higher-level words (decomposed into multiple primes) ---
    ("happy", &[(20, 0.8), (13, 0.7)]),
    ("sad", &[(20, 0.8), (14, 0.6)]),
    ("angry", &[(20, 0.8), (14, 0.5), (19, 0.3)]),
    ("afraid", &[(20, 0.8), (14, 0.5), (25, 0.3)]),
    ("love", &[(20, 0.7), (13, 0.6), (19, 0.5)]),
    ("hate", &[(20, 0.6), (14, 0.7), (19, 0.3)]),
    ("help", &[(24, 0.7), (13, 0.5), (19, 0.3)]),
    ("hurt", &[(20, 0.6), (14, 0.5), (27, 0.4)]),
    ("learn", &[(18, 0.7), (17, 0.5), (34, 0.2)]),
    ("teach", &[(22, 0.5), (18, 0.6), (1, 0.3)]),
    ("give", &[(24, 0.5), (29, 0.4), (1, 0.3)]),
    ("take", &[(24, 0.5), (29, 0.5)]),
    ("eat", &[(24, 0.5), (5, 0.4), (65, 0.3)]),
    ("drink", &[(24, 0.5), (5, 0.3), (65, 0.3)]),
    ("sleep", &[(31, 0.4), (5, 0.5), (48, 0.3)]),
    ("water", &[(4, 0.6), (26, 0.3)]),
    ("fire", &[(4, 0.5), (15, 0.3), (14, 0.3)]),
    ("earth", &[(4, 0.6), (44, 0.3)]),
    ("sky", &[(4, 0.5), (43, 0.4)]),
    ("home", &[(41, 0.5), (31, 0.4), (13, 0.3)]),
    ("child", &[(2, 0.6), (16, 0.5), (31, 0.3)]),
    ("mother", &[(2, 0.5), (67, 0.3), (31, 0.4), (13, 0.3)]),
    ("father", &[(2, 0.5), (67, 0.3), (31, 0.4)]),
];

// ── Core functions ────────────────────────────────────────────────────────

/// Decompose text into a weighted NSM prime vector.
///
/// The input text is lowercased and split on whitespace. Each token is
/// looked up in the built-in vocabulary; unknown tokens are ignored.
/// The returned array holds the accumulated (and L1-normalised) weight
/// for each of the 74 semantic primes.
///
/// # Example
/// ```
/// let v = q2_ndarray::deepnsm::nsm_decompose("I want to know");
/// assert!(v[0] > 0.0);  // I   → prime 0
/// assert!(v[19] > 0.0); // WANT → prime 19
/// assert!(v[18] > 0.0); // KNOW → prime 18
/// ```
pub fn nsm_decompose(text: &str) -> [f32; NSM_PRIME_COUNT] {
    let mut vec = [0.0f32; NSM_PRIME_COUNT];

    for token in text.split_whitespace() {
        let lower = token.to_lowercase();
        // Strip common punctuation from edges.
        let word = lower.trim_matches(|c: char| !c.is_alphanumeric());
        if word.is_empty() {
            continue;
        }
        if let Some((_w, primes)) = VOCAB.iter().find(|(w, _)| *w == word) {
            for &(idx, weight) in *primes {
                vec[idx] += weight;
            }
        }
    }

    // L1-normalise so vectors are comparable regardless of text length.
    let sum: f32 = vec.iter().sum();
    if sum > 0.0 {
        for v in &mut vec {
            *v /= sum;
        }
    }

    vec
}

/// Cosine similarity between two NSM decomposition vectors.
///
/// Returns a value in [-1.0, 1.0].  For non-negative NSM vectors
/// produced by [`nsm_decompose`] the range is [0.0, 1.0].
///
/// Returns `0.0` if either vector has zero magnitude.
pub fn nsm_cosine_similarity(a: &[f32; NSM_PRIME_COUNT], b: &[f32; NSM_PRIME_COUNT]) -> f32 {
    let mut dot = 0.0f32;
    let mut mag_a = 0.0f32;
    let mut mag_b = 0.0f32;

    for i in 0..NSM_PRIME_COUNT {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }

    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

// ── Legality analysis ─────────────────────────────────────────────────────

/// Result of analysing an NSM explication for legality.
///
/// In NSM theory an explication should ideally use ONLY semantic primes
/// (and approved semantic molecules). This struct reports how closely a
/// given explication conforms.
#[derive(Debug, Clone, PartialEq)]
pub struct NsmLegality {
    /// Fraction of tokens that are semantic primes (0.0–1.0).
    pub primes_ratio: f32,
    /// Fraction of tokens that are approved semantic molecules (0.0–1.0).
    pub molecules_ratio: f32,
    /// `true` if the explication uses the target word it is defining
    /// (a circularity violation).
    pub uses_original_word: bool,
    /// Total number of content tokens analysed.
    pub total_tokens: usize,
    /// Number of tokens recognised as semantic primes.
    pub prime_tokens: usize,
    /// Number of tokens recognised as semantic molecules.
    pub molecule_tokens: usize,
}

/// Approved semantic molecules (frequently used complex concepts that are
/// accepted in NSM explications even though they are not primes).
const MOLECULES: &[&str] = &[
    "hands", "eyes", "mouth", "head", "face", "ears", "nose", "legs", "arms", "heart", "mind",
    "children", "men", "women", "animal", "dog", "cat", "bird", "fish", "tree", "ground", "sun",
    "moon", "day", "night", "morning", "evening", "long_time", "short_time", "hot", "cold",
    "hard", "soft", "round", "flat", "sharp", "heavy", "light", "wet", "dry", "colour", "white",
    "black", "red", "green", "blue", "yellow",
];

/// Analyse an NSM explication for legality with respect to a target word.
///
/// Checks what fraction of the explication consists of semantic primes vs
/// molecules, and whether the explication circularly references the
/// target word.
///
/// # Example
/// ```
/// let r = q2_ndarray::deepnsm::analyze_legality(
///     "someone feels something good because of this",
///     "happy",
/// );
/// assert!(!r.uses_original_word);
/// assert!(r.primes_ratio > 0.5);
/// ```
pub fn analyze_legality(explication: &str, target_word: &str) -> NsmLegality {
    let target_lower = target_word.to_lowercase();

    let mut total_tokens = 0usize;
    let mut prime_tokens = 0usize;
    let mut molecule_tokens = 0usize;
    let mut uses_original_word = false;

    // Build a quick set of prime names (lowercased, underscores → spaces removed).
    // We compare normalised tokens against these.
    let prime_set: Vec<String> = NSM_PRIME_NAMES
        .iter()
        .map(|p| p.to_lowercase().replace('_', ""))
        .collect();

    for token in explication.split_whitespace() {
        let lower = token.to_lowercase();
        let word = lower.trim_matches(|c: char| !c.is_alphanumeric());
        if word.is_empty() {
            continue;
        }

        total_tokens += 1;

        if word == target_lower {
            uses_original_word = true;
        }

        let normalised = word.replace('_', "");
        if prime_set.contains(&normalised) {
            prime_tokens += 1;
        } else if MOLECULES.contains(&word) {
            molecule_tokens += 1;
        }
    }

    let total_f = total_tokens.max(1) as f32;

    NsmLegality {
        primes_ratio: prime_tokens as f32 / total_f,
        molecules_ratio: molecule_tokens as f32 / total_f,
        uses_original_word,
        total_tokens,
        prime_tokens,
        molecule_tokens,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prime_count() {
        assert_eq!(NSM_PRIME_NAMES.len(), NSM_PRIME_COUNT);
    }

    #[test]
    fn test_decompose_basic() {
        let v = nsm_decompose("I want to know something");
        // "I" → prime 0, "want" → prime 19, "know" → prime 18, "something" → prime 3
        assert!(v[0] > 0.0, "prime I should be activated");
        assert!(v[19] > 0.0, "prime WANT should be activated");
        assert!(v[18] > 0.0, "prime KNOW should be activated");
        assert!(v[3] > 0.0, "prime SOMETHING should be activated");
        // "to" is not in vocab → should not contribute
        // All values should be non-negative and sum to ~1.0 (L1-normalised).
        let sum: f32 = v.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "L1 norm should be ~1.0, got {sum}");
    }

    #[test]
    fn test_decompose_empty() {
        let v = nsm_decompose("");
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_decompose_unknown_tokens() {
        let v = nsm_decompose("xylophone quasar");
        // All unknown → zero vector
        assert!(v.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_cosine_similarity_self() {
        let v = nsm_decompose("I want to know");
        let sim = nsm_cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "self-similarity should be 1.0, got {sim}"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        // Two vectors with no overlap should have similarity 0.
        let mut a = [0.0f32; NSM_PRIME_COUNT];
        let mut b = [0.0f32; NSM_PRIME_COUNT];
        a[0] = 1.0; // I
        b[32] = 1.0; // DIE
        let sim = nsm_cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5, "orthogonal vectors should have sim ~0");
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let zero = [0.0f32; NSM_PRIME_COUNT];
        let v = nsm_decompose("I know");
        assert_eq!(nsm_cosine_similarity(&zero, &v), 0.0);
        assert_eq!(nsm_cosine_similarity(&v, &zero), 0.0);
        assert_eq!(nsm_cosine_similarity(&zero, &zero), 0.0);
    }

    #[test]
    fn test_cosine_similarity_related_words() {
        // "happy" and "sad" should be somewhat similar (both are feelings)
        // but not identical.
        let happy = nsm_decompose("happy");
        let sad = nsm_decompose("sad");
        let sim = nsm_cosine_similarity(&happy, &sad);
        assert!(sim > 0.3, "happy/sad should share FEEL prime, got {sim}");
        assert!(sim < 1.0, "happy/sad should not be identical, got {sim}");
    }

    #[test]
    fn test_legality_analysis() {
        // An explication using mostly primes.
        let result = analyze_legality(
            "someone feels something good because of this someone",
            "happy",
        );
        assert!(!result.uses_original_word);
        assert!(
            result.primes_ratio > 0.5,
            "primes_ratio should be > 0.5, got {}",
            result.primes_ratio
        );
        assert!(result.total_tokens > 0);
        assert!(result.prime_tokens > 0);
    }

    #[test]
    fn test_legality_circular_reference() {
        let result = analyze_legality("happy is when someone feels good", "happy");
        assert!(
            result.uses_original_word,
            "should detect circular reference to target word"
        );
    }

    #[test]
    fn test_legality_with_molecules() {
        let result = analyze_legality("someone feels something good in the head", "joy");
        assert!(!result.uses_original_word);
        assert!(result.molecule_tokens > 0, "should detect 'head' as molecule");
    }

    #[test]
    fn test_vocab_size() {
        // We promised at least 50 vocabulary entries.
        assert!(
            VOCAB.len() >= 50,
            "vocabulary should have >= 50 entries, has {}",
            VOCAB.len()
        );
    }

    #[test]
    fn test_vocab_indices_in_range() {
        // Every prime index in the vocabulary must be < NSM_PRIME_COUNT.
        for (word, primes) in VOCAB {
            for &(idx, weight) in *primes {
                assert!(
                    idx < NSM_PRIME_COUNT,
                    "word '{word}' has prime index {idx} >= {NSM_PRIME_COUNT}"
                );
                assert!(
                    (0.0..=1.0).contains(&weight),
                    "word '{word}' has weight {weight} outside [0,1]"
                );
            }
        }
    }
}
