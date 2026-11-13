<script lang="ts">
	// Pip, Frog's mascot, as Frog's own icon draws it.
	let { size = 32 }: { size?: number } = $props();
</script>

<svg viewBox="0 0 64 64" width={size} height={size} aria-hidden="true">
	<ellipse cx="32" cy="38" rx="25" ry="20" fill="#6cc46b" />
	<ellipse cx="32" cy="45" rx="15.5" ry="11" fill="#8ad88a" opacity="0.45" />
	<circle cx="19" cy="20" r="12" fill="#6cc46b" />
	<circle cx="45" cy="20" r="12" fill="#6cc46b" />
	<ellipse cx="12.5" cy="40" rx="5" ry="3.3" fill="#f4738f" opacity="0.82" />
	<ellipse cx="51.5" cy="40" rx="5" ry="3.3" fill="#f4738f" opacity="0.82" />
	<circle cx="19" cy="20" r="8" fill="#fbfaf1" />
	<circle cx="45" cy="20" r="8" fill="#fbfaf1" />
	<circle cx="20.2" cy="21" r="4.2" fill="#11231a" />
	<circle cx="43.8" cy="21" r="4.2" fill="#11231a" />
	<circle cx="18.2" cy="18.8" r="1.7" fill="#ffffff" />
	<circle cx="41.8" cy="18.8" r="1.7" fill="#ffffff" />
	<polyline
		points="23,41.5 25.57,43.7 28.14,45.17 30.71,45.91 32,46 33.29,45.91 35.86,45.17 38.43,43.7 41,41.5"
		fill="none"
		stroke="#2c6b40"
		stroke-width="3"
		stroke-linecap="round"
		stroke-linejoin="round"
	/>
</svg>
