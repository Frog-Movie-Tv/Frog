/**
 * Saves of one thing, sent one after another in the order they were made.
 *
 * Two quick taps on a rating used to send two requests at once: the server
 * could apply them in either order, and whichever answer came back last was
 * shown, so the screen could settle on a score that was not saved. Here each
 * save waits for the one before it, and only the newest one's answer (or
 * failure) is worth acting on; an older one is out of date when it lands.
 */
export class InOrder {
	private tail: Promise<unknown> = Promise.resolve();
	private made = 0;

	/** Run `task` after every task given before it, failed or not. */
	run<T>(task: () => Promise<T>): { ticket: number; done: Promise<T> } {
		const ticket = ++this.made;
		const done = this.tail.then(task, task);
		this.tail = done.catch(() => undefined);
		return { ticket, done };
	}

	/** Whether no task was given after the one with this ticket. */
	newest(ticket: number): boolean {
		return ticket === this.made;
	}
}
