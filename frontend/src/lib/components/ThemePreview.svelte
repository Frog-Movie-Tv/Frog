<script lang="ts">
	interface Props {
		/** The theme's page, a pane on it and its accent, in that order. */
		swatches: string[];
		/** Ringed: the theme in use. */
		on?: boolean;
	}

	let { swatches, on = false }: Props = $props();
</script>

<!-- A little window in a theme's own colours, the way a Mac shows its
     appearances: Settings and the welcome both choose a theme with it. -->
<span class="preview" class:on aria-hidden="true">
	<i class="page" style:background={swatches[0]}></i>
	<i class="pane" style:background={swatches[1]}></i>
	<i class="mark" style:background={swatches[2]}></i>
</span>

<style>
	.preview {
		position: relative;
		display: block;
		width: 100%;
		aspect-ratio: 4 / 3;
		overflow: hidden;
		border-radius: 12px;
		box-shadow:
			0 0 0 1px var(--line),
			var(--shadow-1);
		transition:
			box-shadow 200ms var(--ease),
			transform 200ms var(--ease);
	}

	.preview.on {
		box-shadow:
			0 0 0 2px var(--bg),
			0 0 0 4px var(--accent);
	}

	i {
		position: absolute;
	}

	.page {
		inset: 0;
	}

	.pane {
		inset: 30% 12% -8%;
		border-radius: 8px 8px 0 0;
		box-shadow: 0 -2px 10px rgba(0, 0, 0, 0.18);
	}

	.mark {
		top: 46%;
		left: 22%;
		width: 36%;
		height: 11%;
		border-radius: 99px;
	}
</style>
