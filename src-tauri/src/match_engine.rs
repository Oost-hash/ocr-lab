use std::collections::HashSet;

use crate::types::MatchedItem;

// ─── Public API ──────────────────────────────────────────────────────────────

/// Score all catalog items against OCR text, return deduplicated matches above threshold.
/// Groups results by name and keeps only the best score per unique item.
pub fn match_items(
    ocr_text: &str,
    ocr_lines: &[(String, f32, f32)],
    catalog: &[(String, String)], // (unique_name, display_name)
    threshold: f32,
) -> Vec<MatchedItem> {
    let words = build_word_set(ocr_text);
    if words.is_empty() {
        return Vec::new();
    }

    // Score every catalog item, group by name, keep best score + position
    let mut best_by_name: std::collections::HashMap<String, (f32, f32, f32)> =
        std::collections::HashMap::new();

    for (_, name) in catalog {
        if name.is_empty() { continue; }
        let score = score_item(name, &words);
        if score < threshold { continue; }
        let entry = best_by_name.entry(name.clone()).or_insert((0.0, 0.5, 0.5));
        if score > entry.0 {
            // Find best position from OCR lines
            let (x, y) = find_best_position(name, ocr_lines);
            *entry = (score, x, y);
        }
    }

    let mut results: Vec<MatchedItem> = best_by_name
        .into_iter()
        .map(|(name, (score, x, y))| MatchedItem { name, score, x_center: x, y_center: y })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results
}

// ─── Copied from FrameForge ocr.rs ──────────────────────────────────────────

/// Levenshtein distance with early exit.
fn lev_dist(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let (m, n) = (a.len(), b.len());
    if m.abs_diff(n) > 3 { return 99; }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i-1] == b[j-1] { prev[j-1] }
                      else { 1 + prev[j].min(curr[j-1]).min(prev[j-1]) };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Check whether `catalog_word` appears in `ocr_words` via:
///   1. Exact match
///   2. Prefix match: OCR truncated ("prime"→"pri", "voruna"→"vor")
///   3. Suffix substring: "neuroptics" → "rüroptics" (both contain "optics")
///   4. Levenshtein ≤ 1 (or ≤ 2 for ≥8-char words)
///   5. Sliding-window inside longer merged tokens ("Sevagotfirime")
fn word_found_in_set(
    catalog_word: &str,
    ocr_words: &HashSet<String>,
) -> bool {
    if ocr_words.contains(catalog_word) { return true; }
    if catalog_word.len() < 4 { return false; }

    // Prefix: OCR word is the leading portion of the catalog word
    for ocr_w in ocr_words {
        if ocr_w.len() >= 3 && catalog_word.starts_with(ocr_w.as_str()) { return true; }
    }

    // Suffix substring
    if catalog_word.len() >= 6 {
        let suffix_len = (catalog_word.len() / 2).max(5);
        let suffix = &catalog_word[catalog_word.len() - suffix_len..];
        if ocr_words.iter().any(|w| w.find(suffix).is_some_and(|p| p != 1)) { return true; }
    }

    // Levenshtein with edit budget
    let max_dist = if catalog_word.len() >= 8 { 2 }
                   else if catalog_word.len() >= 5 { 1 }
                   else { 0 };
    let wb = catalog_word.as_bytes();
    for ocr_w in ocr_words {
        if ocr_w.len() >= 4 {
            let dist = lev_dist(catalog_word, ocr_w);
            let len_diff = (catalog_word.len() as isize - ocr_w.len() as isize).unsigned_abs();
            if dist <= max_dist && !(len_diff == dist && len_diff >= 2) { return true; }
        }
        // Sliding window for merged tokens
        let ob = ocr_w.as_bytes();
        if ob.len() >= wb.len() + 4 {
            for (win_start, win) in ob.windows(wb.len()).enumerate() {
                let errs = wb.iter().zip(win.iter()).filter(|(a, b)| a != b).count();
                if errs == 0 && win_start + wb.len() == ob.len() && win_start >= 3 { continue; }
                if errs <= max_dist { return true; }
            }
        }
    }
    false
}

/// Normalise OCR text for catalog matching.
fn normalise(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() { return c.to_ascii_lowercase(); }
            match c {
                'À'|'Á'|'Â'|'Ã'|'Ä'|'Å'|'à'|'á'|'â'|'ã'|'ä'|'å' => 'a',
                'È'|'É'|'Ê'|'Ë'|'è'|'é'|'ê'|'ë' => 'e',
                'Ì'|'Í'|'Î'|'Ï'|'ì'|'í'|'î'|'ï' => 'i',
                'Ò'|'Ó'|'Ô'|'Õ'|'Ö'|'ò'|'ó'|'ô'|'õ'|'ö' => 'o',
                'Ù'|'Ú'|'Û'|'Ü'|'ù'|'ú'|'û'|'ü' => 'u',
                'Ñ'|'ñ' => 'n',
                'Ç'|'ç' => 'c',
                'Ý'|'ý'|'ÿ' => 'y',
                _ => ' ',
            }
        })
        .collect()
}

/// Build a word set from OCR text, applying common OCR corrections.
fn build_word_set(text: &str) -> HashSet<String> {
    let corrected = text
        .replace('@', "bl")
        .replace(')', "d")
        .replace('&', " p");
    normalise(&corrected).chars()
        .map(|c| if c.is_ascii_alphabetic() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|s| s.to_string())
        .collect()
}

/// Score a catalog item against OCR words.
fn score_item(display_name: &str, words: &HashSet<String>) -> f32 {
    let norm = normalise(display_name);
    let mut seen = HashSet::new();
    let item_words: Vec<&str> = norm.split_whitespace()
        .filter(|&w| seen.insert(w))
        .collect();
    if item_words.is_empty() { return 0.0; }
    let n_catalog = item_words.len() as f32;
    let n_ocr = words.len() as f32;
    let matched = item_words.iter()
        .filter(|&&w| word_found_in_set(w, words))
        .count();

    let base = (matched as f32 / n_catalog)
        .max(if n_ocr > 0.0 { matched as f32 / n_ocr } else { 0.0 });

    let len_bonus: f32 = item_words.iter()
        .filter(|&&w| !word_found_in_set(w, words))
        .map(|&cw| {
            words.iter()
                .map(|ow| {
                    let diff = (cw.len() as isize - ow.len() as isize).unsigned_abs();
                    if diff == 0 { 0.08_f32 } else if diff == 1 { 0.04 } else { 0.0 }
                })
                .fold(0.0_f32, f32::max)
        })
        .sum::<f32>() / n_catalog;

    base + len_bonus
}

/// Find the best x/y position for a matched item from OCR line positions.
fn find_best_position(name: &str, ocr_lines: &[(String, f32, f32)]) -> (f32, f32) {
    let norm = normalise(name);
    let name_words: Vec<&str> = norm.split_whitespace().collect();
    let mut best_score = 0.0f32;
    let mut best_x = 0.5f32;
    let mut best_y = 0.5f32;
    for (line_text, x, y) in ocr_lines {
        let line_norm = normalise(line_text);
        let line_words: HashSet<&str> = line_norm.split_whitespace().collect();
        let matches = name_words.iter().filter(|w| line_words.contains(**w)).count();
        let score = matches as f32 / name_words.len().max(1) as f32;
        if score > best_score {
            best_score = score;
            best_x = *x;
            best_y = *y;
        }
    }
    (best_x, best_y)
}
