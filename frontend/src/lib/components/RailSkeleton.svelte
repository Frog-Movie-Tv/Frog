<script lang="ts">
	interface Props {
		wide?: boolean;
		count?: number;
	}

	let { wide = false, count = 6 }: Props = $props();
</script>

<section class="rail" class:wide aria-hidden="true">
	<div class="head skeleton"></div>
	<div class="track">
		{#each Array(count) as _, i (i)}
			<div class="card">
				<div class="art skeleton"></div>
				<div class="line skeleton"></div>
				<div class="line short skeleton"></div>
			</div>
		{/each}
	</div>
</section>

<style>
	.rail {
		margin-bottom: 40px;
	}

	.head {
		width: 180px;
		height: 20px;
		margin-bottom: 16px;
	}

	.track {
		display: grid;
		grid-auto-flow: column;
		grid-auto-columns: var(--poster-min);
		gap: var(--gap);
		overflow: hidden;
	}

	.rail.wide .track {
		grid-auto-columns: min(86vw, 340px);
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.art {
		aspect-ratio: 2 / 3;
		border-radius: var(--radius-sm);
	}

	.rail.wide .art {
		aspect-ratio: 16 / 9;
	}

	.line {
		height: 11px;
		border-radius: 4px;
	}

	.line.short {
		width: 45%;
	}
</style>
