// Lint rules for the web app. Formatting is Prettier's job, types are
// svelte-check's; this catches the mistakes neither of them sees.
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

export default ts.config(
	js.configs.recommended,
	...ts.configs.recommended,
	...svelte.configs.recommended,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node }
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts'],
		languageOptions: {
			parserOptions: {
				parser: ts.parser,
				extraFileExtensions: ['.svelte']
			}
		}
	},
	{
		rules: {
			// resolve() exists for apps served under a base path. Frog always runs
			// at the root of its own origin, so every link is already resolved.
			'svelte/no-navigation-without-resolve': 'off',
			// An argument kept for its position, or a value thrown away on
			// purpose, is named with a leading underscore.
			'@typescript-eslint/no-unused-vars': [
				'error',
				{ argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrors: 'none' }
			]
		}
	},
	{
		ignores: [
			'build/',
			'.svelte-kit/',
			'test-results/',
			'playwright-report/',
			'node_modules/',
			'static/',
			'.*-data/'
		]
	}
);
