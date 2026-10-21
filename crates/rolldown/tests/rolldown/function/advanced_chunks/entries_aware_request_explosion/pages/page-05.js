import { c09 } from '../components/c-09.js';
import { c19 } from '../components/c-19.js';
import { c31 } from '../components/c-31.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-04.js'), import('../widgets/w-05.js')]);
  return [c09(), c19(), c31(), ...widgets.map((widget) => widget.default())].join(' ');
}
