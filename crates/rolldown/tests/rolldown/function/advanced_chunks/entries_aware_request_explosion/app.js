import { start } from './router.js';
import { routes } from './app-routes.js';

export const ready = start(routes, 'page-00');
