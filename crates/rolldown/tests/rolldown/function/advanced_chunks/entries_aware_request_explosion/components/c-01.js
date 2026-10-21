import { u01 } from '../utils/u-01.js';
import { u04 } from '../utils/u-04.js';
import { u12 } from '../utils/u-12.js';

export function c01() {
  return u01('c01') + u04('c01') + u12('c01');
}
