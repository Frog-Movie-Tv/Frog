//! A franchise across films and shows.
//!
//! TMDB groups films into collections ("Demon Slayer: Kimetsu no Yaiba
//! Collection") and never puts a show in one, though the show is often where
//! the franchise began. Frog joins them by name: a show belongs to a
//! collection when its name is the collection's without TMDB's word
//! "Collection", or starts with it as a title does before a subtitle. A show
//! page finds its collection the same way. Names only ever add shows to a
//! collection TMDB made; nothing is invented from a name alone.

use crate::state::AppState;
use crate::text;
use crate::tmdb::models::SearchItem;

/// A collection's name as the franchise is called, folded for comparing.
pub fn franchise_of(collection: &str) -> String {
    let folded = text::fold(collection);
    let trimmed = folded.trim();
    trimmed
        .strip_suffix(" collection")
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

/// Whether a show of this name is part of the franchise: the same name, or
/// that name and then a subtitle ("Attack on Titan: The Final Season").
pub fn belongs(show: &str, franchise: &str) -> bool {
    let show = text::fold(show);
    let show = show.trim();
    if franchise.chars().count() < 3 {
        return false;
    }
    match show.strip_prefix(franchise) {
        Some("") => true,
        Some(rest) => rest.starts_with(':') || rest.starts_with(" -") || rest.starts_with(" –"),
        None => false,
    }
}

/// The shows TMDB has under a franchise's name.
pub async fn shows_of(state: &AppState, collection_name: &str) -> Vec<SearchItem> {
    let franchise = franchise_of(collection_name);
    let Ok(raw) = state.tmdb.search_named("tv", &franchise).await else {
        return Vec::new();
    };
    raw["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| {
            v["name"].as_str().is_some_and(|n| belongs(n, &franchise))
                || v["original_name"]
                    .as_str()
                    .is_some_and(|n| belongs(n, &franchise))
        })
        .filter_map(|v| SearchItem::from_value(v, Some("tv")))
        .collect()
}

/// The film collection a show is the beginning of, when TMDB has one by its
/// name.
pub async fn collection_of_show(state: &AppState, show_name: &str) -> Option<i64> {
    let raw = state
        .tmdb
        .search_named("collection", show_name)
        .await
        .ok()?;
    raw["results"]
        .as_array()?
        .iter()
        .find(|c| {
            c["name"]
                .as_str()
                .is_some_and(|name| belongs(show_name, &franchise_of(name)))
        })
        .and_then(|c| c["id"].as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_collection_is_not_part_of_the_name() {
        assert_eq!(
            franchise_of("Demon Slayer: Kimetsu no Yaiba Collection"),
            "demon slayer: kimetsu no yaiba"
        );
        assert_eq!(franchise_of("Pokémon Collection"), "pokemon");
    }

    #[test]
    fn a_show_belongs_by_its_name_or_its_name_and_a_subtitle() {
        let f = franchise_of("Demon Slayer: Kimetsu no Yaiba Collection");
        assert!(belongs("Demon Slayer: Kimetsu no Yaiba", &f));
        let titan = franchise_of("Attack on Titan Collection");
        assert!(belongs("Attack on Titan: Junior High", &titan));
        assert!(belongs("Attack on Titan", &titan));
        // A longer word is another name, not a subtitle.
        assert!(!belongs("Attack on Titanic", &titan));
        assert!(!belongs("Demon Slayer", &f));
        // Two letters match too much to mean anything.
        assert!(!belongs("It", &franchise_of("It Collection")));
    }
}
