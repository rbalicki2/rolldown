import { c07 } from '../components/c-07.js';
import { c15 } from '../components/c-15.js';
import { c25 } from '../components/c-25.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-04.js'), import('../widgets/w-09.js')]);
  return [c07(), c15(), c25(), ...widgets.map((widget) => widget.default())].join(' ');
}
