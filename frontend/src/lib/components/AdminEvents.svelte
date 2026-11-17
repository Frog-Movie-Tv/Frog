<script lang="ts">
	import { api, img } from '#lib/api';
	import { eventDay } from '#lib/events';
	import { toasts } from '#lib/toast.svelte';
	import type { EventSummary } from '#lib/types';
	import Empty from './Empty.svelte';
	import Icon from './Icon.svelte';
	import Premiere from './Premiere.svelte';

	/*
	 * Administration → Events: what packs bring, and the three things an
	 * administrator may do with one. Switch it off for everyone, pin the
	 * moment it opens, and watch its premiere without marking anything seen.
	 * Anything new comes as a pack, made with the /event skill.
	 */

	let events = $state<EventSummary[]>([]);
	let loading = $state(true);
	let previewing = $state<EventSummary | null>(null);
	/** The moment being typed for each pack, as the date field holds it. */
	let pins = $state<Record<string, string>>({});

	/** An ISO moment as a local date field reads it. */
	function local(iso: string): string {
		const d = new Date(iso);
		const p = (n: number) => String(n).padStart(2, '0');
		return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
	}

	async function load() {
		try {
			events = (await api.adminEvents()).events;
			pins = Object.fromEntries(events.map((e) => [e.slug, e.pinned ? local(e.zero) : '']));
		} catch (e) {
			toasts.error(e);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void load();
	});

	async function toggle(e: EventSummary) {
		const on = e.on === false;
		try {
			await api.setEventOn(e.slug, on);
			e.on = on;
			toasts.push(
				on ? `${e.title.name} is on for everyone` : `${e.title.name} is off for everyone`
			);
		} catch (err) {
			toasts.error(err);
		}
	}

	async function pin(e: EventSummary, clear = false) {
		const typed = clear ? '' : (pins[e.slug] ?? '');
		try {
			await api.pinEvent(e.slug, typed ? new Date(typed).toISOString() : null);
			toasts.push(
				typed
					? `Pinned to ${new Date(typed).toLocaleString('en-GB')}`
					: 'Back to midnight, where each person is'
			);
			await load();
		} catch (err) {
			toasts.error(err);
		}
	}
</script>

{#if loading}
	<p class="muted">Loading…</p>
{:else if !events.length}
	<Empty art="none" title="No events" hint="Events come with Frog's releases." />
{:else}
	<ul class="events list-pane">
		{#each events as e (e.slug)}
			<li>
				<div class="top">
					<span class="thumb">
						{#if e.title.backdrop_path}<img src={img(e.title.backdrop_path, 'w300')} alt="" />{/if}
					</span>
					<span class="words">
						<a href="/events/{e.slug}"><strong>{e.title.name}</strong></a>
						<span class="faint">
							Opens {eventDay(e.day)}{e.pinned
								? `, pinned to ${new Date(e.zero).toLocaleString('en-GB')}`
								: ', at midnight where each person is'}
						</span>
					</span>
					<button
						class="btn"
						class:on={e.on !== false}
						aria-pressed={e.on !== false}
						onclick={() => toggle(e)}
					>
						{e.on === false ? 'Off' : 'On'}
					</button>
				</div>
				<div class="tools">
					<label>
						<span class="faint">Pin a moment</span>
						<input type="datetime-local" bind:value={pins[e.slug]} />
					</label>
					<button class="btn" onclick={() => pin(e)} disabled={!pins[e.slug]}>Pin</button>
					{#if e.pinned}
						<button class="btn ghost" onclick={() => pin(e, true)}>Unpin</button>
					{/if}
					<button class="btn ghost" onclick={() => (previewing = e)}>
						<Icon name="play" size={14} filled />
						Preview premiere
					</button>
				</div>
			</li>
		{/each}
	</ul>
{/if}

{#if previewing}
	<Premiere event={previewing} preview ondone={() => (previewing = null)} />
{/if}

<style>
	.events li {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 14px;
		min-width: 0;
	}

	.top {
		display: flex;
		align-items: center;
		gap: 12px;
		min-width: 0;
	}

	.thumb {
		flex: none;
		width: 96px;
		aspect-ratio: 16 / 9;
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elev-2);
	}

	.thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.words {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.words a {
		color: inherit;
		text-decoration: none;
	}

	.words .faint {
		font-size: 0.82rem;
	}

	.tools {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		gap: 8px;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 0.8rem;
	}

	input {
		min-height: 38px;
	}
</style>
