use unicode_segmentation::UnicodeSegmentation;

use crate::Algorithm;

#[derive(Debug, Clone)]
pub struct PreparedQuery {
    algorithm: Algorithm,
    normalized: String,
    graphemes: Vec<String>,
}

impl PreparedQuery {
    pub fn new(query: &str, algorithm: Algorithm) -> Self {
        let normalized = query.to_lowercase();
        let graphemes = normalized.graphemes(true).map(str::to_owned).collect();
        Self {
            algorithm,
            normalized,
            graphemes,
        }
    }

    pub fn similarity(&self, candidate: &str, sensitivity: f64) -> f64 {
        let candidate = candidate.to_lowercase();
        match self.algorithm {
            Algorithm::JaroWinkler => jaro_winkler_normalized(&self.normalized, &candidate),
            Algorithm::Levenshtein => {
                let candidate: Vec<&str> = candidate.graphemes(true).collect();
                normalized_levenshtein(&self.graphemes, &candidate, sensitivity, None)
                    .expect("an unlimited edit-distance calculation cannot be pruned")
            }
        }
    }

    pub fn score_candidate(
        &self,
        candidate: &str,
        sensitivity: f64,
        threshold: f64,
    ) -> Option<f64> {
        let candidate = candidate.to_lowercase();
        match self.algorithm {
            Algorithm::JaroWinkler => {
                let score = jaro_winkler_normalized(&self.normalized, &candidate);
                (score >= threshold).then_some(score)
            }
            Algorithm::Levenshtein => {
                let candidate: Vec<&str> = candidate.graphemes(true).collect();
                let max_length = self.graphemes.len().max(candidate.len());
                if max_length == 0 {
                    return Some(1.0);
                }
                if sensitivity >= 0.0
                    && self.graphemes.len().abs_diff(candidate.len()) as f64
                        > (1.0 - threshold) * max_length as f64
                {
                    return None;
                }

                normalized_levenshtein(
                    &self.graphemes,
                    &candidate,
                    sensitivity,
                    (sensitivity >= 0.0).then_some((1.0 - threshold) * max_length as f64),
                )
                .filter(|score| *score >= threshold)
            }
        }
    }
}

pub fn similarity(first: &str, second: &str, sensitivity: f64, algorithm: Algorithm) -> f64 {
    PreparedQuery::new(first, algorithm).similarity(second, sensitivity)
}

pub fn jaro_winkler(first: &str, second: &str) -> f64 {
    jaro_winkler_normalized(&first.to_lowercase(), &second.to_lowercase())
}

fn jaro_winkler_normalized(first: &str, second: &str) -> f64 {
    let first: Vec<&str> = first.graphemes(true).collect();
    let second: Vec<&str> = second.graphemes(true).collect();
    let jaro = jaro_distance(&first, &second);

    if jaro > 0.7 {
        let prefix_length = first
            .iter()
            .zip(&second)
            .take(4)
            .take_while(|(left, right)| left == right)
            .count();
        jaro + prefix_length as f64 * 0.1 * (1.0 - jaro)
    } else {
        jaro
    }
}

fn jaro_distance(first: &[&str], second: &[&str]) -> f64 {
    if first == second {
        return 1.0;
    }
    if first.is_empty() || second.is_empty() {
        return 0.0;
    }

    let match_distance = first.len().max(second.len()) / 2;
    let match_distance = match_distance.saturating_sub(1);
    let mut first_matches = vec![false; first.len()];
    let mut second_matches = vec![false; second.len()];

    for (first_index, first_grapheme) in first.iter().enumerate() {
        let start = first_index.saturating_sub(match_distance);
        let end = (first_index + match_distance + 1).min(second.len());
        for second_index in start..end {
            if !second_matches[second_index] && *first_grapheme == second[second_index] {
                first_matches[first_index] = true;
                second_matches[second_index] = true;
                break;
            }
        }
    }

    let matches = first_matches.iter().filter(|matched| **matched).count();
    if matches == 0 {
        return 0.0;
    }

    let matched_first = first
        .iter()
        .zip(first_matches)
        .filter_map(|(value, matched)| matched.then_some(*value));
    let matched_second = second
        .iter()
        .zip(second_matches)
        .filter_map(|(value, matched)| matched.then_some(*value));
    let transpositions = matched_first
        .zip(matched_second)
        .filter(|(left, right)| left != right)
        .count() as f64
        / 2.0;
    let matches = matches as f64;

    (matches / first.len() as f64
        + matches / second.len() as f64
        + (matches - transpositions) / matches)
        / 3.0
}

fn normalized_levenshtein<T: AsRef<str>>(
    query: &[T],
    candidate: &[&str],
    sensitivity: f64,
    max_distance: Option<f64>,
) -> Option<f64> {
    let max_length = query.len().max(candidate.len());
    if max_length == 0 {
        return Some(1.0);
    }

    let distance = levenshtein_distance(query, candidate, sensitivity, max_distance)?;
    Some(1.0 - distance / max_length as f64)
}

fn levenshtein_distance<T: AsRef<str>>(
    first: &[T],
    second: &[&str],
    sensitivity: f64,
    max_distance: Option<f64>,
) -> Option<f64> {
    let (row, column, swapped) = if first.len() <= second.len() {
        (first.len(), second.len(), false)
    } else {
        (second.len(), first.len(), true)
    };
    let mut previous: Vec<f64> = (0..=row).map(|value| value as f64).collect();
    let mut current = vec![0.0; row + 1];

    for column_index in 0..column {
        current[0] = (column_index + 1) as f64;
        let mut row_minimum = current[0];
        for row_index in 0..row {
            let (row_value, column_value) = if swapped {
                (second[row_index], first[column_index].as_ref())
            } else {
                (first[row_index].as_ref(), second[column_index])
            };
            let substitution = if row_value == column_value {
                0.0
            } else {
                sensitivity
            };
            current[row_index + 1] = (previous[row_index + 1] + 1.0)
                .min(current[row_index] + 1.0)
                .min(previous[row_index] + substitution);
            row_minimum = row_minimum.min(current[row_index + 1]);
        }
        if max_distance.is_some_and(|limit| row_minimum > limit) {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    Some(previous[row])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_representative_jaro_winkler_scores() {
        assert_eq!(jaro_winkler("hello", "hallo"), 0.88);
        assert_eq!(jaro_winkler("martha", "marhta"), 0.9611111111111111);
        assert_eq!(jaro_winkler("", ""), 1.0);
        assert_eq!(jaro_winkler("a", "b"), 0.0);
        assert_eq!(jaro_winkler("SpellCheck", "spellcheck"), 1.0);
    }

    #[test]
    fn applies_winkler_boost_only_to_strong_matches() {
        let first = vec!["a", "b", "x", "x", "x", "x"];
        let second = vec!["a", "b", "y", "y", "y", "y"];
        let raw = jaro_distance(&first, &second);
        assert!(raw < 0.7);
        assert_eq!(jaro_winkler("abxxxx", "abyyyy"), raw);
    }

    #[test]
    fn treats_extended_grapheme_clusters_as_one_character() {
        let first: Vec<&str> = "👩‍🔬abc".graphemes(true).collect();
        let second: Vec<&str> = "👩‍🔬abd".graphemes(true).collect();
        let raw = jaro_distance(&first, &second);
        let expected = raw + 3.0 * 0.1 * (1.0 - raw);
        assert!((jaro_winkler("👩‍🔬abc", "👩‍🔬abd") - expected).abs() < 1e-12);
    }

    #[test]
    fn pruned_levenshtein_agrees_with_exact_score() {
        let strings = [
            "",
            "a",
            "ab",
            "kitten",
            "sitting",
            "spellcheck",
            "shellcheck",
            "café",
        ];
        for query in strings {
            for candidate in strings {
                for sensitivity in [0.5, 1.0, 2.0] {
                    for threshold in [0.0, 0.5, 0.75, 1.0] {
                        let prepared = PreparedQuery::new(query, Algorithm::Levenshtein);
                        let exact = prepared.similarity(candidate, sensitivity);
                        let pruned = prepared.score_candidate(candidate, sensitivity, threshold);
                        match pruned {
                            Some(score) => {
                                assert!(exact >= threshold);
                                assert!((score - exact).abs() < 1e-12);
                            }
                            None => assert!(exact < threshold),
                        }
                    }
                }
            }
        }
    }
}
