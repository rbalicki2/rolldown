import { c15 } from '../components/c-15.js';
import { c32 } from '../components/c-32.js';
import { c39 } from '../components/c-39.js';

export default function render() {
  return [c15(), c32(), c39()].join(' ');
}
