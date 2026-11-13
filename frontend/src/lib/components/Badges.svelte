<script lang="ts">
	import { prettyDate } from '#lib/format';
	import { backCloses } from '#lib/layers.svelte';
	import { portal } from '#lib/portal';
	import type { HeldBadge } from '#lib/types';
	import Icon from './Icon.svelte';

	interface Props {
		badges: HeldBadge[];
		/** How large each is drawn, in pixels. */
		size?: number;
		/** How many show before "+N". */
		most?: number;
		/** Pictures only, for inside a link: the card opens on the profile. */
		still?: boolean;
	}

	let { badges, size = 26, most = 3, still = false }: Props = $props();

	const shown = $derived(badges.slice(0, most));
	const more = $derived(badges.length - shown.length);
	let open = $state<number | null>(null);
	const current = $derived(open === null ? null : badges[open]);

	backCloses(
		() => open !== null,
		() => (open = null)
	);

	function key(event: KeyboardEvent) {
		if (open === null) return;
		if (event.key === 'Escape') open = null;
		else if (event.key === 'ArrowRight') open = Math.min(badges.length - 1, open + 1);
		else if (event.key === 'ArrowLeft') open = Math.max(0, open - 1);
	}
</script>

<svelte:window onkeydown={key} />

{#if badges.length}
	<span class="badges" style:--size="{size}px">
		{#each shown as b, i (b.id)}
			{#if still}
				<img src={b.small} alt={b.name} title={b.name} width={size} height={size} />
			{:else}
				<button class="badge" aria-label={b.name} title={b.name} onclick={() => (open = i)}>
					<img src={b.small} alt="" width={size} height={size} />
				</button>
			{/if}
		{/each}
		{#if more > 0}
			{#if still}
				<span class="more" aria-label="{more} more {more === 1 ? 'badge' : 'badges'}">+{more}</span>
			{:else}
				<button
					class="more"
					aria-label="{more} more {more === 1 ? 'badge' : 'badges'}"
					onclick={() => (open = most)}>+{more}</button
				>
			{/if}
		{/if}
	</span>
{/if}

{#if current && open !== null}
	<!-- At the end of the page, so no card's own layer can sit over it. -->
	<div use:portal>
		<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
		<div class="scrim" onclick={() => (open = null)}></div>
		<div class="card sheet" role="dialog" aria-modal="true" aria-labelledby="badge-card-name">
			<button class="btn ghost small close" aria-label="Close" onclick={() => (open = null)}>
				<Icon name="x" size={16} />
			</button>
			<img class="large" src={current.large} alt="" />
			<h3 id="badge-card-name">{current.name}</h3>
			{#if current.description}<p class="about">{current.description}</p>{/if}
			<p class="faint small">Given {prettyDate(current.awarded_at)}</p>
			{#if badges.length > 1}
				<div class="all" role="group" aria-label="Badges">
					{#each badges as b, i (b.id)}
						<button
							class="badge"
							class:on={i === open}
							aria-label={b.name}
							aria-pressed={i === open}
							onclick={() => (open = i)}
						>
							<img src={b.small} alt="" width="30" height="30" />
						</button>
					{/each}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.badges {
		display: inline-flex;
		align-items: center;
		gap: calc(var(--size) * 0.22);
		flex: none;
		vertical-align: middle;
	}

	.badges img,
	.badge {
		width: var(--size);
		height: var(--size);
	}

	.badge {
		display: inline-grid;
		place-items: center;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: none;
		cursor: pointer;
		transition: transform var(--fast) var(--ease);
	}

	.badge:hover {
		transform: scale(1.08);
	}

	.badge:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	.more {
		min-width: var(--size);
		height: var(--size);
		padding: 0 6px;
		display: inline-grid;
		place-items: center;
		border-radius: 999px;
		border: 1px solid var(--line-strong);
		background: var(--bg-elev);
		color: var(--text-dim);
		font: inherit;
		font-size: max(0.7rem, calc(var(--size) * 0.42));
		font-weight: 650;
		cursor: pointer;
	}

	span.more {
		cursor: default;
	}

	.scrim {
		position: fixed;
		inset: 0;
		z-index: 300;
		background: rgb(0 0 0 / 0.5);
	}

	.sheet {
		position: fixed;
		z-index: 301;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(360px, calc(100vw - 32px));
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 26px 20px 20px;
		text-align: center;
		background: var(--bg-float);
		animation: open 200ms var(--spring) backwards;
	}

	@keyframes open {
		from {
			opacity: 0;
			transform: translate(-50%, -48%) scale(0.96);
		}
	}

	.close {
		position: absolute;
		top: 8px;
		right: 8px;
	}

	.large {
		width: min(240px, 62vw);
		height: auto;
		aspect-ratio: 1;
		margin-bottom: 8px;
	}

	h3 {
		margin: 0;
		font-size: 1.2rem;
		overflow-wrap: anywhere;
	}

	.about {
		margin: 0;
		color: var(--text-dim);
		overflow-wrap: anywhere;
	}

	.all {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 8px;
		margin-top: 10px;
	}

	.all .badge {
		width: 36px;
		height: 36px;
		opacity: 0.6;
	}

	.all .badge.on {
		opacity: 1;
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}

	@media (prefers-reduced-motion: reduce) {
		.sheet {
			animation: none;
		}
	}
</style>
