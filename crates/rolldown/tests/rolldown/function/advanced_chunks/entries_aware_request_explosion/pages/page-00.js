import { c01 } from '../components/c-01.js';
import { c04 } from '../components/c-04.js';
import { c09 } from '../components/c-09.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-01.js'), import('../widgets/w-04.js')]);
  return [c01(), c04(), c09(), ...widgets.map((widget) => widget.default())].join(' ');
}
