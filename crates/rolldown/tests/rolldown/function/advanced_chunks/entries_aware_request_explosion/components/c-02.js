import { u01 } from '../utils/u-01.js';
import { u02 } from '../utils/u-02.js';
import { u07 } from '../utils/u-07.js';

export function c02() {
  return u01('c02') + u02('c02') + u07('c02');
}
