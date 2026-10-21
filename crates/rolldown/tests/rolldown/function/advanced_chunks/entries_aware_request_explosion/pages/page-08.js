import { c09 } from '../components/c-09.js';
import { c12 } from '../components/c-12.js';
import { c17 } from '../components/c-17.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-01.js'), import('../widgets/w-08.js')]);
  return [c09(), c12(), c17(), ...widgets.map((widget) => widget.default())].join(' ');
}
