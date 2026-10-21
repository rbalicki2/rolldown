import { c17 } from '../components/c-17.js';
import { c27 } from '../components/c-27.js';
import { c39 } from '../components/c-39.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-04.js'), import('../widgets/w-09.js')]);
  return [c17(), c27(), c39(), ...widgets.map((widget) => widget.default())].join(' ');
}
