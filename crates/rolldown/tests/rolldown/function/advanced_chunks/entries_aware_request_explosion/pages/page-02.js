import { c01 } from '../components/c-01.js';
import { c13 } from '../components/c-13.js';
import { c26 } from '../components/c-26.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-02.js'), import('../widgets/w-07.js')]);
  return [c01(), c13(), c26(), ...widgets.map((widget) => widget.default())].join(' ');
}
