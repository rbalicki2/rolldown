import { c06 } from '../components/c-06.js';
import { c11 } from '../components/c-11.js';
import { c16 } from '../components/c-16.js';

export default function render() {
  return [c06(), c11(), c16()].join(' ');
}
