//! One genre vocabulary for every title, films and shows alike.
//!
//! TMDB names a show's genres differently from a film's: "Sci-Fi & Fantasy"
//! where a film has Science Fiction and Fantasy, "Action & Adventure" for
//! Action and Adventure. Counted as they come, Stats and profiles listed
//! Science Fiction beside Sci-Fi & Fantasy. Frog stores a title's genres
//! already in the film words (`catalog::upsert_title` through
//! `TitleCore::from_detail`, and `db::unify_genres` for rows from before), so
//! Stats, profiles, the library's genre filter, smart shelves and badges all
//! read the one list without knowing about this table.

/// TMDB's show genres and the film genres each one stands for. A genre not
/// listed is the same word for both and stays as it is.
pub const AS_FILM: &[(&str, &[&str])] = &[
    ("Action & Adventure", &["Action", "Adventure"]),
    ("Sci-Fi & Fantasy", &["Science Fiction", "Fantasy"]),
    ("War & Politics", &["War", "Politics"]),
    ("Kids", &["Family"]),
];

/// A title's genres in the one vocabulary, in TMDB's order, each once.
pub fn unify<S: AsRef<str>>(genres: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(genres.len() + 1);
    for g in genres {
        let g = g.as_ref();
        let words: &[&str] = AS_FILM
            .iter()
            .find(|(show, _)| *show == g)
            .map(|(_, film)| *film)
            .unwrap_or(std::slice::from_ref(&g));
        for w in words {
            if !out.iter().any(|have| have == w) {
                out.push((*w).to_string());
            }
        }
    }
    out
}

/// The same, on the JSON list `title.genres_json` holds. Anything unreadable
/// comes back as it was.
pub fn unify_json(raw: &str) -> String {
    match serde_json::from_str::<Vec<String>>(raw) {
        Ok(list) => serde_json::to_string(&unify(&list)).unwrap_or_else(|_| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}

/// Whether a stored list still holds a show-only word, so the startup pass
/// touches only rows that need it.
pub fn needs_unifying(raw: &str) -> bool {
    AS_FILM
        .iter()
        .any(|(show, _)| raw.contains(&format!("\"{show}\"")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_show_s_genres_read_as_a_film_s() {
        assert_eq!(
            unify(&["Sci-Fi & Fantasy", "Drama", "Action & Adventure"]),
            vec!["Science Fiction", "Fantasy", "Drama", "Action", "Adventure"]
        );
    }

    #[test]
    fn each_genre_is_listed_once() {
        assert_eq!(
            unify(&["Action", "Action & Adventure", "Kids", "Family"]),
            vec!["Action", "Adventure", "Family"]
        );
    }

    #[test]
    fn the_stored_list_is_rewritten_only_when_it_needs_to_be() {
        assert!(needs_unifying(r#"["Drama","Sci-Fi & Fantasy"]"#));
        assert!(!needs_unifying(r#"["Drama","Science Fiction"]"#));
        assert_eq!(unify_json(r#"["War & Politics"]"#), r#"["War","Politics"]"#);
        assert_eq!(unify_json("not json"), "not json");
    }
}
