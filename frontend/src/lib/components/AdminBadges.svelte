<script lang="ts">
	import { untrack } from 'svelte';
	import { api } from '#lib/api';
	import { conditionText } from '#lib/badge-board';
	import { backCloses } from '#lib/layers.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { AdminBadge, AdminUser, WaitingBadge } from '#lib/types';
	import BadgeEditor from './BadgeEditor.svelte';
	import Empty from './Empty.svelte';
	import Icon from './Icon.svelte';
	import Select from './Select.svelte';
	import Spinner from './Spinner.svelte';

	interface Props {
		users: AdminUser[];
	}

	let { users }: Props = $props();

	let badges = $state<AdminBadge[]>([]);
	/** Frog's own badges whose name a badge made here already has. */
	let waiting = $state<WaitingBadge[]>([]);
	let loaded = $state(false);
	/** The editor: open on a badge, on a new one (null), or shut (undefined). */
	let editing = $state<AdminBadge | null | undefined>(undefined);
	/** Giving one badge to people, or one person their badges. */
	let giving = $state<AdminBadge | null>(null);
	let person = $state<AdminUser | null>(null);

	let tab = $state<'made' | 'own'>('made');
	let query = $state('');
	let sort = $state<'name' | 'most' | 'fewest' | 'newest'>('name');

	backCloses(
		() => !!giving || !!person,
		() => {
			giving = null;
			person = null;
		}
	);

	async function load() {
		try {
			({ badges, waiting } = await api.badges());
		} catch (e) {
			toasts.error(e);
		} finally {
			loaded = true;
		}
	}
	load();

	const made = $derived(badges.filter((b) => !b.rule));
	const own = $derived(badges.filter((b) => b.rule));
	const givenOut = $derived(badges.reduce((n, b) => n + b.holders.length, 0));

	const fold = (text: string) =>
		text
			.normalize('NFD')
			.replace(/\p{Diacritic}/gu, '')
			.toLowerCase();

	const shown = $derived.by(() => {
		const words = fold(query).split(/\s+/).filter(Boolean);
		const list = (tab === 'made' ? made : own).filter((b) => {
			if (!words.length) return true;
			const text = fold(`${b.name} ${b.description ?? ''} ${conditionText(b.condition)}`);
			return words.every((w) => text.includes(w));
		});
		const by = {
			name: (a: AdminBadge, b: AdminBadge) => a.name.localeCompare(b.name),
			most: (a: AdminBadge, b: AdminBadge) => b.holders.length - a.holders.length,
			fewest: (a: AdminBadge, b: AdminBadge) => a.holders.length - b.holders.length,
			newest: (a: AdminBadge, b: AdminBadge) => b.created_at.localeCompare(a.created_at)
		}[sort];
		return [...list].sort((a, b) => by(a, b) || a.name.localeCompare(b.name));
	});

	const count = (n: number) =>
		n === 0 ? 'Nobody yet' : n === 1 ? '1 person' : `${n.toLocaleString()} people`;

	/** A person's badges, the ones they held when it opened first: ticking one
	    leaves the rows where they are. */
	const theirs = $derived.by(() => {
		const id = person?.id;
		if (id === undefined || !badges.length) return [];
		return untrack(() => {
			const holds = (b: AdminBadge) => (b.holders.includes(id) ? 0 : 1);
			return [...badges].sort((a, b) => holds(a) - holds(b) || a.name.localeCompare(b.name));
		});
	});
	const nameOf = (id: number) => users.find((u) => u.id === id)?.username ?? 'someone';

	/** How a badge is earned, in a few words. */
	function how(badge: AdminBadge): string {
		if (badge.rule) return 'Frog gives it on its own';
		if (badge.condition) return `On its own at ${conditionText(badge.condition)}`;
		return 'Given by hand';
	}

	/** Opened from a person's row in People, or `?person=` in the address. */
	export function forPerson(user: AdminUser) {
		person = user;
	}

	async function toggle(badge: AdminBadge, userId: number) {
		const holds = badge.holders.includes(userId);
		try {
			if (holds) {
				await api.takeBadge(badge.id, userId);
				badge.holders = badge.holders.filter((h) => h !== userId);
				toasts.push(`${badge.name} taken back from ${nameOf(userId)}`);
			} else {
				await api.giveBadge(badge.id, userId);
				badge.holders = [...badge.holders, userId];
				toasts.push(`${badge.name} given to ${nameOf(userId)}`);
			}
		} catch (e) {
			toasts.error(e);
		}
	}

	async function remove(badge: AdminBadge) {
		try {
			const { removed } = await api.deleteBadge(badge.id);
			badges = badges.filter((b) => b.id !== badge.id);
			toasts.undoable(`${badge.name} deleted`, async () => {
				try {
					await api.restoreBadge(removed);
					await load();
				} catch (e) {
					toasts.error(e);
				}
			});
		} catch (e) {
			toasts.error(e);
		}
	}
</script>

<div class="summary">
	<div class="stat card">
		<strong>{made.length.toLocaleString()}</strong>
		<span class="faint">made here</span>
	</div>
	<div class="stat card">
		<strong>{own.length.toLocaleString()}</strong>
		<span class="faint">of Frog’s own</span>
	</div>
	<div class="stat card">
		<strong>{givenOut.toLocaleString()}</strong>
		<span class="faint">held in all</span>
	</div>
	<button class="btn primary new" onclick={() => (editing = null)}>
		<Icon name="plus" size={16} /> New badge
	</button>
</div>

<div class="tools">
	<div class="segmented" role="group" aria-label="Which badges">
		<button class:on={tab === 'made'} onclick={() => (tab = 'made')}>
			Made here <span class="faint">{made.length}</span>
		</button>
		<button class:on={tab === 'own'} onclick={() => (tab = 'own')}>
			Frog’s own <span class="faint">{own.length}</span>
		</button>
	</div>
	<div class="searchbar">
		<Icon name="search" size={17} />
		<input
			bind:value={query}
			type="search"
			placeholder="Search badges"
			aria-label="Search badges"
			autocomplete="off"
			spellcheck="false"
		/>
	</div>
	<Select
		label="Sort"
		value={sort}
		options={[
			{ value: 'name', label: 'A to Z' },
			{ value: 'most', label: 'Most held' },
			{ value: 'fewest', label: 'Fewest held' },
			{ value: 'newest', label: 'Newest' }
		]}
		onchange={(v) => (sort = v as typeof sort)}
	/>
</div>

{#if tab === 'own'}
	<p class="faint lead">
		Frog gives these on its own, for milestones and moments. Rename or redraw any of them; their
		rules stay as they are.
	</p>
{/if}

{#if waiting.length}
	<p class="waiting card">
		<Icon name="info" size={16} />
		<span>
			{waiting.length === 1 ? 'One of Frog’s badges is' : `${waiting.length} of Frog’s badges are`}
			waiting for a name a badge made here already has: {waiting.map((w) => w.name).join(', ')}.
			Rename yours to let {waiting.length === 1 ? 'it' : 'them'} in.
		</span>
	</p>
{/if}

{#if !loaded}
	<Spinner label="Loading badges" />
{:else if tab === 'made' && !made.length}
	<Empty
		art="rest"
		title="No badges made here yet"
		hint="Draw one from any picture, then give it out by hand or let Frog give it once a count is reached."
	>
		<button class="btn primary" onclick={() => (editing = null)}>
			<Icon name="plus" size={16} /> New badge
		</button>
	</Empty>
{:else if !shown.length}
	<p class="faint none">No badge matches “{query}”.</p>
{:else}
	<ul class="list list-pane">
		{#each shown as badge (badge.id)}
			<li>
				<img
					src={api.badgeImage(badge.id, 'small', badge.updated_at)}
					alt=""
					width="48"
					height="48"
					loading="lazy"
				/>
				<span class="label">
					<strong>{badge.name}</strong>
					{#if badge.description}<span class="about">{badge.description}</span>{/if}
					<span class="meta">
						<span class="tag" class:auto={badge.rule || badge.condition}>{how(badge)}</span>
						<button class="held" onclick={() => (giving = badge)} title="Who holds it">
							<Icon name="users" size={13} />
							{count(badge.holders.length)}
						</button>
					</span>
				</span>
				<span class="controls">
					<button class="btn ghost" onclick={() => (giving = badge)}>Give</button>
					<button
						class="btn ghost"
						aria-label="Edit {badge.name}"
						onclick={() => (editing = badge)}
					>
						<Icon name="pencil" size={15} /> Edit
					</button>
					{#if !badge.rule}
						<button
							class="btn ghost danger"
							aria-label="Delete {badge.name}"
							onclick={() => remove(badge)}
						>
							<Icon name="trash" size={15} />
						</button>
					{/if}
				</span>
			</li>
		{/each}
	</ul>
{/if}

{#if editing !== undefined}
	<BadgeEditor
		badge={editing}
		onclose={() => (editing = undefined)}
		onsaved={() => {
			editing = undefined;
			void load();
		}}
	/>
{/if}

{#if giving || person}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div
		class="scrim"
		onclick={() => {
			giving = null;
			person = null;
		}}
	></div>
	<div class="picker card" role="dialog" aria-modal="true" aria-labelledby="badge-picker-title">
		<div class="picker-head">
			<h3 id="badge-picker-title">
				{giving ? `Who holds ${giving.name}` : `${person?.username}'s badges`}
			</h3>
			<button
				class="btn ghost small"
				aria-label="Close"
				onclick={() => {
					giving = null;
					person = null;
				}}
			>
				<Icon name="x" size={16} />
			</button>
		</div>
		{#if giving?.condition}
			<p class="faint small hint">
				Frog gives it again to anyone still at {conditionText(giving.condition)}.
			</p>
		{/if}
		<ul class="choices">
			{#if giving}
				{#each users as user (user.id)}
					{@const on = giving.holders.includes(user.id)}
					<li>
						<label class="choice">
							<input
								type="checkbox"
								checked={on}
								onchange={() => giving && toggle(giving, user.id)}
							/>
							<span>{user.username}</span>
							{#if user.disabled}<span class="faint small">Switched off</span>{/if}
						</label>
					</li>
				{/each}
			{:else if person}
				{#each theirs as badge (badge.id)}
					{@const on = badge.holders.includes(person.id)}
					<li>
						<label class="choice">
							<input
								type="checkbox"
								checked={on}
								onchange={() => person && toggle(badge, person.id)}
							/>
							<img
								src={api.badgeImage(badge.id, 'small', badge.updated_at)}
								alt=""
								width="26"
								height="26"
							/>
							<span>{badge.name}</span>
						</label>
					</li>
				{:else}
					<li class="faint small">No badges yet.</li>
				{/each}
			{/if}
		</ul>
	</div>
{/if}

<style>
	.summary {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr)) auto;
		align-items: stretch;
		gap: 12px;
		margin-bottom: 16px;
	}

	.stat {
		display: grid;
		gap: 2px;
		padding: 14px 18px;
	}

	.stat strong {
		font-family: var(--font-display);
		font-size: 1.5rem;
		font-variant-numeric: tabular-nums;
	}

	.new {
		align-self: center;
	}

	.tools {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		margin-bottom: 12px;
	}

	.segmented .faint {
		margin-left: 4px;
		font-weight: 500;
	}

	.searchbar {
		flex: 1 1 220px;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 14px;
		background: var(--bg-elev);
		border: 1px solid var(--line-strong);
		border-radius: var(--radius-pill);
		color: var(--text-faint);
	}

	.searchbar:focus-within {
		border-color: var(--accent);
	}

	.searchbar input {
		flex: 1;
		min-width: 0;
		padding: 9px 0;
		background: none;
		border: none;
		outline: none;
		color: var(--text);
		font: inherit;
	}

	.lead {
		margin: 0 0 12px;
		font-size: 0.9rem;
	}

	.waiting {
		display: flex;
		gap: 10px;
		align-items: flex-start;
		margin: 0 0 12px;
		padding: 12px 14px;
		color: var(--text-dim);
		font-size: 0.9rem;
	}

	.none {
		margin: 12px 0 0;
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.list li {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 12px 14px;
		min-width: 0;
	}

	.list li + li {
		border-top: 1px solid var(--line);
	}

	.list img {
		flex: none;
		width: 48px;
		height: 48px;
	}

	.label {
		flex: 1;
		display: grid;
		gap: 3px;
		min-width: 0;
	}

	.label strong {
		overflow-wrap: anywhere;
	}

	.about {
		color: var(--text-dim);
		font-size: 0.86rem;
	}

	.meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 10px;
		font-size: 0.78rem;
	}

	.tag {
		padding: 1px 8px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		color: var(--text-dim);
		font-weight: 600;
	}

	.tag.auto {
		background: color-mix(in srgb, var(--accent) 14%, transparent);
		color: var(--accent);
	}

	.held {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0;
		border: 0;
		background: none;
		color: var(--text-faint);
		font: inherit;
		cursor: pointer;
	}

	.held:hover {
		color: var(--text);
	}

	.controls {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 4px;
		flex: none;
	}

	.scrim {
		position: fixed;
		inset: 0;
		z-index: 300;
		background: rgb(0 0 0 / 0.5);
	}

	.picker {
		position: fixed;
		z-index: 301;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(380px, calc(100vw - 32px));
		max-height: 80vh;
		display: flex;
		flex-direction: column;
		padding: 16px;
		background: var(--bg-float);
	}

	.picker-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-bottom: 8px;
	}

	.picker-head h3 {
		margin: 0;
		font-size: 1rem;
		overflow-wrap: anywhere;
	}

	.hint {
		margin: 0 0 8px;
	}

	.choices {
		list-style: none;
		margin: 0;
		padding: 0;
		overflow-y: auto;
	}

	.choice {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 9px 4px;
		cursor: pointer;
		min-width: 0;
	}

	.choice span:not(.small) {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	@media (max-width: 760px) {
		.summary {
			grid-template-columns: repeat(3, minmax(0, 1fr));
		}

		.new {
			grid-column: 1 / -1;
			justify-self: start;
		}
	}

	@media (max-width: 600px) {
		.stat {
			padding: 10px 12px;
		}

		.stat strong {
			font-size: 1.2rem;
		}

		.list li {
			flex-wrap: wrap;
		}

		.controls {
			width: 100%;
			justify-content: flex-start;
		}
	}
</style>
