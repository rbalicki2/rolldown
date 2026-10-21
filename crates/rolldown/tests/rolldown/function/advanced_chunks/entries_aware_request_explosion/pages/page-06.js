import { c25 } from '../components/c-25.js';
import { c30 } from '../components/c-30.js';
import { c37 } from '../components/c-37.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-07.js'), import('../widgets/w-10.js')]);
  return [c25(), c30(), c37(), ...widgets.map((widget) => widget.default())].join(' ');
}
