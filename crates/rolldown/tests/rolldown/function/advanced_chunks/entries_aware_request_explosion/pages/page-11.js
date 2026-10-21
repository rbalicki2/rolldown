import { c05 } from '../components/c-05.js';
import { c25 } from '../components/c-25.js';
import { c27 } from '../components/c-27.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-10.js'), import('../widgets/w-11.js')]);
  return [c05(), c25(), c27(), ...widgets.map((widget) => widget.default())].join(' ');
}
