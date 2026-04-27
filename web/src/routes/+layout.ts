// Pure SPA — disable SSR and prerendering globally so adapter-static produces
// a single-page app shell that hydrates entirely in the browser.
export const ssr = false;
export const prerender = false;
export const trailingSlash = 'never';
