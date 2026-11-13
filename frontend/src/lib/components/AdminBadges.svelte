<script lang="ts">
	import { api } from '#lib/api';
	import { backCloses } from '#lib/layers.svelte';
	import { toasts } from '#lib/toast.svelte';
	import type { AdminBadge, AdminUser } from '#lib/types';
	import BadgeEditor from './BadgeEditor.svelte';
	import Icon from './Icon.svelte';

	interface Props {
		users: AdminUser[];
	}

	let { users }: Props = $props();

	let badges = $state<AdminBadge[]>([]);
	let loaded = $state(false);
	/** The editor: open on a badge, on a new one (null), or shut (undefined). */
	let editing = $state<AdminBadge | null | undefined>(undefined);
	/** Giving one badge to people, or one person their badges. */
	let giving = $state<AdminBadge | null>(null);
	let person = $state<AdminUser | null>(null);

	backCloses(
		() => !!giving || !!person,
		() => {
			giving = null;
			person = null;
		}
	);

	async function load() {
		try {
			badges = (await api.badges()).badges;
		} catch (e) {
			toasts.error(e);
		} finally {
			loaded = true;
		}
	}
	load();

	const count = (n: number) => (n === 1 ? '1 person' : `${n} people`);
	const nameOf = (id: number) => users.find((u) => u.id === id)?.username ?? 'someone';

	/** Opened from a person's row in People. */
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

<section class="block" id="badges">
	<div class="block-head">
		<h2>Badges</h2>
		<button class="btn ghost small" onclick={() => (editing = null)}>
			<Icon name="plus" size={15} /> New badge
		</button>
	</div>
	{#if loaded && !badges.length}
		<p class="faint empty">Make a badge, then give it to the people who earned it.</p>
	{:else}
		<ul class="list list-pane">
			{#each badges as badge (badge.id)}
				<li>
					<img
						src={api.badgeImage(badge.id, 'small', badge.updated_at)}
						alt=""
						width="40"
						height="40"
					/>
					<span class="label">
						<strong>{badge.name}</strong>
						<span class="faint">
							{badge.description ? `${badge.description} · ` : ''}{count(badge.holders.length)}
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
						<button
							class="btn ghost danger"
							aria-label="Delete {badge.name}"
							onclick={() => remove(badge)}
						>
							<Icon name="trash" size={15} />
						</button>
					</span>
				</li>
			{/each}
		</ul>
	{/if}
</section>

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
				{giving ? `Give ${giving.name}` : `${person?.username}'s badges`}
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
				{#each badges as badge (badge.id)}
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
					<li class="faint small">No badges yet. Make one under Badges.</li>
				{/each}
			{/if}
		</ul>
	</div>
{/if}

<style>
	.block-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.empty {
		margin: 8px 0 0;
	}

	.list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.list li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 12px;
		min-width: 0;
	}

	.list li + li {
		border-top: 1px solid var(--line);
	}

	.list img {
		flex: none;
		width: 40px;
		height: 40px;
	}

	.label {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.label strong,
	.label .faint {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.label .faint {
		font-size: 0.82rem;
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

	@media (max-width: 600px) {
		.list li {
			flex-wrap: wrap;
		}

		.controls {
			width: 100%;
			justify-content: flex-start;
		}
	}
</style>
