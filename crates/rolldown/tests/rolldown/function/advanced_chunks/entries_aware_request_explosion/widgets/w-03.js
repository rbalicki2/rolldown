import { c02 } from '../components/c-02.js';
import { c04 } from '../components/c-04.js';
import { c23 } from '../components/c-23.js';

export default function render() {
  return [c02(), c04(), c23()].join(' ');
}
