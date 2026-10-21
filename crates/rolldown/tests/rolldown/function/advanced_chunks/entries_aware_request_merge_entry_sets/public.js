import './lib-both.js';

export const pages = [() => import('./page-public-1.js'), () => import('./page-public-2.js')];
