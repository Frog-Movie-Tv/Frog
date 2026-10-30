use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// A name with case and accents taken out, for matching what someone types:
/// "pokemon" finds "Pokémon", "shippuden" finds "Shippūden". SQLite only folds
/// ASCII case, so titles keep this beside their name and the library filter
/// compares the folded term against it.
///
/// Compatibility decomposition also turns full-width letters and circled
/// digits into plain ones. A few letters are distinct rather than accented,
/// so decomposition leaves them alone; those are spelled out the way people
/// type them on a keyboard without them.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.nfkd() {
        if is_combining_mark(c) {
            continue;
        }
        match c {
            'ø' | 'Ø' => out.push('o'),
            'æ' | 'Æ' => out.push_str("ae"),
            'œ' | 'Œ' => out.push_str("oe"),
            'ß' | 'ẞ' => out.push_str("ss"),
            'đ' | 'Đ' | 'ð' | 'Ð' => out.push('d'),
            'ł' | 'Ł' => out.push('l'),
            'þ' | 'Þ' => out.push_str("th"),
            'ı' => out.push('i'),
            _ => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// What a title is found by: its name and its original name, folded.
pub fn search_name(name: &str, original_name: Option<&str>) -> String {
    match original_name {
        Some(original) if original != name => fold(&format!("{name}\n{original}")),
        _ => fold(name),
    }
}

/// A name as search compares it: folded, "&" read as "and", punctuation gone.
/// "Tom & Jerry" and "tom and jerry" are the same words.
pub fn words(s: &str) -> Vec<String> {
    fold(&s.replace('&', " and "))
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// How closely a name answers what was typed, best first. Used for every kind
/// of result, so a title and a person are measured the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Closeness {
    /// The same words.
    Exact,
    /// The name starts with what was typed.
    Starts,
    /// Every word typed is in the name, the last perhaps still being typed.
    AllWords,
    /// It shares something with the query, or a provider has its reasons.
    Loose,
}

/// `typed` is `words` of the query.
pub fn closeness(name: &str, typed: &[String]) -> Closeness {
    let have = words(name);
    if typed.is_empty() {
        return Closeness::Loose;
    }
    if have == typed {
        return Closeness::Exact;
    }
    let (last, whole) = typed.split_last().expect("not empty");
    let leads = have.len() >= typed.len()
        && have.iter().zip(whole).all(|(a, b)| a == b)
        && have[typed.len() - 1].starts_with(last.as_str());
    if leads {
        return Closeness::Starts;
    }
    // "and" joins names rather than being one of them.
    let wanted = |w: &&String| w.as_str() != "and";
    let found = whole.iter().filter(wanted).all(|w| have.contains(w))
        && (last == "and" || have.iter().any(|h| h.starts_with(last.as_str())));
    if found {
        Closeness::AllWords
    } else {
        Closeness::Loose
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(name: &str, typed: &str) -> Closeness {
        closeness(name, &words(typed))
    }

    #[test]
    fn an_ampersand_is_the_word_and() {
        assert_eq!(close("Tom and Jerry", "Tom & Jerry"), Closeness::Exact);
        assert_eq!(close("Tom & Jerry", "tom and jerry"), Closeness::Exact);
        assert_eq!(
            close("Tom & Jerry Kids Show", "tom & jerry"),
            Closeness::Starts
        );
        assert_eq!(
            close("The Tom and Jerry Show", "Tom & Jerry"),
            Closeness::AllWords
        );
        assert_eq!(close("Tom Pittman", "Tom & Jerry"), Closeness::Loose);
    }

    #[test]
    fn the_last_word_may_still_be_being_typed() {
        assert_eq!(close("Interstellar", "inters"), Closeness::Starts);
        assert_eq!(close("Breaking Bad", "bad bre"), Closeness::AllWords);
        assert_eq!(close("Pokémon", "pokemon"), Closeness::Exact);
        assert_eq!(close("Anything", ""), Closeness::Loose);
    }

    #[test]
    fn accents_and_case_fall_away() {
        assert_eq!(fold("Pokémon"), "pokemon");
        assert_eq!(fold("Naruto Shippūden"), "naruto shippuden");
        assert_eq!(fold("SHIPPUDEN"), "shippuden");
        assert_eq!(fold("Amélie"), "amelie");
        assert_eq!(fold("Søren & Æsir, Straße"), "soren & aesir, strasse");
        assert_eq!(fold("Ｆｕｌｌ ①"), "full 1");
        assert_eq!(fold("Breaking Bad"), "breaking bad");
    }

    #[test]
    fn the_original_name_is_searchable_too() {
        assert_eq!(
            search_name("Spirited Away", Some("千と千尋の神隠し")),
            format!("spirited away\n{}", fold("千と千尋の神隠し"))
        );
        assert_eq!(search_name("Dark", Some("Dark")), "dark");
        assert_eq!(search_name("Dark", None), "dark");
    }
}
