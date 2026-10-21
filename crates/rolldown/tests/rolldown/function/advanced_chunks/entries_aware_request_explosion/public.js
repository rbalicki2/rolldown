import { start } from './router.js';
import { routes } from './public-routes.js';

export const ready = start(routes, 'page-00');
