import { c17 } from '../components/c-17.js';
import { c19 } from '../components/c-19.js';
import { c37 } from '../components/c-37.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-07.js'), import('../widgets/w-10.js')]);
  return [c17(), c19(), c37(), ...widgets.map((widget) => widget.default())].join(' ');
}
