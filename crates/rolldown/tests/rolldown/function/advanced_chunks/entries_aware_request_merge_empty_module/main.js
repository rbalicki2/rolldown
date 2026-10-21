import { shared } from './barrel.js';

export const page = () => import('./page.js');
console.log(shared);
