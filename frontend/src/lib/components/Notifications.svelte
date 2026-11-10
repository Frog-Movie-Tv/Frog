<script lang="ts">
	import { api } from '#lib/api';
	import { toasts } from '#lib/toast.svelte';

	type Step = 'loading' | 'unsupported' | 'insecure' | 'blocked' | 'off' | 'on';

	let step = $state<Step>('loading');
	let devices = $state(0);
	let busy = $state(false);

	function keyBytes(base64url: string): Uint8Array<ArrayBuffer> {
		const padded = base64url
			.replace(/-/g, '+')
			.replace(/_/g, '/')
			.padEnd(Math.ceil(base64url.length / 4) * 4, '=');
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
				if (permission === 'denied') toasts.push('Notifications are blocked in this browser');
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
			toasts.push('Notifications are off');
		} catch (e) {
			toasts.error(e);
		} finally {
			busy = false;
		}
	}

	async function toggle(on: boolean) {
		if (on) await turnOn();
		else await turnOff();
	}

	const hint = $derived(
		{
			loading: '',
			unsupported: 'This browser cannot show notifications from Frog.',
			insecure: 'Available when Frog is opened over HTTPS.',
			blocked: 'Blocked in this browser. Allow notifications for Frog in its site settings.',
			off: 'The moment an episode or film you follow comes out',
			on:
				devices > 1
					? `On for this device and ${devices - 1} more`
					: 'The moment an episode or film you follow comes out'
		}[step]
	);
	const available = $derived(step === 'on' || step === 'off');
</script>

<div class="rows list-pane" data-step={step}>
	<label class="row" class:unavailable={!available}>
		<span class="label">
			<strong>Notifications</strong>
			{#if hint}<span class="faint">{hint}</span>{/if}
		</span>
		<input
			type="checkbox"
			class="toggle-switch"
			aria-label="Notifications"
			checked={step === 'on'}
			disabled={!available || busy}
			onchange={(e) => toggle(e.currentTarget.checked)}
		/>
	</label>
</div>

<style>
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 20px;
		padding: 15px 18px;
		cursor: pointer;
	}

	.row.unavailable {
		cursor: default;
	}

	.label {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}

	.label .faint {
		font-size: 0.78rem;
	}
</style>
