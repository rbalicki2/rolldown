import { c02 } from '../components/c-02.js';
import { c05 } from '../components/c-05.js';
import { c09 } from '../components/c-09.js';

export default function render() {
  return [c02(), c05(), c09()].join(' ');
}
