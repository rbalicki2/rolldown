import { c02 } from '../components/c-02.js';
import { c17 } from '../components/c-17.js';
import { c29 } from '../components/c-29.js';

export default async function render() {
  const widgets = await Promise.all([import('../widgets/w-07.js'), import('../widgets/w-10.js')]);
  return [c02(), c17(), c29(), ...widgets.map((widget) => widget.default())].join(' ');
}
