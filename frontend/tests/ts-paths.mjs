// Lets `node --test` load the app's TypeScript as it is written: Vite resolves
// "./parsers" and "#lib/types" without an extension, Node does not.
export async function resolve(specifier, context, next) {
	try {
		return await next(specifier, context);
	} catch (error) {
		if (!/^(\.{1,2}\/|#lib\/)/.test(specifier) || /\.\w+$/.test(specifier)) throw error;
		return next(`${specifier}.ts`, context);
	}
}
