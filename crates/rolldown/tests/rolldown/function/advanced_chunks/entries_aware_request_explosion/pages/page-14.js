import { c05 } from '../components/c-05.js';
import { c33 } from '../components/c-33.js';
import { c38 } from '../components/c-38.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-02.js'), import('../widgets/w-07.js')]);
  return [c05(), c33(), c38(), ...widgets.map((widget) => widget.default())].join(' ');
}
