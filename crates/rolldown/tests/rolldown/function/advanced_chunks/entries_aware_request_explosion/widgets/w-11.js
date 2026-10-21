import { c10 } from '../components/c-10.js';
import { c28 } from '../components/c-28.js';
import { c39 } from '../components/c-39.js';

export default function render() {
  return [c10(), c28(), c39()].join(' ');
}
