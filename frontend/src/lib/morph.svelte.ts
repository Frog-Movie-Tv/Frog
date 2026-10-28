/**
 * The poster last opened, as `kind-tmdbId`.
 *
 * A poster morphs into the title page through a shared view-transition
 * name. Naming every poster on the page made the browser capture each one on
 * every navigation, which took seconds on a library of hundreds; only the
 * poster that was opened needs the name, there and on the way back.
 */
export const morph = $state({ key: '' });
