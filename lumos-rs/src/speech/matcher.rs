//! Follow the script by voice: fuzzy-match what was just heard against the words that come next in the script.
//! Pure functions (no audio, no engine) so they are easy to test in Spanish, Galician and English.

use crate::model::{Script, ScriptKind};

/// Words ahead of the reader that can be matched.
pub const WINDOW_WORDS: usize = 48;
/// Last N heard words compared with the window.
pub const TAIL: usize = 4;

/// Lower-case, accent-free, letters/digits only, single spaces. `ñ`→`n`, `ç`→`c` so recogniser spelling variants still match.
pub fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        let m = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        if m == ' ' {
            if !space {
                out.push(' ');
                space = true;
            }
        } else {
            out.push(m);
            space = false;
        }
    }
    out.trim_end().to_string()
}

pub fn words_of(s: &str) -> Vec<String> {
    norm(s).split(' ').filter(|w| !w.is_empty()).map(|w| w.to_string()).collect()
}

fn lev1(a: &str, b: &str) -> bool {
    // true when the edit distance is ≤ 1 (substitution, insertion or deletion)
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let (mut i, mut j, mut diff) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else {
            diff += 1;
            if diff > 1 {
                return false;
            }
            if a.len() > b.len() {
                i += 1;
            } else if b.len() > a.len() {
                j += 1;
            } else {
                i += 1;
                j += 1;
            }
        }
    }
    diff + (a.len() - i) + (b.len() - j) <= 1
}

/// Two words "are the same" when equal, share a 4-letter stem (long words) or differ by one letter (words ≥ 5).
pub fn same_word(heard: &str, script: &str) -> bool {
    if heard == script {
        return true;
    }
    let (h, s) = (heard.chars().count(), script.chars().count());
    if h > 3 && s > 3 && heard.chars().take(4).eq(script.chars().take(4)) {
        return true;
    }
    h >= 5 && s >= 5 && lev1(heard, script)
}

/// Index in `win` just after the best match of the last ≤ [`TAIL`] heard words, if at least 2/3 of them match.
pub fn match_heard(heard: &[String], win: &[String]) -> Option<usize> {
    let n = heard.len().min(TAIL);
    if n < 2 {
        return None;
    }
    let tail = &heard[heard.len() - n..];
    let (mut best, mut best_score) = (None, 0.0f32);
    for i in 0..win.len().saturating_sub(n - 1) {
        let hits = (0..n).filter(|k| same_word(&tail[*k], &win[i + k])).count();
        let score = hits as f32 / n as f32;
        // prefer the earliest among equals so a repeated phrase does not skip ahead
        if score > best_score + 1e-6 {
            best_score = score;
            best = Some(i + n);
        }
    }
    (best_score >= 0.66).then_some(best).flatten()
}

/// The next spoken words from block index `from` on, with the block index each word belongs to.
pub fn window(script: &Script, from: usize) -> (Vec<String>, Vec<usize>) {
    let (mut win, mut owner) = (Vec::new(), Vec::new());
    for (bi, b) in script.blocks.iter().enumerate().skip(from) {
        if win.len() >= WINDOW_WORDS {
            break;
        }
        if matches!(b.kind, ScriptKind::Direction | ScriptKind::Note) {
            continue;
        }
        for w in words_of(&b.text) {
            win.push(w);
            owner.push(bi);
        }
    }
    (win, owner)
}

/// Block index the speaker has reached, looking only forward from `from` (the reader's block index; `0` when before the start).
pub fn target_block(script: &Script, from: usize, heard: &[String]) -> Option<usize> {
    let (win, owner) = window(script, from);
    let end = match_heard(heard, &win)?;
    owner.get(end.min(owner.len()).checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ScriptBlock, ScriptKind};

    fn blk(id: u32, kind: ScriptKind, text: &str) -> ScriptBlock {
        ScriptBlock { id, kind, speaker: String::new(), text: text.into(), page: 0, marks: vec![] }
    }
    fn script(blocks: Vec<ScriptBlock>) -> Script {
        Script { id: 1, name: "t".into(), blocks, pdf: None }
    }
    fn h(s: &str) -> Vec<String> {
        words_of(s)
    }

    #[test]
    fn normalisation_folds_accents_and_punctuation() {
        assert_eq!(norm("¡Qué NOCHE, señor Núñez!"), "que noche senor nunez");
        assert_eq!(norm("  Ata-xe  o   coiro… "), "ata xe o coiro");
        assert_eq!(norm("Façanas da vila"), "facanas da vila");
    }

    #[test]
    fn word_similarity() {
        assert!(same_word("noche", "noches"));
        assert!(same_word("camiño", "camino"));
        assert!(same_word("cantaban", "cantando"), "shared 4-letter stem");
        assert!(same_word("lumes", "lumez"), "one letter off");
        assert!(!same_word("sol", "sal"), "short words must be exact");
        assert!(!same_word("noche", "dia"));
    }

    #[test]
    fn matches_a_phrase_with_recognition_errors() {
        let win = h("se apagan las luces del teatro y cae el telon");
        let end = match_heard(&h("las luces del teatro"), &win).unwrap();
        assert_eq!(&win[end - 1], "teatro");
        // one wrong word out of four still matches
        let end = match_heard(&h("las luzes del teatro"), &win).unwrap();
        assert_eq!(&win[end - 1], "teatro");
        // two wrong of four does not
        assert!(match_heard(&h("las naves de teatro"), &win).is_none());
        // too short
        assert!(match_heard(&h("luces"), &win).is_none());
    }

    #[test]
    fn repeated_phrase_prefers_the_earliest_occurrence() {
        let win = h("vamos a casa vamos a casa vamos a casa");
        assert_eq!(match_heard(&h("vamos a casa"), &win), Some(3));
    }

    #[test]
    fn target_block_skips_directions_and_looks_forward_only() {
        let s = script(vec![
            blk(1, ScriptKind::Heading, "Acto primero"),
            blk(2, ScriptKind::Speech, "Buenas noches a todos los presentes"),
            blk(3, ScriptKind::Direction, "Entra Ana por la derecha"),
            blk(4, ScriptKind::Speech, "Perdón por llegar tarde a la reunión"),
            blk(5, ScriptKind::Speech, "No pasa nada siéntate con nosotros"),
        ]);
        assert_eq!(target_block(&s, 0, &h("a todos los presentes")), Some(1));
        // the stage direction is never matched
        assert_eq!(target_block(&s, 0, &h("entra ana por la derecha")), None);
        assert_eq!(target_block(&s, 1, &h("por llegar tarde a la")), Some(3));
        assert_eq!(target_block(&s, 3, &h("siéntate con nosotros")), Some(4));
        // already past: looking from block 4 cannot jump back to block 1
        assert_eq!(target_block(&s, 4, &h("buenas noches a todos")), None);
    }

    #[test]
    fn galician_text() {
        let s = script(vec![blk(1, ScriptKind::Speech, "Onte pola noite chovía sobre o camiño da aldea"), blk(2, ScriptKind::Speech, "Ninguén oíu os cans ladrar na distancia")]);
        assert_eq!(target_block(&s, 0, &h("pola noite chovia sobre o camino")), Some(0));
        assert_eq!(target_block(&s, 0, &h("ninguen oiu os cans ladrar")), Some(1));
    }
}
