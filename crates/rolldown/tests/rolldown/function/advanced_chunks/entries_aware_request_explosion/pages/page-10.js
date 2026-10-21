import { c09 } from '../components/c-09.js';
import { c21 } from '../components/c-21.js';
import { c34 } from '../components/c-34.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-06.js'), import('../widgets/w-07.js')]);
  return [c09(), c21(), c34(), ...widgets.map((widget) => widget.default())].join(' ');
}
