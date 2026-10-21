import { c01 } from '../components/c-01.js';
import { c16 } from '../components/c-16.js';
import { c33 } from '../components/c-33.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-01.js'), import('../widgets/w-04.js')]);
  return [c01(), c16(), c33(), ...widgets.map((widget) => widget.default())].join(' ');
}
