<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '#lib/components/Icon.svelte';
	import { session } from '#lib/session.svelte';
	import { MASCOTS, THEMES, settings, type MascotName, type ThemeName } from '#lib/settings.svelte';

	let step = $state<1 | 2>(1);
	const s = $derived(settings.current);
	const name = $derived(session.status?.username ?? '');

	/** Welcomed once; leaving through any door counts. */
	async function finish(to: string) {
		settings.set('onboarded', true);
		await goto(to, { replace: true });
	}
</script>

<svelte:head>
	<title>Welcome · Frog</title>
</svelte:head>

<div class="page welcome">
	<div class="steps" aria-label="Step {step} of 2">
		<i class:on={step >= 1}></i>
		<i class:on={step >= 2}></i>
	</div>

	{#if step === 1}
		<header>
			<img src="/mascots/{s.mascot}.svg" alt="" width="72" height="72" />
			<h1>Welcome{name ? `, ${name}` : ''}</h1>
			<p class="muted">Make Frog yours. You can change all of this later in Settings.</p>
		</header>

		<section>
			<h2>Theme</h2>
			<div class="tiles">
				{#each THEMES as t (t.id)}
					<button
						class="tile"
						class:on={s.theme === t.id}
						aria-pressed={s.theme === t.id}
						onclick={() => settings.set('theme', t.id as ThemeName)}
					>
						<span class="swatches" aria-hidden="true">
							{#each t.swatches as c (c)}<i style:background={c}></i>{/each}
						</span>
						<strong>{t.label}</strong>
						<span class="faint">{t.hint}</span>
					</button>
				{/each}
			</div>
		</section>

		<section>
			<h2>Mascot</h2>
			<div class="tiles mascots">
				{#each MASCOTS as m (m.id)}
					<button
						class="tile"
						class:on={s.mascot === m.id}
						aria-pressed={s.mascot === m.id}
						onclick={() => settings.set('mascot', m.id as MascotName)}
					>
						<img src="/mascots/{m.id}.svg" alt="" width="44" height="44" />
						<strong>{m.label}</strong>
						<span class="faint">{m.hint}</span>
					</button>
				{/each}
			</div>
		</section>

		<div class="actions">
			<button class="btn ghost" onclick={() => finish('/')}>Skip</button>
			<button class="btn primary" onclick={() => (step = 2)}>
				Continue <Icon name="chevron-right" size={16} />
			</button>
		</div>
	{:else}
		<header>
			<img src="/mascots/{s.mascot}.svg" alt="" width="72" height="72" />
			<h1>Fill your library</h1>
			<p class="muted">Bring what you have watched, or start with something you love.</p>
		</header>

		<div class="choices">
			<button class="choice card" onclick={() => finish('/import')}>
				<Icon name="upload" size={22} />
				<span>
					<strong>Import from another tracker</strong>
					<span class="faint"
						>Letterboxd, Trakt, IMDb, TV Time, AniList, MyAnimeList or a Frog backup</span
					>
				</span>
				<Icon name="chevron-right" size={18} />
			</button>
			<button class="choice card" onclick={() => finish('/search')}>
				<Icon name="search" size={22} />
				<span>
					<strong>Search for a film or show</strong>
					<span class="faint">Add it, rate it, tick off episodes</span>
				</span>
				<Icon name="chevron-right" size={18} />
			</button>
		</div>

		<div class="actions">
			<button class="btn ghost" onclick={() => (step = 1)}>
				<Icon name="chevron-left" size={16} /> Back
			</button>
			<button class="btn ghost" onclick={() => finish('/')}>Later</button>
		</div>
	{/if}
</div>

<style>
	.welcome {
		max-width: 760px;
		display: flex;
		flex-direction: column;
		gap: 26px;
	}

	.steps {
		display: flex;
		gap: 6px;
		justify-content: center;
	}

	.steps i {
		width: 28px;
		height: 4px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
	}

	.steps i.on {
		background: var(--accent);
	}

	header {
		text-align: center;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
	}

	header p {
		margin: 0;
	}

	h2 {
		font-size: 1rem;
		margin-bottom: 10px;
	}

	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 130px), 1fr));
		gap: 10px;
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		padding: 12px;
		border-radius: var(--radius);
		border: 1px solid var(--line);
		background: var(--bg-elev);
		color: var(--text);
		text-align: left;
		cursor: pointer;
	}

	.tile.on {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.tile .faint {
		font-size: 0.76rem;
	}

	.swatches {
		display: flex;
		gap: 4px;
		margin-bottom: 4px;
	}

	.swatches i {
		width: 18px;
		height: 18px;
		border-radius: 50%;
		border: 1px solid var(--line-strong);
	}

	.choices {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.choice {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 16px 18px;
		color: var(--text);
		text-align: left;
		cursor: pointer;
	}

	.choice:hover {
		border-color: var(--accent);
	}

	.choice > span {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.choice .faint {
		font-size: 0.82rem;
	}

	.actions {
		display: flex;
		justify-content: space-between;
		gap: 10px;
	}
</style>
