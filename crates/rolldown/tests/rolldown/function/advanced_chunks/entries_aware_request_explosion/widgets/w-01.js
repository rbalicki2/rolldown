import { c09 } from '../components/c-09.js';
import { c18 } from '../components/c-18.js';
import { c20 } from '../components/c-20.js';

export default function render() {
  return [c09(), c18(), c20()].join(' ');
}
