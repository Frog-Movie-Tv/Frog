<script lang="ts">
	interface Props {
		/** Address of an uploaded picture; the initial stands in without one. */
		src?: string | null;
		name: string;
		size?: number;
	}

	let { src = null, name, size = 40 }: Props = $props();
</script>

<span class="avatar" style:--size="{size}px" aria-hidden="true">
	{#if src}
		<img {src} alt="" width={size} height={size} />
	{:else}
		{name.slice(0, 1).toUpperCase()}
	{/if}
</span>

<style>
	.avatar {
		width: var(--size);
		height: var(--size);
		border-radius: 50%;
		display: grid;
		place-items: center;
		overflow: hidden;
		flex: none;
		/* The accent's soft tint is mostly see-through. On an opaque base, so a
		   banner behind it stays behind it, in every theme. */
		background: linear-gradient(var(--accent-soft), var(--accent-soft)), var(--bg-elev);
		color: var(--accent);
		font-weight: 700;
		font-size: calc(var(--size) * 0.42);
	}

	img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
</style>
