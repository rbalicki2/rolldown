import { u00 } from './utils/u-00.js';

export function start(routes, path) {
  return routes[path]()
    .then((page) => page.default())
    .then(u00);
}
