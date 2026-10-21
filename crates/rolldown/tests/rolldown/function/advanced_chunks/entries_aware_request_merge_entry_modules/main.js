import { shared } from './shared.js';
import { mainOnly } from './main-only.js';

export const page = () => import('./page.js');
console.log(shared, mainOnly);
