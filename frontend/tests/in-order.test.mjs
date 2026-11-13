// Saves of one thing go out in the order they were made. Run with `npm test`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { InOrder } from '../src/lib/in-order.ts';

const later = (ms, value, log) =>
	new Promise((resolve) =>
		setTimeout(() => {
			log.push(value);
			resolve(value);
		}, ms)
	);

test('a slow first save still lands before a quick second one', async () => {
	const queue = new InOrder();
	const log = [];
	const first = queue.run(() => later(40, 'first', log));
	const second = queue.run(() => later(1, 'second', log));
	assert.equal(await second.done, 'second');
	assert.deepEqual(log, ['first', 'second']);
	assert.equal(queue.newest(first.ticket), false);
	assert.equal(queue.newest(second.ticket), true);
});

test('a failed save does not hold up the next', async () => {
	const queue = new InOrder();
	const failed = queue.run(() => Promise.reject(new Error('offline')));
	const next = queue.run(() => Promise.resolve(80));
	await assert.rejects(failed.done, /offline/);
	assert.equal(await next.done, 80);
	assert.equal(queue.newest(failed.ticket), false, 'a newer save has its say');
});
