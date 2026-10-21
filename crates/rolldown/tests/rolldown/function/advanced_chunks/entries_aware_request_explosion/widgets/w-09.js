import { c02 } from '../components/c-02.js';
import { c25 } from '../components/c-25.js';
import { c28 } from '../components/c-28.js';

export default function render() {
  return [c02(), c25(), c28()].join(' ');
}
