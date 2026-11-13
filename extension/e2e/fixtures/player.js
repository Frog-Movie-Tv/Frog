// What the tests use to drive a fixture's player. Not part of the extension.
(() => {
	const deep = (root) => {
		const found = root.querySelector('video');
		if (found) return found;
		for (const el of root.querySelectorAll('*')) {
			if (el.shadowRoot) {
				const inner = deep(el.shadowRoot);
				if (inner) return inner;
			}
		}
		return null;
	};
	window.video = () => deep(document);
	/** Play, and jump to a point in it (0 to 1) once its length is known. */
	window.playAt = async (fraction) => {
		const v = window.video();
		v.muted = true;
		if (!(v.duration > 0))
			await new Promise((ok) => v.addEventListener('loadedmetadata', ok, { once: true }));
		if (fraction != null) v.currentTime = v.duration * fraction;
		await v.play();
		return { duration: v.duration, src: v.currentSrc };
	};
})();
