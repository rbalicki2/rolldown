import { c09 } from '../components/c-09.js';
import { c11 } from '../components/c-11.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-07.js'), import('../widgets/w-10.js')]);
  return [c09(), c11(), ...widgets.map((widget) => widget.default())].join(' ');
}
