<script lang="ts">
	import { fade, fly } from 'svelte/transition';
	import { api, frogId, img } from '#lib/api';
	import { parseList } from '#lib/import/list';
	import { backCloses } from '#lib/layers.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { SearchItem } from '#lib/types';
	import Icon from './Icon.svelte';

	interface Props {
		shelfId: number;
		shelfName: string;
		/** Frog's ids of what is on the shelf already, to leave out. */
		onShelf: Set<number>;
		ondone: (added: number) => void;
		onclose: () => void;
	}

	let { shelfId, shelfName, onShelf, ondone, onclose }: Props = $props();

	const EXAMPLE = [
		'Heat (1995)',
		'Spirited Away (2001)',
		'https://letterboxd.com/film/dune-2021/'
	].join('\n');

	/** The server takes this many rows a request. */
	const BATCH = 200;

	interface Found {
		line: string;
		item: SearchItem | null;
		/** Not certain: the closest title, left unticked. */
		unsure: boolean;
		on: boolean;
	}

	let text = $state('');
	let step = $state<'paste' | 'finding' | 'review' | 'adding'>('paste');
	let found = $state<Found[]>([]);
	let done = $state(0);
	let total = $state(0);

	backCloses(
		() => step !== 'adding',
		() => onclose()
	);

	const rows = $derived(parseList(text));
	const chosen = $derived(found.filter((f) => f.on && f.item));
	const missing = $derived(found.filter((f) => !f.item));

	async function find() {
		if (!rows.length) return;
		step = 'finding';
		done = 0;
		total = rows.length;
		const out: Found[] = [];
		try {
			for (let i = 0; i < rows.length; i += BATCH) {
				const batch = rows.slice(i, i + BATCH);
				const { results } = await api.matchImport(batch);
				for (const row of batch) {
					const r = results.find((x) => x.key === row.key);
					const item = r?.matched ?? r?.candidates[0] ?? null;
					const unsure = !r?.matched && !!item;
					out.push({ line: row.label ?? row.title, item, unsure, on: !!item && !unsure });
				}
				done = Math.min(rows.length, i + BATCH);
			}
			found = out;
			step = 'review';
		} catch (e) {
			toasts.error(e);
			step = 'paste';
		}
	}

	async function add() {
		step = 'adding';
		done = 0;
		total = chosen.length;
		let added = 0;
		const seen = [...onShelf];
		try {
			// One at a time, in the list's order: each goes to the end of the shelf.
			for (const f of chosen) {
				const id = await frogId(f.item!);
				if (!seen.includes(id)) {
					await api.addToShelf(shelfId, id);
					seen.push(id);
					added++;
				}
				done++;
			}
			ondone(added);
		} catch (e) {
			toasts.error(e);
			ondone(added);
		}
	}

	const keyOf = (f: Found, i: number) => `${i}:${f.line}`;
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
	class="scrim"
	transition:fade={{ duration: 120 }}
	onclick={() => step !== 'adding' && onclose()}
></div>

<div
	class="dialog"
	role="dialog"
	aria-modal="true"
	aria-labelledby="from-list-title"
	transition:fly={{ y: 16, duration: 180 }}
>
	<h2 id="from-list-title">Add a list to {shelfName}</h2>

	{#if step === 'paste'}
		<p class="faint lead">
			One title a line, with its year if you like, or links from TMDB, IMDb, Letterboxd or AniList.
			A Letterboxd list export works too. They go on in this order.
		</p>
		<!-- svelte-ignore a11y_autofocus -->
		<textarea class="input" rows="9" autofocus placeholder={EXAMPLE} bind:value={text}></textarea>
		<div class="actions">
			<span class="faint count">{rows.length ? `${rows.length} found in it` : ''}</span>
			<button type="button" class="btn ghost" onclick={onclose}>Cancel</button>
			<button type="button" class="btn primary" disabled={!rows.length} onclick={find}>
				Find titles
			</button>
		</div>
	{:else if step === 'finding' || step === 'adding'}
		<div class="working">
			<div class="meter"><span style:width="{total ? (done / total) * 100 : 0}%"></span></div>
			<p class="faint">
				{step === 'finding' ? 'Finding' : 'Adding'}
				{done} of {total}
			</p>
		</div>
	{:else}
		<ul class="found">
			{#each found as f, i (keyOf(f, i))}
				<li class:missing={!f.item}>
					{#if f.item}
						<label>
							<input type="checkbox" bind:checked={f.on} />
							{#if f.item.poster_path}
								<img src={img(f.item.poster_path, 'w92')} alt="" loading="lazy" />
							{:else}
								<span class="thumb"></span>
							{/if}
							<span class="what">
								<strong>{f.item.name}</strong>
								<span class="faint"
									>{f.item.year ?? ''}{#if f.unsure}{f.item.year ? ' · ' : ''}Closest match for “{f.line}”{/if}</span
								>
							</span>
						</label>
					{:else}
						<span class="thumb none"><Icon name="x" size={13} /></span>
						<span class="what">
							<strong>{f.line}</strong>
							<span class="faint">Nothing found</span>
						</span>
					{/if}
				</li>
			{/each}
		</ul>
		<div class="actions">
			<span class="faint count"
				>{missing.length ? `${missing.length} not found` : 'Everything found'}</span
			>
			<button type="button" class="btn ghost" onclick={() => (step = 'paste')}>Back</button>
			<button type="button" class="btn primary" disabled={!chosen.length} onclick={add}>
				Add {chosen.length}
			</button>
		</div>
	{/if}
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 95;
		background: rgba(0, 0, 0, 0.55);
	}

	.dialog {
		position: fixed;
		z-index: 96;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		width: min(560px, calc(100vw - 28px));
		max-height: calc(100dvh - 40px);
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 22px;
		border-radius: var(--radius-lg);
		background: var(--bg-float);
		border: 1px solid var(--line-strong);
		box-shadow:
			inset 0 1px 0 var(--shine),
			var(--shadow-3);
	}

	h2 {
		font-size: 1.2rem;
	}

	.lead {
		font-size: 0.86rem;
		margin: -4px 0 0;
	}

	textarea {
		width: 100%;
		resize: vertical;
		min-height: 160px;
		font-family: var(--font-mono);
		font-size: 0.82rem;
		line-height: 1.5;
	}

	.actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 10px;
	}

	.count {
		margin-right: auto;
		font-size: 0.82rem;
	}

	.working {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 18px 0;
	}

	.meter {
		height: 8px;
		border-radius: var(--radius-pill);
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.meter span {
		display: block;
		height: 100%;
		background: var(--accent);
		transition: width var(--fast) var(--ease);
	}

	.found {
		list-style: none;
		margin: 0;
		padding: 0;
		overflow-y: auto;
		max-height: 52dvh;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.found li,
	.found label {
		display: flex;
		align-items: center;
		gap: 11px;
		min-width: 0;
	}

	.found li {
		padding: 5px 8px;
		border-radius: var(--radius-sm);
	}

	.found label {
		flex: 1;
		cursor: pointer;
	}

	.found li:hover {
		background: var(--bg-hover);
	}

	.found img,
	.thumb {
		flex: none;
		width: 28px;
		aspect-ratio: 2 / 3;
		border-radius: 4px;
		object-fit: cover;
		background: var(--bg-elev-2);
	}

	.thumb.none {
		display: grid;
		place-items: center;
		color: var(--text-faint);
	}

	.what {
		display: flex;
		flex-direction: column;
		min-width: 0;
		font-size: 0.88rem;
	}

	.what strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.what .faint {
		font-size: 0.76rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.missing .what strong {
		color: var(--text-dim);
		font-weight: 500;
	}
</style>
