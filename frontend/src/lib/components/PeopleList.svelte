<script lang="ts">
	import Avatar from './Avatar.svelte';
	import { avatarOf, matching, personName } from '#lib/people.svelte';
	import type { Choosable } from '#lib/types';

	/*
	 * People to tick: the Recommend sheet's list, and the "Watched with" row.
	 * A search box only once the list is long enough to need one.
	 */

	interface Props {
		people: Choosable[] | null;
		picked: number[];
		/** Shown beside someone who already has this title from you. */
		sentLabel?: string;
		label: string;
	}

	let { people, picked = $bindable(), sentLabel, label }: Props = $props();

	/** More than this many, and a search box helps. */
	const SEARCH_FROM = 8;

	let typed = $state('');
	const shown = $derived(matching(people ?? [], typed));

	function toggle(id: number, on: boolean) {
		picked = on ? [...picked.filter((p) => p !== id), id] : picked.filter((p) => p !== id);
	}
</script>

{#if people === null}
	<p class="faint loading">Loading</p>
{:else}
	{#if people.length > SEARCH_FROM}
		<input class="input search" type="search" placeholder="Find someone" bind:value={typed} />
	{/if}
	<ul class="people" aria-label={label}>
		{#each shown as p (p.id)}
			<li>
				<label class="person">
					<input
						type="checkbox"
						checked={picked.includes(p.id)}
						onchange={(e) => toggle(p.id, e.currentTarget.checked)}
					/>
					<Avatar src={avatarOf(p)} name={personName(p)} size={30} />
					<span class="name">
						<strong>{personName(p)}</strong>
						{#if p.display_name}<small>@{p.username}</small>{/if}
					</span>
					{#if p.sent && sentLabel}<small class="sent">{sentLabel}</small>{/if}
				</label>
			</li>
		{:else}
			<li class="faint none">Nobody by that name</li>
		{/each}
	</ul>
{/if}

<style>
	.loading,
	.none {
		padding: 10px 12px;
		font-size: 0.86rem;
	}

	.search {
		width: 100%;
		margin-bottom: 8px;
	}

	.people {
		list-style: none;
		margin: 0;
		padding: 0;
		max-height: 260px;
		overflow-y: auto;
		border: 1px solid var(--line);
		border-radius: var(--radius-sm);
	}

	.people li + li {
		border-top: 1px solid var(--line);
	}

	.person {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 48px;
		padding: 8px 12px;
		cursor: pointer;
	}

	.person input {
		width: 18px;
		height: 18px;
		flex: none;
		accent-color: var(--accent);
	}

	.name {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
	}

	.name strong {
		font-size: 0.9rem;
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.name small,
	.sent {
		font-size: 0.74rem;
		color: var(--text-faint);
	}

	.sent {
		flex: none;
	}
</style>
