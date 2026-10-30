<script lang="ts">
	import Logo from '#lib/components/Logo.svelte';
	import Spinner from '#lib/components/Spinner.svelte';
	import { prettyDate } from '#lib/format';
	import { session } from '#lib/session.svelte';
	import { settings } from '#lib/settings.svelte';

	interface Group {
		title: string;
		items: string[];
	}

	interface Release {
		version: string;
		codename: string;
		date: string;
		intro: string;
		groups: Group[];
	}

	let releases = $state<Release[]>([]);
	let loading = $state(true);

	$effect(() => {
		void fetch('/changelog.json')
			.then((r) => (r.ok ? r.json() : []))
			.then((r: Release[]) => (releases = r))
			.catch(() => (releases = []))
			.finally(() => (loading = false));
	});

	// Opening this page is what clears the dot in the navigation.
	$effect(() => {
		const version = session.status?.version;
		if (version && settings.loaded && settings.current.lastSeenVersion !== version) {
			settings.set('lastSeenVersion', version);
		}
	});

	const current = $derived(session.status?.version ?? '');

	/**
	 * Render one changelog line.
	 *
	 * Everything is escaped first, then a small allowlist of inline tags is
	 * put back. Escaping only `<` left `&lt;kbd>` behind, because the closing
	 * angle bracket was never escaped and so never matched on the way out.
	 */
	function render(item: string): string {
		return item
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/&lt;(\/?)(kbd|em|strong|code)&gt;/g, '<$1$2>')
			.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
			.replace(/`([^`]+)`/g, '<code>$1</code>')
			.replace(
				/\[([^\]]+)\]\(([^)]+)\)/g,
				'<a href="$2" rel="noreferrer noopener" target="_blank">$1</a>'
			);
	}

	function tone(title: string) {
		const t = title.toLowerCase();
		if (t.startsWith('fix')) return 'fixed';
		if (t.startsWith('highlight') || t.startsWith('add')) return 'added';
		return 'changed';
	}
</script>

<svelte:head>
	<title>What's new · Frog</title>
</svelte:head>

<div class="page">
	<header class="head">
		<Logo size={44} />
		<div>
			<h1>What's new</h1>
			<p class="muted">Newest first.</p>
		</div>
	</header>

	{#if loading}
		<Spinner label="Reading the notes" />
	{:else if !releases.length}
		<p class="muted">No release notes are bundled with this build.</p>
	{:else}
		<ol class="releases">
			{#each releases as release (release.version)}
				<li class="release">
					<div class="version">
						<h2>{release.version}</h2>
						{#if release.codename}<span class="codename">{release.codename}</span>{/if}
						{#if release.version === current}<span class="pill now">Installed</span>{/if}
						{#if release.date}
							<time class="faint">{prettyDate(release.date) || release.date}</time>
						{/if}
					</div>

					<div class="body">
						{#if release.intro}<p class="intro">{release.intro}</p>{/if}

						{#each release.groups as group (group.title)}
							<section class="group" data-tone={tone(group.title)}>
								<h3>{group.title}</h3>
								<ul>
									{#each group.items as item, i (i)}
										<!-- eslint-disable-next-line svelte/no-at-html-tags -->
										<li>{@html render(item)}</li>
									{/each}
								</ul>
							</section>
						{/each}
					</div>
				</li>
			{/each}
		</ol>
	{/if}
</div>

<style>
	.head {
		display: flex;
		align-items: center;
		gap: 18px;
		margin-bottom: 34px;
	}

	.head p {
		font-size: 0.9rem;
		margin-top: 4px;
	}

	.releases {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 36px;
		max-width: 80ch;
	}

	.release {
		display: grid;
		grid-template-columns: 150px minmax(0, 1fr);
		gap: 24px;
	}

	.version {
		display: flex;
		flex-direction: column;
		gap: 7px;
		align-items: flex-start;
		position: sticky;
		top: calc(var(--header-h) + 18px);
		align-self: start;
	}

	.version h2 {
		font-size: 1.5rem;
		font-variant-numeric: tabular-nums;
	}

	.codename {
		font-family: var(--font-display);
		font-size: 0.92rem;
		color: var(--accent);
		font-weight: 600;
	}

	.pill.now {
		background: var(--accent-soft);
		border-color: color-mix(in srgb, var(--accent) 40%, transparent);
		color: var(--accent);
	}

	time {
		font-size: 0.78rem;
	}

	.body {
		display: flex;
		flex-direction: column;
		gap: 22px;
	}

	.intro {
		font-size: 1rem;
		color: var(--text-dim);
	}

	.group h3 {
		font-size: 0.74rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--text-faint);
		margin-bottom: 10px;
	}

	.group[data-tone='added'] h3 {
		color: var(--good);
	}
	.group[data-tone='fixed'] h3 {
		color: var(--warn);
	}

	.group ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 9px;
	}

	.group li {
		position: relative;
		padding-left: 18px;
		font-size: 0.92rem;
		line-height: 1.55;
		color: var(--text-dim);
	}

	.group li::before {
		content: '';
		position: absolute;
		left: 0;
		top: 0.62em;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--line-strong);
	}

	.group[data-tone='added'] li::before {
		background: var(--good);
	}
	.group[data-tone='fixed'] li::before {
		background: var(--warn);
	}

	.group li :global(strong) {
		color: var(--text);
		font-weight: 650;
	}

	.group li :global(code) {
		font-family: var(--font-mono);
		font-size: 0.88em;
		background: var(--bg-elev-2);
		padding: 1px 5px;
		border-radius: 4px;
	}

	.group li :global(kbd) {
		display: inline-block;
		padding: 1px 6px;
		border-radius: 5px;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev-2);
		font-family: var(--font-mono);
		font-size: 0.82em;
	}

	.group li :global(a) {
		color: var(--accent);
		text-decoration: underline;
	}

	@media (max-width: 720px) {
		.release {
			grid-template-columns: 1fr;
			gap: 12px;
		}

		.version {
			position: static;
			flex-direction: row;
			align-items: baseline;
			gap: 12px;
		}
	}
</style>
