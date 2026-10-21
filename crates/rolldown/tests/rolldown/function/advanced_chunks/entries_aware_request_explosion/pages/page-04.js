import { c08 } from '../components/c-08.js';
import { c25 } from '../components/c-25.js';
import { c33 } from '../components/c-33.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-00.js'), import('../widgets/w-01.js')]);
  return [c08(), c25(), c33(), ...widgets.map((widget) => widget.default())].join(' ');
}
