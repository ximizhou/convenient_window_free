<script>
  import { onMount } from 'svelte';
  import { zh, en } from './copy.js';
  const cornerKeys = ['top-left', 'top-right', 'bottom-left', 'bottom-right'];
  const names = {
    'top-left': 'tl',
    'top-right': 'tr',
    'bottom-left': 'bl',
    'bottom-right': 'br',
  };
  const actionNames = { none: 'none', 'show-desktop': 'show' };
  const allEdges = ['left', 'right', 'top', 'bottom'];
  let lang = 'zh',
    tab = 'overview',
    selected = 'top-left',
    state = null,
    draft = null,
    busy = false,
    error = '',
    alive = true,
    follow = true;
  $: text = lang === 'zh' ? zh : en;
  $: dirty = !!draft && JSON.stringify(draft) !== JSON.stringify(state?.prefs);
  function adopt(v) {
    state = v;
    draft = structuredClone(v.prefs);
  }
  async function call(body, signal) {
    const response = await fetch('convenient-window/control', {
      method: body ? 'POST' : 'GET',
      headers: body
        ? { 'content-type': 'application/json', 'x-convenient-window': 'panel' }
        : {},
      cache: 'no-store',
      credentials: 'same-origin',
      body: body ? JSON.stringify(body) : undefined,
      signal,
    });
    const data = await response.json();
    if (!response.ok) throw Error(data.error || 'generic');
    return data;
  }
  let active,
    refreshId = 0;
  async function run(action) {
    if (busy) return;
    refreshId++;
    busy = true;
    error = '';
    const ctl = new AbortController();
    active = ctl;
    try {
      const next = await call(
        action
          ? {
              action,
              rev: state.rev,
              session: state.session,
              ...(action === 'save' ? { prefs: draft } : {}),
            }
          : null,
        ctl.signal,
      );
      if (alive) adopt(next);
    } catch (e) {
      if (alive) {
        error = e.message;
        try {
          const latest = await call(null, ctl.signal);
          if (alive && e.message !== 'stale') {
            state = latest;
            if (!draft || action === 'start' || action === 'stop')
              draft = structuredClone(latest.prefs);
          }
        } catch {}
      }
    } finally {
      if (alive) {
        busy = false;
        active = null;
      }
    }
  }
  onMount(() => {
    alive = true;
    function sync() {
      if (follow)
        lang = (document.documentElement.lang || navigator.language || 'en')
          .toLowerCase()
          .startsWith('zh')
          ? 'zh'
          : 'en';
    }
    sync();
    const obs = new MutationObserver(sync);
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['lang'],
    });
    run(null);
    const timer = setInterval(refreshStatus, 4000);
    return () => {
      alive = false;
      clearInterval(timer);
      obs.disconnect();
      active?.abort();
    };
  });
  async function refreshStatus() {
    if (!alive || busy || !state || document.visibilityState !== 'visible')
      return;
    const request = ++refreshId;
    try {
      const next = await call(null);
      if (!alive || busy || request !== refreshId) return;
      if (dirty && (state.rev !== next.rev || state.session !== next.session)) {
        error = 'stale';
        return;
      }
      if (dirty) state = next;
      else adopt(next);
    } catch {
      if (alive && !busy && request === refreshId) error = 'host-failed';
    }
  }
  function chooseEdge(key) {
    if (draft.edges.includes(key)) {
      if (draft.edges.length === 1) return;
      draft.edges = draft.edges.filter((x) => x !== key);
    } else
      draft.edges = allEdges.filter(
        (x) => draft.edges.includes(x) || x === key,
      );
  }
</script>

<section class="cw" {lang} data-cw-panel>
  <header>
    <div class="wordmark">
      <svg viewBox="0 0 26 26" aria-hidden="true"
        ><rect x="3" y="4" width="20" height="18" rx="3" /><path
          d="M3 10h20m-8 0v12"
        /></svg
      ><b>{text.name}</b>
    </div>
    <button
      class="lang"
      onclick={() => {
        follow = false;
        lang = lang === 'zh' ? 'en' : 'zh';
      }}>{text.lang}</button
    >
  </header>
  <main>
    <div class="mast">
      <div>
        <div class="eyebrow">{text.local}</div>
        <h1>{text[tab]}</h1>
        <p>
          {tab === 'overview'
            ? text.overviewHint
            : tab === 'corners'
              ? text.cornerHint
              : text.edgeHint}
        </p>
      </div>
    </div>
    <nav aria-label={text.name}>
      {#each ['overview', 'corners', 'edges'] as key}<button
          aria-pressed={tab === key}
          onclick={() => (tab = key)}>{text[key]}</button
        >{/each}
    </nav>
    {#if error || state?.blocked || state?.fault}<div class="err" role="alert">
        <strong>{text.failed}</strong> · {text.error[
          error || state.blocked || state.fault
        ] || text.error.generic}
        <button onclick={() => run(null)} disabled={busy}>{text.refresh}</button
        >
      </div>{/if}
    {#if draft && state}
      {#if tab === 'overview'}
        <article class="hero">
          <div>
            <div class="eyebrow">{text.master}</div>
            <h2>
              <i class:ok={state.running}></i>{state.running
                ? text.on
                : text.off}
            </h2>
            <p>{text.detail}</p>
          </div>
          <button
            class="primary"
            data-cw-power
            disabled={busy ||
              (!state.running &&
                (dirty || !!state.blocked || error === 'stale'))}
            onclick={() => run(state.running ? 'stop' : 'start')}
            >{busy
              ? text.working
              : state.running
                ? text.stop
                : text.start}</button
          >
        </article>
        <div class="rows">
          <button class="jump" onclick={() => (tab = 'edges')}
            ><span
              ><b>{text.edgeToggle}</b><small
                >{draft.edgeHide ? text.on : text.off}</small
              ></span
            ><span>{text.configure} →</span></button
          ><button class="jump" onclick={() => (tab = 'corners')}
            ><span
              ><b>{text.corners}</b><small
                >{text.activeAngles} · {Object.values(draft.corners).filter(
                  (x) => x !== 'none',
                ).length} / 4</small
              ></span
            ><span>{text.configure} →</span></button
          >
        </div>
        <p class="note">{text.help} {text.session}</p>
      {:else if tab === 'corners'}
        <div class="grid">
          <div class="screen" aria-label={text.display}>
            <span class="screen-title">{text.display}</span
            >{#each cornerKeys as key}<button
                class="corner {names[key]}"
                aria-label={text[names[key]]}
                aria-pressed={selected === key}
                onclick={() => (selected = key)}>{text[names[key]]}</button
              >{/each}
          </div>
          <div class="editor">
            <h2>{text[names[selected]]}</h2>
            <label
              >{text.cornerActions}<select
                bind:value={draft.corners[selected]}
                disabled={busy || !!state.blocked}
                >{#each Object.keys(actionNames) as k}<option value={k}
                    >{text[actionNames[k]]}</option
                  >{/each}</select
              ></label
            ><label
              >{text.cornerDelay} · {draft.hoverDelayMs}
              {text.ms}<input
                aria-label={text.cornerDelay}
                type="range"
                min="100"
                max="2000"
                step="50"
                bind:value={draft.hoverDelayMs}
                disabled={busy || !!state.blocked}
              /></label
            >
          </div>
        </div>
      {:else}
        <div class="rows">
          <label class="row"
            ><span><b>{text.edgeToggle}</b></span><input
              type="checkbox"
              bind:checked={draft.edgeHide}
              disabled={busy || !!state.blocked}
            /></label
          >
          <div class="edgegroup">
            <small>{text.allowed}</small>
            <div class="pills">
              {#each allEdges as key}<button
                  aria-pressed={draft.edges.includes(key)}
                  disabled={busy ||
                    !!state.blocked ||
                    (draft.edges.includes(key) && draft.edges.length === 1)}
                  onclick={() => chooseEdge(key)}>{text[key]}</button
                >{/each}
            </div>
          </div>
        </div>
      {/if}
      {#if tab !== 'overview' || dirty}<div class="save">
          <small role="status"
            >{state.blocked
              ? text.failed
              : dirty
                ? text.dirty
                : text.saved}</small
          ><button
            class="primary"
            disabled={!dirty || busy || !!state.blocked || error === 'stale'}
            onclick={() => run('save')}
            >{busy ? text.working : text.save}</button
          >
        </div>{/if}
      <footer>
        {text.version}
        {state.product} <span>·</span>
        {text.adapter}
        {state.adapter} <span>·</span>
        {text.engine}
        {state.helper}
      </footer>
    {:else if !error}<p role="status">{text.load}</p>{/if}
  </main>
</section>

<style>
  .cw {
    --paper: #fbfcfb;
    --ink: #172522;
    --weak: #526661;
    --line: #cdd8d3;
    --tint: #eaf3ec;
    --key: #266346;
    --btn: #fff;
    box-sizing: border-box;
    min-height: 100%;
    background: var(--paper);
    color: var(--ink);
    font:
      15px/1.5 system-ui,
      'Microsoft YaHei',
      sans-serif;
    color-scheme: light;
    container-type: inline-size;
  }
  .cw * {
    box-sizing: border-box;
  }
  .cw button,
  .cw select,
  .cw input {
    font: inherit;
    color: inherit;
  }
  .cw button {
    cursor: pointer;
    transition: background 0.15s;
  }
  .cw button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .cw :focus-visible {
    outline: 3px solid #51a67e;
    outline-offset: 3px;
  }
  .cw header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--line);
    padding: 13px clamp(16px, 4%, 40px);
  }
  .wordmark {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .wordmark svg {
    stroke: var(--key);
    width: 25px;
    height: 25px;
    fill: none;
    stroke-width: 1.8;
  }
  .cw b,
  .cw strong {
    font-weight: 650;
  }
  .cw .lang,
  .err button {
    border: none;
    background: none;
    color: var(--weak);
    padding: 5px;
  }
  .cw main {
    max-width: 860px;
    margin: auto;
    padding: clamp(19px, 5%, 48px);
  }
  .cw h1,
  .cw h2,
  .cw p {
    margin: 0;
  }
  .cw h1 {
    font-size: clamp(26px, 5cqw, 38px);
    letter-spacing: -0.045em;
    line-height: 1.25;
    margin: 6px 0 8px;
  }
  .cw h2 {
    font-size: 22px;
    letter-spacing: -0.025em;
  }
  .cw p,
  .cw small,
  .cw footer {
    color: var(--weak);
  }
  .cw small,
  .cw footer,
  .eyebrow {
    font-size: 13px;
  }
  .eyebrow {
    letter-spacing: 0.075em;
    font-weight: 600;
    color: var(--key);
  }
  .cw nav {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    border-bottom: 1px solid var(--line);
    margin: 22px 0;
  }
  .cw nav button,
  .pills button {
    border: 0;
    background: transparent;
    color: var(--weak);
    padding: 9px 13px;
  }
  .cw nav button[aria-pressed='true'] {
    color: var(--ink);
    box-shadow: inset 0 -3px var(--key);
    font-weight: 650;
  }
  .hero,
  .grid,
  .rows {
    border: 1px solid var(--line);
    border-radius: 13px;
  }
  .hero {
    padding: 22px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    background: var(--tint);
    margin-bottom: 16px;
  }
  .hero h2 {
    margin: 5px 0;
  }
  .hero i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--weak);
    margin-right: 9px;
  }
  .hero i.ok {
    background: var(--key);
  }
  .cw .primary {
    background: var(--key);
    color: var(--btn);
    border: 0;
    border-radius: 8px;
    padding: 9px 15px;
    font-weight: 600;
  }
  .row,
  .jump,
  .edgegroup {
    padding: 15px 18px;
  }
  .row,
  .jump {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .rows > :not(:last-child) {
    border-bottom: 1px solid var(--line);
  }
  .jump {
    width: 100%;
    text-align: start;
    background: none;
    border: 0;
  }
  .jump:hover,
  .cw nav button:hover,
  .pills button:hover {
    background: var(--tint);
  }
  .jump small,
  .edgegroup small {
    display: block;
  }
  .jump > span:last-child {
    font-size: 13px;
    color: var(--key);
  }
  .cw input {
    accent-color: var(--key);
  }
  .row input {
    width: 19px;
    height: 19px;
    flex-shrink: 0;
  }
  .cw .note {
    margin-top: 15px;
    font-size: 13px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
    padding: 20px;
  }
  .screen {
    position: relative;
    aspect-ratio: 16/10;
    border: 2px solid var(--line);
    border-radius: 9px;
    background: var(--tint);
    display: grid;
    place-items: center;
    align-self: center;
  }
  .screen-title {
    font-size: 13px;
    color: var(--weak);
  }
  .corner,
  .pills button {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--paper);
    font-size: 12px;
    padding: 5px 7px;
  }
  .corner {
    position: absolute;
  }
  .corner[aria-pressed='true'],
  .pills button[aria-pressed='true'] {
    background: var(--key);
    color: var(--btn);
    border-color: var(--key);
  }
  .tl,
  .tr {
    top: 7px;
  }
  .bl,
  .br {
    bottom: 7px;
  }
  .tl,
  .bl {
    left: 7px;
  }
  .tr,
  .br {
    right: 7px;
  }
  .editor label {
    font-size: 13px;
    margin-top: 11px;
    display: block;
    color: var(--weak);
  }
  .editor input,
  .editor select {
    display: block;
    width: 100%;
    margin-top: 4px;
  }
  .editor select {
    color: var(--ink);
    background: var(--paper);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px;
  }
  .pills,
  .save {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .pills {
    margin-top: 8px;
  }
  .save {
    justify-content: flex-end;
    margin-top: 12px;
  }
  .cw footer {
    margin-top: 22px;
  }
  .cw footer span {
    margin: 0 4px;
  }
  .err {
    background: #fff2da;
    color: #603b0b;
    padding: 12px;
    margin-bottom: 12px;
    border-radius: 7px;
  }
  .err button {
    text-decoration: underline;
    color: inherit;
  }
  @container (max-width:520px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .screen {
      max-width: 310px;
      width: 100%;
      justify-self: center;
    }
    .cw nav button {
      padding: 8px 10px;
    }
  }
  @media (prefers-color-scheme: dark) {
    .cw {
      --paper: #17211e;
      --ink: #eaf3ee;
      --weak: #bdccc4;
      --line: #3e5148;
      --tint: #24382d;
      --key: #a6d5b2;
      --btn: #152a1c;
      color-scheme: dark;
    }
  }
</style>
