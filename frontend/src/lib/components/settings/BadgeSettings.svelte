<script lang="ts">
	import { api } from '#lib/api';
	import { prettyDate } from '#lib/format';
	import { moved, sortable } from '#lib/sortable';
	import { toasts } from '#lib/toast.svelte';
	import type { HeldBadge } from '#lib/types';
	import Icon from '../Icon.svelte';

	let badges = $state<HeldBadge[]>([]);
	let loaded = $state(false);

	async function load() {
		try {
			badges = (await api.myBadges()).badges;
		} catch (e) {
			toasts.error(e);
		} finally {
			loaded = true;
		}
	}
	load();

	async function save() {
		try {
			badges = (await api.arrangeBadges(badges.map((b) => ({ id: b.id, shown: b.shown })))).badges;
		} catch (e) {
			toasts.error(e);
			await load();
		}
	}

	const shownCount = $derived(badges.filter((b) => b.shown).length);
</script>

{#if loaded && badges.length}
	<div class="setting">
		<div class="setting-top">
			<span class="setting-tile" aria-hidden="true"><Icon name="sparkle" size={18} /></span>
			<span class="setting-label">
				<strong>Badges</strong>
				<span class="faint">
					Drag to order them. The first three sit beside your name{shownCount > 3
						? ', the rest one tap away'
						: ''}.
				</span>
			</span>
		</div>
		<ol
			class="setting-body"
			use:sortable={{ onmove: (from, to) => (badges = moved(badges, from, to)), onend: save }}
		>
			{#each badges as badge (badge.id)}
				<li data-sort-item class:hidden={!badge.shown}>
					<button class="handle" data-sort-handle aria-label="Move {badge.name}">
						<Icon name="grip" size={16} />
					</button>
					<img src={badge.small} alt="" width="34" height="34" />
					<span class="label">
						<strong>{badge.name}</strong>
						<span class="faint">Given {prettyDate(badge.awarded_at)}</span>
					</span>
					<label class="show">
						<span class="sr">Show {badge.name} on your profile</span>
						<input
							type="checkbox"
							class="toggle-switch"
							checked={badge.shown}
							onchange={(e) => {
								badge.shown = e.currentTarget.checked;
								void save();
							}}
						/>
					</label>
				</li>
			{/each}
		</ol>
	</div>
{/if}

<style>
	ol {
		list-style: none;
		margin-block: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	li {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 8px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		min-width: 0;
	}

	li:global([data-dragging]) {
		box-shadow: var(--shadow-3);
	}

	li.hidden img,
	li.hidden .label {
		opacity: 0.5;
	}

	.handle {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border: 0;
		border-radius: var(--radius-sm);
		background: none;
		color: var(--text-dim);
		cursor: grab;
		touch-action: none;
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
		font-size: 0.8rem;
	}

	.sr {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip-path: inset(50%);
	}
</style>
