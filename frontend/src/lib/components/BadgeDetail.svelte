<script lang="ts">
	import { DIFFICULTY, LADDERS, heldBy, share, type BadgeCard } from '#lib/badge-board';
	import { prettyDate } from '#lib/format';
	import { backCloses } from '#lib/layers.svelte';
	import { portal } from '#lib/portal';
	import Icon from './Icon.svelte';

	interface Props {
		card: BadgeCard;
		/** Every step of its ladder, bottom to top, when it is on one. */
		ladder: BadgeCard[];
		accounts: number;
		/** The page is the viewer's own. */
		own: boolean;
		/** Whether it can sit beside the name: earned, and the top step held of its ladder. */
		pinnable: boolean;
		pinned: boolean;
		onpick: (card: BadgeCard) => void;
		onclose: () => void;
		onpin: (card: BadgeCard, pinned: boolean) => void;
	}

	let { card, ladder, accounts, own, pinnable, pinned, onpick, onclose, onpin }: Props = $props();

	backCloses(
		() => true,
		() => onclose()
	);

	const count = (n: number) => n.toLocaleString();
	const day = (d: string) => prettyDate(`${d}T12:00:00`);

	function key(event: KeyboardEvent) {
		if (event.key === 'Escape') onclose();
		const at = ladder.findIndex((c) => c.id === card.id);
		if (at < 0) return;
		if (event.key === 'ArrowRight' && at < ladder.length - 1) onpick(ladder[at + 1]);
		if (event.key === 'ArrowLeft' && at > 0) onpick(ladder[at - 1]);
	}
</script>

<svelte:window onkeydown={key} />

<div use:portal>
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div class="scrim" onclick={onclose}></div>
	<div class="card sheet" role="dialog" aria-modal="true" aria-labelledby="badge-detail-name">
		<button class="btn ghost small close" aria-label="Close" onclick={onclose}>
			<Icon name="x" size={16} />
		</button>

		<div class="head">
			<!-- The small one stands in while the large one is drawn. -->
			<img
				class="large"
				class:locked={!card.earned}
				src={card.large}
				style:background-image="url({card.small})"
				alt=""
			/>
			<div class="titles">
				{#if card.ladder && card.step !== null && card.steps}
					<span class="eyebrow">
						{LADDERS[card.ladder] ?? card.ladder} · step {card.step + 1} of {card.steps}
					</span>
				{/if}
				<h3 id="badge-detail-name">{card.name}</h3>
				{#if card.description}<p class="about">{card.description}</p>{/if}
			</div>
		</div>

		<dl class="facts">
			{#if card.earned && card.held}
				<div>
					<dt>{card.held.automatic ? 'Earned' : 'Given'}</dt>
					<dd>
						{card.held.earned_on ? day(card.held.earned_on) : prettyDate(card.held.awarded_at)}
						{#if card.held.earned_with}<span class="faint">with {card.held.earned_with}</span>{/if}
					</dd>
				</div>
			{:else if card.progress}
				<div>
					<dt>Progress</dt>
					<dd class="progress">
						<span class="numbers">{count(card.progress.have)} of {count(card.progress.need)}</span>
						<span
							class="meter"
							role="progressbar"
							aria-label="{card.name}: {count(card.progress.have)} of {count(card.progress.need)}"
							aria-valuemin={0}
							aria-valuemax={card.progress.need}
							aria-valuenow={card.progress.have}
						>
							<span style:width="{share(card) * 100}%"></span>
						</span>
					</dd>
				</div>
			{:else}
				<div>
					<dt>Status</dt>
					<dd>Still to earn</dd>
				</div>
			{/if}
			{#if card.difficulty}
				<div>
					<dt>Difficulty</dt>
					<dd class="difficulty">
						<span class="pips" aria-hidden="true">
							{#each [1, 2, 3, 4, 5] as n (n)}<span class:on={n <= (card.difficulty ?? 0)}
								></span>{/each}
						</span>
						{DIFFICULTY[card.difficulty - 1]}
					</dd>
				</div>
			{/if}
			{#if card.group !== 'given'}
				<div>
					<dt>What counts</dt>
					<dd>
						{card.moment
							? 'Only what you log as it happens, on days that are over'
							: 'Everything in your library, imports included'}
					</dd>
				</div>
			{/if}
			<div>
				<dt>Here</dt>
				<dd>{heldBy(card.holders, accounts)}</dd>
			</div>
		</dl>

		{#if ladder.length > 1}
			<div class="ladder" role="group" aria-label="Every step">
				{#each ladder as step (step.id)}
					<button
						class="step"
						class:on={step.id === card.id}
						class:locked={!step.earned}
						aria-label="{step.name}{step.earned ? ', earned' : ''}"
						aria-pressed={step.id === card.id}
						title={step.name}
						onclick={() => onpick(step)}
					>
						<img src={step.small} alt="" width="34" height="34" />
					</button>
				{/each}
			</div>
		{/if}

		{#if own && card.earned && card.held && pinnable}
			<label class="choice">
				<span>
					<strong>Beside your name</strong>
					<span class="faint"
						>Wherever your name shows. Your Badges tab shows every one either way.</span
					>
				</span>
				<input
					type="checkbox"
					class="toggle-switch"
					checked={pinned}
					onchange={(e) => onpin(card, e.currentTarget.checked)}
				/>
			</label>
		{/if}
	</div>
</div>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 300;
		background: rgb(0 0 0 / 0.5);
		animation: fade 160ms var(--ease) backwards;
	}

	.sheet {
		position: fixed;
		z-index: 301;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(460px, calc(100vw - 32px));
		max-height: calc(100dvh - 48px);
		overflow-y: auto;
		display: grid;
		gap: 16px;
		padding: 24px;
		background: var(--bg-float);
		animation: open 220ms var(--spring) backwards;
	}

	@keyframes open {
		from {
			opacity: 0;
			transform: translate(-50%, -48%) scale(0.96);
		}
	}

	@keyframes fade {
		from {
			opacity: 0;
		}
	}

	.close {
		position: absolute;
		top: 10px;
		right: 10px;
	}

	.head {
		display: grid;
		justify-items: center;
		gap: 12px;
		text-align: center;
	}

	.large {
		width: min(180px, 46vw);
		height: auto;
		aspect-ratio: 1;
		background: center / contain no-repeat;
		filter: drop-shadow(0 10px 24px rgb(0 0 0 / 0.25));
	}

	.large.locked {
		filter: grayscale(1);
		opacity: 0.5;
	}

	.titles {
		display: grid;
		gap: 4px;
	}

	.eyebrow {
		color: var(--text-faint);
		font-size: 0.78rem;
		font-weight: 600;
		letter-spacing: 0.02em;
	}

	h3 {
		margin: 0;
		font-size: 1.3rem;
		overflow-wrap: anywhere;
	}

	.about {
		margin: 0;
		color: var(--text-dim);
		overflow-wrap: anywhere;
	}

	.facts {
		margin: 0;
		display: grid;
		gap: 0;
		border-top: 1px solid var(--line);
	}

	.facts > div {
		display: grid;
		grid-template-columns: 7.5rem minmax(0, 1fr);
		gap: 12px;
		padding: 10px 0;
		border-bottom: 1px solid var(--line);
		font-size: 0.9rem;
	}

	dt {
		color: var(--text-faint);
	}

	dd {
		margin: 0;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px 8px;
	}

	.progress {
		display: grid;
		gap: 6px;
	}

	.numbers {
		font-variant-numeric: tabular-nums;
	}

	.meter {
		display: block;
		height: 6px;
		border-radius: 999px;
		background: var(--bg-elev-2);
		overflow: hidden;
	}

	.meter > span {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--accent);
	}

	.pips {
		display: inline-flex;
		gap: 3px;
	}

	.pips span {
		width: 14px;
		height: 6px;
		border-radius: 999px;
		background: var(--bg-elev-2);
	}

	.pips span.on {
		background: var(--accent);
	}

	.ladder {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 6px;
	}

	.step {
		display: grid;
		place-items: center;
		padding: 3px;
		border: 0;
		border-radius: 50%;
		background: none;
		cursor: pointer;
		transition: transform var(--fast) var(--ease);
	}

	.step:hover {
		transform: scale(1.08);
	}

	.step.locked img {
		filter: grayscale(1);
		opacity: 0.45;
	}

	.step.on {
		outline: 2px solid var(--accent);
		outline-offset: 1px;
	}

	.choice {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 10px 12px;
		border-radius: var(--radius-sm);
		background: var(--bg-elev);
		cursor: pointer;
	}

	.choice > span {
		display: grid;
		gap: 2px;
	}

	.choice .faint {
		font-size: 0.82rem;
	}

	/* A phone: a sheet from the bottom. */
	@media (max-width: 600px) {
		.sheet {
			top: auto;
			bottom: 0;
			left: 0;
			transform: none;
			width: 100%;
			max-height: 88dvh;
			border-radius: var(--radius) var(--radius) 0 0;
			padding: 22px 18px calc(18px + env(safe-area-inset-bottom));
			animation-name: rise;
		}

		@keyframes rise {
			from {
				transform: translateY(24px);
				opacity: 0;
			}
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.sheet,
		.scrim {
			animation: none;
		}
	}
</style>
