import { c15 } from '../components/c-15.js';
import { c23 } from '../components/c-23.js';
import { c33 } from '../components/c-33.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-01.js'), import('../widgets/w-04.js')]);
  return [c15(), c23(), c33(), ...widgets.map((widget) => widget.default())].join(' ');
}
