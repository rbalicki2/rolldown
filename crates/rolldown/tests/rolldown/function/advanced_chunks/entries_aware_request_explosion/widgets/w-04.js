import { c13 } from '../components/c-13.js';
import { c17 } from '../components/c-17.js';
import { c30 } from '../components/c-30.js';

export default function render() {
  return [c13(), c17(), c30()].join(' ');
}
