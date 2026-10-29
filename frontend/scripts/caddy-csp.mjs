/**
 * The Content-Security-Policy the deployed site is served with.
 *
 * Caddy sends it (deploy/caddy/Caddyfile), so nothing local ever sees it: not
 * a dev build, not the audit, not Playwright. It has broken the live site
 * three times. The audit and the e2e suite read it from the Caddyfile and add
 * it to what they load, so a page the policy would break fails here first.
 * Reading it rather than copying it keeps the Caddyfile the only source.
 */
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const CADDYFILE = join(repo, 'deploy', 'caddy', 'Caddyfile');

/**
 * The policy, exactly as Caddy sends it. Throws when the Caddyfile does not
 * hold exactly one, rather than quietly auditing without it.
 * @returns {string}
 */
export function deployedCsp() {
	const text = readFileSync(CADDYFILE, 'utf8');
	const found = [...text.matchAll(/^\s*Content-Security-Policy\s+"([^"]+)"\s*$/gm)];
	if (found.length !== 1) {
		throw new Error(
			`Expected one Content-Security-Policy header in ${CADDYFILE}, found ${found.length}. ` +
				'The audit checks pages against it and will not run without it.'
		);
	}
	return found[0][1];
}

/**
 * Serve every same-origin response in this browser context with the deployed
 * policy, and collect what it blocks. Returns the list, filled as it happens.
 * @param {import('@playwright/test').BrowserContext} context
 * @param {string} origin
 * @returns {Promise<string[]>}
 */
export async function enforceDeployedCsp(context, origin) {
	const policy = deployedCsp();
	/** @type {string[]} */
	const violations = [];
	await context.route('**/*', async (route) => {
		const request = route.request();
		// A policy governs the document or worker it arrives with; on an image
		// or a fetch it does nothing, so those pass straight through.
		const governs =
			request.resourceType() === 'document' || request.url().endsWith('/service-worker.js');
		if (!governs || !request.url().startsWith(origin)) return route.continue();
		try {
			const response = await route.fetch();
			await route.fulfill({
				response,
				headers: { ...response.headers(), 'content-security-policy': policy }
			});
		} catch {
			// The page moved on before the response arrived.
			await route.abort().catch(() => {});
		}
	});
	await context.exposeBinding('__frogCspViolation', (_source, text) => violations.push(text));
	await context.addInitScript(() => {
		document.addEventListener('securitypolicyviolation', (e) => {
			const blocked = e.blockedURI || 'inline';
			// @ts-expect-error: added by exposeBinding above
			window.__frogCspViolation(
				`${e.effectiveDirective} blocked ${blocked} on ${location.pathname}`
			);
		});
	});
	return violations;
}
