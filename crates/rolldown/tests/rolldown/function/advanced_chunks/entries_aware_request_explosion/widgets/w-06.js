import { c03 } from '../components/c-03.js';
import { c04 } from '../components/c-04.js';
import { c35 } from '../components/c-35.js';

export default function render() {
  return [c03(), c04(), c35()].join(' ');
}
