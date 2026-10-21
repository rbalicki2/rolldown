import './lib-both.js';

export const pages = [() => import('./page-app-1.js'), () => import('./page-app-2.js')];
