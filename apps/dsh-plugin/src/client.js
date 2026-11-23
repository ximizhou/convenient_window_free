import React from 'react';
import { name } from '../package.json';
export { name };
export const inject = ['slots', 'locale'];
const ID = 'convenient-window';
function Shell({ language }) {
  const url = new URL('convenient-window/panel.html', document.baseURI);
  url.searchParams.set('lang', language);
  return React.createElement('iframe', {
    title: name,
    'data-cw-panel': 'desktop',
    src: url.href,
    style: {
      display: 'block',
      width: '100%',
      height: '100%',
      minHeight: 0,
      border: 0,
    },
    allow: 'clipboard-write',
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
      en: { language: 'en-US' },
      zh: { language: 'zh-CN' },
    }),
  );
  const t = ctx.locale.bind(ns);
  ctx.slots.inject('main', () =>
    ctx.slots.register({ name: 'main', key: ID, locale: ns }, () =>
      React.createElement(Shell, { language: t('language') }),
    ),
  );
  ctx.slots.inject('sidebar.panellist', () =>
    ctx.slots.register(
      {
        name: 'sidebar.panellist',
        id: ID,
        order: 20,
        locale: ns,
        label: () => name,
      },
      Mark,
    ),
  );
}
