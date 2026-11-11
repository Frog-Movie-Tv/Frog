/**
 * Moves an element to the end of `<body>` while it is shown, so no ancestor
 * that clips (`overflow: hidden`) or makes a new containing block
 * (`backdrop-filter`, `transform`) can cut it off. Use as `use:portal`.
 */
export function portal(node: HTMLElement) {
	document.body.appendChild(node);
	return {
		destroy() {
			node.remove();
		}
	};
}
