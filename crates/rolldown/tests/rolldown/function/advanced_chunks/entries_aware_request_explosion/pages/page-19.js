import { c13 } from '../components/c-13.js';
import { c33 } from '../components/c-33.js';
import { c35 } from '../components/c-35.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-03.js'), import('../widgets/w-10.js')]);
  return [c13(), c33(), c35(), ...widgets.map((widget) => widget.default())].join(' ');
}
