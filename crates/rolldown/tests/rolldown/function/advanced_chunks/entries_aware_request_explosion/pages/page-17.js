import { c01 } from '../components/c-01.js';
import { c23 } from '../components/c-23.js';
import { c31 } from '../components/c-31.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-04.js'), import('../widgets/w-05.js')]);
  return [c01(), c23(), c31(), ...widgets.map((widget) => widget.default())].join(' ');
}
