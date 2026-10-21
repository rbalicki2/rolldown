import { c01 } from '../components/c-01.js';
import { c03 } from '../components/c-03.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-03.js'), import('../widgets/w-10.js')]);
  return [c01(), c03(), ...widgets.map((widget) => widget.default())].join(' ');
}
