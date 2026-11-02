//! Spotting the user's name in what other people on a call say.

/// How a line addresses the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mention {
    /// Their name came up.
    Named,
    /// A question or request with their name in it ("Ana, can you…?").
    Asked,
}

/// The names to listen for: `user_name` is "Full Name" or "Full Name, Nickname, …".
/// Each entry counts in full, and so does the first name of the first entry. Names
/// shorter than three letters are skipped; they'd match too much.
pub fn name_variants(user_name: &str) -> Vec<Vec<String>> {
    let mut variants: Vec<Vec<String>> = Vec::new();
    let mut add = |words: Vec<String>| {
        if !words.is_empty() && words.concat().chars().count() >= 3 && !variants.contains(&words) {
            variants.push(words);
        }
    };
    for (i, entry) in user_name.split(',').enumerate() {
        let words = words(entry);
        if i == 0 && words.len() > 1 {
            add(vec![words[0].clone()]);
        }
        add(words);
    }
    variants
}

/// Whether `text` says one of the `names` (from [`name_variants`]), and how.
pub fn mention(text: &str, names: &[Vec<String>]) -> Option<Mention> {
    let said = words(text);
    let found = names
        .iter()
        .any(|name| said.windows(name.len()).any(|w| w == name.as_slice()));
    if !found {
        return None;
    }
    Some(if is_request(text) {
        Mention::Asked
    } else {
        Mention::Named
    })
}

const REQUESTS: [&str; 12] = [
    "can you",
    "could you",
    "would you",
    "will you",
    "please",
    "do you",
    "what do you think",
    "puedes",
    "podrias",
    "por favor",
    "quieres",
    "tu que opinas",
];

fn is_request(text: &str) -> bool {
    if text.contains('?') || text.contains('¿') {
        return true;
    }
    let folded = format!(" {} ", words(text).join(" "));
    REQUESTS.iter().any(|r| folded.contains(&format!(" {r} ")))
}

/// Lowercase words without accents: "¿Peñaloza, puedes?" → ["penaloza", "puedes"].
fn words(text: &str) -> Vec<String> {
    let folded: String = text.chars().flat_map(char::to_lowercase).map(fold).collect();
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        c => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heard(text: &str, user: &str) -> Option<Mention> {
        mention(text, &name_variants(user))
    }

    #[test]
    fn hears_the_full_or_first_name_and_nicknames() {
        assert_eq!(
            heard("Thanks Ana, that helps.", "Ana Torres"),
            Some(Mention::Named)
        );
        assert_eq!(
            heard("I talked to ana torres yesterday", "Ana Torres"),
            Some(Mention::Named)
        );
        assert_eq!(heard("Ramon has the numbers", "Ramón Díaz"), Some(Mention::Named));
        assert_eq!(
            heard("Dani, that's yours", "Daniel Ruiz, Dani"),
            Some(Mention::Named)
        );
        assert_eq!(
            heard("Anabel has the numbers", "Ana Torres"),
            None,
            "whole words only"
        );
        assert_eq!(heard("Torres is a surname here", "Ana Torres"), None);
        assert_eq!(heard("Anything from Ana?", ""), None);
    }

    #[test]
    fn tells_questions_and_requests_apart() {
        assert_eq!(
            heard("Ana, can you send the plan?", "Ana Torres"),
            Some(Mention::Asked)
        );
        assert_eq!(
            heard("Ana please take this one", "Ana Torres"),
            Some(Mention::Asked)
        );
        assert_eq!(
            heard("¿Ana, puedes revisarlo", "Ana Torres"),
            Some(Mention::Asked)
        );
        assert_eq!(heard("Ana podrías revisarlo", "Ana Torres"), Some(Mention::Asked));
        assert_eq!(
            heard("As Ana said, we ship Friday.", "Ana Torres"),
            Some(Mention::Named)
        );
    }

    #[test]
    fn skips_names_too_short_to_tell() {
        assert!(name_variants("Al").is_empty());
        assert_eq!(
            name_variants("Bo Lee, Bo"),
            vec![vec!["bo".to_string(), "lee".to_string()]]
        );
    }
}
