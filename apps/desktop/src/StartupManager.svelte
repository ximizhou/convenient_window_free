<script lang="ts">
  import { onMount } from "svelte";
  import { filterStartupItems, readStartupItems, startupErrorKey, startupNameFromPath } from "./startup-manager";
  import type { StartupErrorKey, StartupFilter, StartupItem, StartupRequest, StartupResponse } from "./startup-manager";

  export let english = false;
  export let request: (request: StartupRequest) => Promise<StartupResponse>;
  export let pickExecutable: () => Promise<string | null>;

  const zh = {
    title: "登录时启动", description: "查看随 Windows 登录启动的应用，按需开启或关闭。",
    add: "添加应用", refresh: "刷新", search: "搜索应用或路径", all: "全部", enabled: "已启用", disabled: "已禁用",
    unknown: "状态未知", readOnly: "只读", userRun: "当前用户", machineRun: "所有用户", machineRun32: "所有用户 · 32 位",
    userStartup: "个人启动文件夹", commonStartup: "公共启动文件夹", details: "查看启动命令", command: "启动命令",
    empty: "没有找到常规启动应用", emptyHint: "添加一个应用，即可在下次登录时自动打开。", noMatches: "没有匹配的应用", clear: "清除筛选",
    loading: "正在读取启动项…", updating: "正在更新…", changedNextLogin: "更改在下次登录时生效，不会关闭正在运行的应用。",
    limitedScope: "显示常规注册表与启动文件夹项目。计划任务、服务和商店应用由 Windows 管理。",
    partial: "部分启动项暂时无法读取。可刷新重试。", addTitle: "添加启动应用", name: "应用名称", executable: "所选应用",
    addHint: "确认后设为当前用户的登录启动项。现在不会打开应用。", confirm: "添加并启用", cancel: "取消", saved: "启动项已更新", added: "已添加启动应用",
    unsupported: "此功能目前支持 Windows。", needsHelper: "请先安装或更新后台助手，然后刷新。",
    ordinaryUser: "请以普通权限运行当前应用，再重试。", identityUnavailable: "无法确认当前用户身份，请重新打开应用后重试。", invalidName: "名称无效，请使用简短的应用名称。",
    accessDenied: "无法更改此项目。请检查权限，或使用 Windows 启动应用设置。", changed: "此项目已被其他程序更改，请刷新后重试。",
    invalidPath: "无法添加这个文件，请重新选择有效的应用程序。", duplicate: "这个启动项已经存在，请刷新列表查看。",
    missing: "这个启动项已不存在，请刷新列表。", unavailable: "启动项管理暂不可用，请刷新或检查后台助手。", failed: "操作未完成，请刷新后重试。",
    toggle: "切换启动状态", managed: "此项目由系统管理", unknownHint: "无法确认此项目的启用状态，暂不可更改。"
  };
  const en: typeof zh = {
    title: "Start at sign-in", description: "Choose which apps open when you sign in to Windows.",
    add: "Add app", refresh: "Refresh", search: "Search apps or paths", all: "All", enabled: "Enabled", disabled: "Disabled",
    unknown: "Unknown", readOnly: "Read only", userRun: "Current user", machineRun: "All users", machineRun32: "All users · 32-bit",
    userStartup: "Personal startup folder", commonStartup: "Shared startup folder", details: "Show startup command", command: "Startup command",
    empty: "No regular startup apps found", emptyHint: "Add an app to open it automatically at your next sign-in.", noMatches: "No matching apps", clear: "Clear filters",
    loading: "Reading startup apps…", updating: "Updating…", changedNextLogin: "Changes apply at your next sign-in. Running apps stay open.",
    limitedScope: "Shows regular registry and startup folder entries. Windows manages scheduled tasks, services and Store apps.",
    partial: "Some startup entries could not be read. Refresh to try again.", addTitle: "Add a startup app", name: "App name", executable: "Selected app",
    addHint: "Adds this app for the current user's sign-in. It will not open now.", confirm: "Add and enable", cancel: "Cancel", saved: "Startup entry updated", added: "Startup app added",
    unsupported: "This feature currently supports Windows.", needsHelper: "Install or update the helper, then refresh.",
    ordinaryUser: "Run this app with standard user permissions, then try again.", identityUnavailable: "The current user could not be identified. Reopen the app and try again.", invalidName: "Enter a valid, short app name.",
    accessDenied: "This entry cannot be changed. Check permissions or use Windows Startup settings.", changed: "Another app changed this entry. Refresh before trying again.",
    invalidPath: "This file cannot be added. Select a valid application.", duplicate: "This startup entry already exists. Refresh to see it.",
    missing: "This startup entry no longer exists. Refresh the list.", unavailable: "Startup management is unavailable. Refresh or check the helper.", failed: "The operation did not complete. Refresh and try again.",
    toggle: "Change startup status", managed: "This entry is managed by the system", unknownHint: "Its enabled status could not be confirmed, so it cannot be changed."
  };
  $: text = english ? en : zh;

  let items: StartupItem[] = [];
  let query = "";
  let filter: StartupFilter = "all";
  let loaded = false;
  let pending: "list" | "add" | "setEnabled" | null = null;
  let pendingId = "";
  let picking = false;
  let error: StartupErrorKey | null = null;
  let notice: "saved" | "added" | null = null;
  let partial = false;
  let draft: { executablePath: string; name: string } | null = null;
  let alive = false;
  let generation = 0;
  $: visibleItems = filterStartupItems(items, query, filter);
  $: busy = pending !== null || picking;
  $: blocked = error === "unsupported" || error === "needsHelper";

  async function perform(input: StartupRequest): Promise<void> {
    if (!alive || pending || picking) return;
    const current = ++generation;
    pending = input.action;
    pendingId = input.action === "setEnabled" ? input.id : "";
    error = null;
    notice = null;
    try {
      const response = await request(input);
      if (!alive || current !== generation) return;
      const next = readStartupItems(response);
      if (!next) { error = startupErrorKey(response?.error?.code); return; }
      items = next;
      loaded = true;
      partial = Boolean(response.warnings?.length);
      if (input.action === "add") { draft = null; notice = "added"; }
      if (input.action === "setEnabled") notice = "saved";
    } catch {
      if (alive && current === generation) error = "failed";
    } finally {
      if (alive && current === generation) { pending = null; pendingId = ""; }
    }
  }

  function toggle(item: StartupItem): void {
    if (!item.editable || item.enabled === null || busy || blocked) return;
    void perform({ action: "setEnabled", id: item.id, revision: item.revision, enabled: !item.enabled });
  }

  async function chooseApplication(): Promise<void> {
    if (busy || blocked || !alive) return;
    picking = true;
    error = null;
    notice = null;
    const current = ++generation;
    try {
      const filename = await pickExecutable();
      if (alive && current === generation && filename) draft = { executablePath: filename, name: startupNameFromPath(filename) };
    } catch {
      if (alive && current === generation) error = "invalidPath";
    } finally {
      if (alive && current === generation) picking = false;
    }
  }

  function addApplication(): void {
    if (!draft || !draft.name.trim() || busy) return;
    void perform({ action: "add", executablePath: draft.executablePath, name: draft.name.trim() });
  }

  onMount(() => {
    alive = true;
    void perform({ action: "list" });
    const refresh = () => { if (document.visibilityState !== "hidden" && !draft) void perform({ action: "list" }); };
    window.addEventListener("focus", refresh);
    return () => { alive = false; generation++; window.removeEventListener("focus", refresh); };
  });
</script>

<section class="startup-manager" aria-label={text.title}>
  <div class="startup-heading"><div><h2>{text.title}</h2><p>{text.description}</p></div><button class="startup-add" disabled={busy || blocked} on:click={chooseApplication} type="button"><span aria-hidden="true">＋</span>{text.add}</button></div>
  <div class="startup-search"><svg aria-hidden="true" viewBox="0 0 20 20"><circle cx="8" cy="8" r="5.5" /><path d="m12 12 5 5" /></svg><input aria-label={text.search} placeholder={text.search} bind:value={query} type="search" /><button class="startup-refresh" disabled={busy} on:click={() => perform({ action: "list" })} type="button" title={text.refresh} aria-label={text.refresh}><svg aria-hidden="true" viewBox="0 0 20 20"><path d="M16.5 8a6.5 6.5 0 1 0-.8 5M16.5 3v5h-5" /></svg></button></div>
  <div class="startup-filters" aria-label={text.title}>{#each ["all", "enabled", "disabled"] as value}<button aria-pressed={filter === value} class:active={filter === value} on:click={() => { filter = value as StartupFilter; }} type="button">{text[value as StartupFilter]}</button>{/each}</div>

  {#if draft}
    <form class="startup-draft" on:submit|preventDefault={addApplication}>
      <h3>{text.addTitle}</h3><label>{text.name}<input aria-label={text.name} bind:value={draft.name} maxlength="80" disabled={pending === "add"} /></label>
      <div class="startup-selected"><span>{text.executable}</span><p>{draft.executablePath}</p></div><p class="startup-hint">{text.addHint}</p>
      <div class="startup-draft-actions"><button disabled={pending === "add"} on:click={() => { draft = null; }} type="button">{text.cancel}</button><button class="startup-add" disabled={busy || !draft.name.trim()} type="submit">{text.confirm}</button></div>
    </form>
  {/if}
  {#if error}<p class="startup-message error" role="alert">{text[error]}</p>{/if}
  {#if notice}<p class="startup-message" role="status">{text[notice]}</p>{/if}
  {#if partial}<p class="startup-message warning" role="status">{text.partial}</p>{/if}
  {#if pending === "list" && !loaded}<p class="startup-empty" role="status">{text.loading}</p>{/if}
  {#if loaded}
    <ul class="startup-list" aria-busy={pending !== null}>
      {#each visibleItems as item (item.id)}
        <li class="startup-item" data-startup-id={item.id}>
          <div class="startup-row"><span class="startup-icon" aria-hidden="true">{item.name.slice(0, 1).toLocaleUpperCase() || "□"}</span>
            <div class="startup-item-title"><strong title={item.name}>{item.name}</strong><span>{text[item.source]}</span></div>
            <div class="startup-state"><span class:on={item.enabled === true}>{pendingId === item.id ? text.updating : item.enabled === null ? text.unknown : item.enabled ? text.enabled : text.disabled}</span>
              <button class="startup-switch" class:on={item.enabled === true} role="switch" aria-checked={item.enabled === true} aria-label={text.toggle + ": " + item.name} disabled={busy || blocked || !item.editable || item.enabled === null} on:click={() => toggle(item)} title={item.enabled === null ? text.unknownHint : item.editable ? text.toggle : text.managed} type="button"><i></i></button>
            </div>
          </div>
          <details><summary>{text.details}{#if !item.editable}<span class="startup-readonly">{text.readOnly}</span>{/if}</summary><div class="startup-command"><span>{text.command}</span><p>{item.command}</p>{#if item.enabled === null}<small>{text.unknownHint}</small>{:else if !item.editable}<small>{text.managed}</small>{/if}</div></details>
        </li>
      {:else}
        <li class="startup-empty"><strong>{items.length ? text.noMatches : text.empty}</strong>{#if items.length}<button on:click={() => { query = ""; filter = "all"; }} type="button">{text.clear}</button>{:else}<p>{text.emptyHint}</p>{/if}</li>
      {/each}
    </ul>
  {/if}
  <p class="startup-hint">{text.changedNextLogin}</p><p class="startup-scope">{text.limitedScope}</p>
</section>

<style>
  .startup-manager { display: grid; gap: 14px; min-width: 0; color: var(--ink); }
  .startup-heading { display: flex; align-items: flex-start; gap: 12px; }
  .startup-heading > div { flex: 1; min-width: 0; }
  .startup-heading h2 { margin: 0 0 6px; font-size: var(--fs-lg); font-weight: 650; }
  .startup-heading p, .startup-hint, .startup-scope { margin: 0; color: var(--muted); font-size: var(--fs-sm); line-height: 1.6; }
  .startup-manager button { cursor: pointer; }
  .startup-manager button:disabled { cursor: default; opacity: .48; }
  .startup-manager button:focus-visible, .startup-manager input:focus-visible, .startup-manager summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .startup-add { display: inline-flex; align-items: center; justify-content: center; gap: 4px; flex-shrink: 0; padding: 8px 10px; background: var(--accent-bg); color: var(--accent-soft); border: 1px solid var(--accent-line); border-radius: var(--radius-sm); font-size: var(--fs-sm); }
  .startup-search { display: flex; align-items: center; gap: 8px; padding-left: 10px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--radius-sm); }
  .startup-search > svg, .startup-refresh svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.5; flex-shrink: 0; color: var(--muted); }
  .startup-search input { width: 100%; min-width: 0; border: 0; border-radius: var(--radius-xs); padding: 9px 0; background: transparent; font-size: var(--fs-sm); }
  .startup-refresh { display: grid; place-items: center; background: transparent; border: 0; min-width: 36px; min-height: 36px; padding: 8px; }
  .startup-filters { display: flex; gap: 4px; padding-bottom: 8px; border-bottom: 1px solid var(--line); }
  .startup-filters button { border: 1px solid transparent; border-radius: var(--radius-xs); background: transparent; color: var(--muted); padding: 5px 10px; font-size: var(--fs-sm); }
  .startup-filters button.active { color: var(--ink); background: var(--raised); border-color: var(--line-strong); }
  .startup-list { padding: 0; margin: 0; list-style: none; }
  .startup-item { border-bottom: 1px solid var(--line); padding: 12px 0; }
  .startup-item:first-child { padding-top: 0; }
  .startup-row { display: flex; align-items: center; gap: 9px; min-width: 0; }
  .startup-icon { display: grid; place-items: center; width: 32px; height: 34px; flex-shrink: 0; background: var(--raised); border: 1px solid var(--line); border-radius: var(--radius-sm); font-size: var(--fs-lg); color: var(--muted); }
  .startup-item-title { display: grid; gap: 3px; min-width: 0; flex: 1; }
  .startup-item-title strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 550; font-size: var(--fs-md); }
  .startup-item-title > span { color: var(--muted); font-size: var(--fs-xs); }
  .startup-state { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .startup-state > span { font-size: var(--fs-xs); color: var(--muted); }
  .startup-state > span.on { color: var(--green); }
  .startup-switch { width: 32px; height: 19px; padding: 2px; border: 1px solid var(--line-strong); border-radius: var(--radius-pill); background: var(--track); }
  .startup-switch i { display: block; width: 13px; height: 13px; border-radius: 50%; background: var(--muted); }
  .startup-switch.on { background: var(--accent); border-color: var(--accent); }
  .startup-switch.on i { margin-left: 13px; background: var(--button-text); }
  .startup-item details { margin: 7px 0 0 41px; min-width: 0; }
  .startup-item summary { color: var(--muted); font-size: var(--fs-xs); cursor: pointer; width: fit-content; }
  .startup-readonly { margin-left: 8px; color: var(--faint-text); }
  .startup-command { margin-top: 8px; background: var(--bg); border-radius: var(--radius-xs); padding: 9px; }
  .startup-command span, .startup-selected span { color: var(--muted); font-size: var(--fs-xs); }
  .startup-command p, .startup-selected p { margin: 5px 0 0; overflow-wrap: anywhere; white-space: pre-wrap; font-size: var(--fs-sm); line-height: 1.5; user-select: text; }
  .startup-command small { display: block; margin-top: 6px; color: var(--muted); font-size: var(--fs-xs); line-height: 1.5; }
  .startup-message { margin: 0; padding: 9px 11px; color: var(--green); background: var(--green-bg); border-radius: var(--radius-sm); font-size: var(--fs-sm); line-height: 1.5; }
  .startup-message.error { color: var(--danger); background: var(--danger-bg); }
  .startup-message.warning { color: var(--amber); background: var(--amber-bg); }
  .startup-empty { display: grid; gap: 9px; padding: 22px 10px; text-align: center; color: var(--muted); font-size: var(--fs-sm); line-height: 1.6; }
  .startup-empty strong { color: var(--ink); font-weight: 550; }
  .startup-empty p { margin: 0; }
  .startup-empty button { justify-self: center; border: 0; background: transparent; color: var(--accent-soft); font-size: var(--fs-sm); }
  .startup-scope { font-size: var(--fs-xs); color: var(--faint-text); }
  .startup-draft { display: grid; gap: 11px; padding: 14px; border: 1px solid var(--accent-line); border-radius: var(--radius-sm); background: var(--panel); }
  .startup-draft h3 { margin: 0; font-size: var(--fs-md); }
  .startup-draft label { display: grid; gap: 5px; font-size: var(--fs-sm); }
  .startup-draft input { width: 100%; min-width: 0; padding: 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--radius-xs); }
  .startup-draft-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .startup-draft-actions > button:not(.startup-add) { border: 1px solid var(--line-strong); border-radius: var(--radius-sm); background: transparent; padding: 7px 10px; font-size: var(--fs-sm); }
  @media (max-width: 520px) { .startup-state { flex-direction: column-reverse; gap: 3px; } .startup-heading { flex-wrap: wrap; } }
</style>
