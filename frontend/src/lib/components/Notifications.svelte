<script lang="ts">
	import { api } from '#lib/api';
	import Icon from '#lib/components/Icon.svelte';
	import { toasts } from '#lib/toast.svelte';

	type Step = 'loading' | 'unsupported' | 'insecure' | 'blocked' | 'off' | 'on';

	let step = $state<Step>('loading');
	let devices = $state(0);
	let busy = $state(false);

	function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
		const padded = base64url.replace(/-/g, '+').replace(/_/g, '/').padEnd(Math.ceil(base64url.length / 4) * 4, '=');
		const raw = atob(padded);
		const bytes = new Uint8Array(new ArrayBuffer(raw.length));
		for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
		return bytes;
	}

	/** The worker, or null if it never comes up (plain HTTP, private windows). */
	async function worker(): Promise<ServiceWorkerRegistration | null> {
		if (!('serviceWorker' in navigator)) return null;
		const timeout = new Promise<null>((r) => setTimeout(() => r(null), 4000));
		return Promise.race([navigator.serviceWorker.ready, timeout]);
	}

	async function load() {
		if (!window.isSecureContext) {
			step = 'insecure';
			return;
		}
		if (!('PushManager' in window) || !('Notification' in window)) {
			step = 'unsupported';
			return;
		}
		if (Notification.permission === 'denied') {
			step = 'blocked';
			return;
		}
		try {
			const [status, reg] = await Promise.all([api.pushStatus(), worker()]);
			devices = status.devices;
			if (!reg) {
				step = 'unsupported';
				return;
			}
			const sub = await reg.pushManager.getSubscription();
			if (sub && Notification.permission === 'granted') {
				// Re-send it: the server may have dropped it, or the keys rotated.
				await api.pushSubscribe(sub.toJSON());
				devices = (await api.pushStatus()).devices;
				step = 'on';
			} else {
				step = 'off';
			}
		} catch (e) {
			toasts.error(e);
			step = 'off';
		}
	}
	load();

	async function turnOn() {
		busy = true;
		try {
			const permission = await Notification.requestPermission();
			if (permission !== 'granted') {
				step = permission === 'denied' ? 'blocked' : 'off';
				return;
			}
			const reg = await worker();
			if (!reg) {
				step = 'unsupported';
				return;
			}
			const { public_key } = await api.pushStatus();
			let sub = await reg.pushManager.getSubscription();
			// A subscription made for another server key cannot be reused.
			if (sub) await sub.unsubscribe();
			sub = await reg.pushManager.subscribe({
				userVisibleOnly: true,
				applicationServerKey: keyBytes(public_key)
			});
			await api.pushSubscribe(sub.toJSON());
			devices = (await api.pushStatus()).devices;
			step = 'on';
			toasts.push('Notifications are on');
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function turnOff() {
		busy = true;
		try {
			const reg = await worker();
			const sub = await reg?.pushManager.getSubscription();
			if (sub) {
				await api.pushUnsubscribe(sub.endpoint);
				await sub.unsubscribe();
			}
			devices = (await api.pushStatus()).devices;
			step = 'off';
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function test() {
		busy = true;
		try {
			const result = await api.pushTest();
			toasts.push(result.sent === 1 ? 'Sent to 1 device' : `Sent to ${result.sent} devices`);
		} catch (e) {
			toasts.error(e);
			// The server may have just forgotten this device; show the truth.
			await load();
		} finally {
			busy = false;
		}
	}

	const hint = $derived(
		{
			loading: '',
			unsupported: 'This browser cannot show notifications from Frog.',
			insecure: 'Available when Frog is opened over HTTPS.',
			blocked: 'Blocked in this browser. Allow notifications for Frog in its site settings.',
			off: 'New episodes of the shows you follow, on the day they air.',
			on: devices > 1 ? `On for this device and ${devices - 1} more` : 'On for this device'
		}[step]
	);
</script>

<div class="notifications card" data-step={step}>
	<div class="top">
		<span class="label">
			<strong>New episodes</strong>
			<span class="faint">{hint}</span>
		</span>
		{#if step === 'off'}
			<button class="btn primary" disabled={busy} onclick={turnOn}>
				<Icon name="bell" size={16} /> Turn on
			</button>
		{:else if step === 'on'}
			<div class="actions">
				<button class="btn" disabled={busy} onclick={test}>Send a test</button>
				<button class="btn ghost" disabled={busy} onclick={turnOff}>
					<Icon name="bell-off" size={16} /> Turn off
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.notifications {
		padding: 15px 18px;
	}

	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px 16px;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.82rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
</style>
