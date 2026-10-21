import { c17 } from '../components/c-17.js';
import { c20 } from '../components/c-20.js';
import { c25 } from '../components/c-25.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-00.js'), import('../widgets/w-01.js')]);
  return [c17(), c20(), c25(), ...widgets.map((widget) => widget.default())].join(' ');
}
