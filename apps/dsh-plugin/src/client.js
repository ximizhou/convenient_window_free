import React from 'react';
import { mount, unmount } from 'svelte';
import Panel from './Panel.svelte';
export const name = 'convenient-window';
export const inject = ['slots', 'locale'];
const ID = 'convenient-window';
function Shell() {
  const ref = React.useRef(null);
  React.useEffect(() => {
    const instance = mount(Panel, { target: ref.current });
    return () => {
      void unmount(instance);
    };
  }, []);
  return React.createElement('div', {
    ref,
    style: { height: '100%', minHeight: 0, width: '100%', overflow: 'auto' },
  });
}
function Mark({ size = 20 }) {
  return React.createElement(
    'svg',
    {
      width: size,
      height: size,
      viewBox: '0 0 24 24',
      fill: 'none',
      stroke: 'currentColor',
      strokeWidth: 1.65,
      'aria-hidden': true,
    },
    React.createElement('rect', { x: 3, y: 4, width: 18, height: 16, rx: 3 }),
    React.createElement('path', { d: 'M3 9h18m-6 0v11' }),
  );
}
export function apply(ctx) {
  const ns = 'convenient-window-panel';
  ctx.effect(() =>
    ctx.locale.register(ns, {
      en: { title: 'Convenient Window' },
      zh: { title: '便捷窗口' },
    }),
  );
  const t = ctx.locale.bind(ns);
  ctx.slots.inject('main', () =>
    ctx.slots.register({ name: 'main', key: ID, locale: ns }, Shell),
  );
  ctx.slots.inject('sidebar.panellist', () =>
    ctx.slots.register(
      {
        name: 'sidebar.panellist',
        id: ID,
        order: 20,
        locale: ns,
        label: () => t('title'),
      },
      Mark,
    ),
  );
}
