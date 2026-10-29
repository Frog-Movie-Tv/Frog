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

#[cfg(test)]
mod tests {
    use super::*;

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
