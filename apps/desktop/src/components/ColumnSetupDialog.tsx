import * as React from 'react';
import { renderInlineMarkdown } from '../lib/inlineMarkdown';
import { useColumnSetup, type ColumnSetupDialogProps } from '../hooks/useColumnSetup';
import { DiffView } from './DiffView';

export function ColumnSetupDialog(props: ColumnSetupDialogProps) {
  const ref = React.useRef<HTMLDialogElement>(null);
  const model = useColumnSetup(props);
  const { options, selection, preview, phase, error } = model;
  const writing = phase === 'applying';
  const disabled = props.readOnly || writing || !options || options.configured;
  const chosen = selection.mode === 'existing' ? selection.headings : [];

  React.useEffect(() => {
    const dialog = ref.current;
    dialog?.showModal();
    dialog?.querySelector<HTMLElement>('[autofocus]')?.focus();
    const cycleFocus = (event: KeyboardEvent) => {
      if (event.key !== 'Tab' || !dialog?.open) return;
      // WebKit can leave focus on body after a clicked button is replaced. A
      // window capture handler also handles that first Tab, and uses the same
      // order across engines rather than relying on platform tab preferences.
      const items = Array.from(dialog.querySelectorAll<HTMLElement>('button:enabled, input:enabled:not([type="radio"]), input[type="radio"]:enabled:checked, [tabindex="0"]'))
        .filter((item) => item.getClientRects().length > 0);
      const index = items.indexOf(document.activeElement as HTMLElement);
      const next = index < 0 ? (event.shiftKey ? items.at(-1) : items[0])
        : items[(index + (event.shiftKey ? -1 : 1) + items.length) % items.length];
      event.preventDefault();
      event.stopPropagation();
      (next ?? dialog).focus();
    };
    window.addEventListener('keydown', cycleFocus, true);
    return () => {
      window.removeEventListener('keydown', cycleFocus, true);
      dialog?.close();
      if (props.restoreFocus?.isConnected) props.restoreFocus.focus();
    };
  }, [props.restoreFocus]);

  const changeHeading = (name: string, checked: boolean, done?: boolean) => {
    const next = new Map(chosen.map((heading) => [heading.name, heading.done]));
    if (checked) next.set(name, done ?? false); else next.delete(name);
    model.change({ mode: 'existing', headings: (options?.headings ?? [])
      .filter((heading) => heading.selectable && next.has(heading.name))
      .map((heading) => ({ name: heading.name, done: next.get(heading.name)! })) });
  };

  return (
    <dialog
      ref={ref}
      className="column-setup-dialog"
      aria-labelledby="column-setup-title"
      aria-describedby="column-setup-description"
      onCancel={(event) => { event.preventDefault(); model.close(); }}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <div className="column-setup-dialog__body">
        <header className="column-setup-dialog__header">
          <h2 id="column-setup-title">{props.startTask ? '创建列并推进任务' : '配置任务列'}</h2>
          <p id="column-setup-description">{props.startTask
            ? '为此来源配置默认列，并将下面这项未归列任务及其子任务移到「进行中」顶部。保留勾选状态、备注和元数据，不移动其他任务。确认后一次写入；取消不会创建列或移动任务。'
            : '只配置此来源的任务列，不移动或重排现有任务，不改变任务勾选状态。确认前不会写入文件。'}</p>
          {props.startTask && <p className="column-setup-dialog__task">{renderInlineMarkdown(props.startTask.body)}</p>}
          <dl className="column-setup-dialog__source">
            <div><dt>项目</dt><dd><code>{props.projectPath}</code></dd></div>
            <div><dt>来源</dt><dd><code>{props.sourcePath}</code></dd></div>
          </dl>
        </header>

        {phase === 'loading' && <p role="status">正在读取二级标题…</p>}
        {error && <p className="review-error" role="alert">{error}</p>}
        {!options && phase !== 'loading' && <button type="button" className="line-action" onClick={() => void model.load()}>重新读取</button>}
        {props.readOnly && <p role="status">此来源或项目服务为只读，不能配置任务列。</p>}
        {options?.configured && <p role="status">此来源已定义任务列，不能用初始配置覆盖。请关闭并重新读取项目。</p>}

        {options && !props.startTask && (
          <>
            <fieldset className="column-setup-choices" disabled={disabled}>
              <legend>配置方式</legend>
              <label className={selection.mode === 'defaults' ? 'is-selected' : undefined}>
                <input type="radio" name="column-setup-mode" aria-label="创建默认状态列（推荐）" checked={selection.mode === 'defaults'} onChange={() => model.change({ mode: 'defaults' })} />
                <span><strong>创建默认状态列（推荐）</strong><small>待办 / 进行中 / 已完成。缺失的二级标题追加到文件末尾且保持为空；已有唯一同名标题会复用，原有任务留在原处。</small></span>
              </label>
              <label className={selection.mode === 'existing' ? 'is-selected' : undefined}>
                <input type="radio" name="column-setup-mode" aria-label="使用现有标题" checked={selection.mode === 'existing'} onChange={() => model.change({ mode: 'existing', headings: [] })} />
                <span><strong>使用现有标题</strong><small>普通文档标题不等于任务状态。只将明确勾选的二级标题设为列；其中的任务自然归列，但不会移动或改变勾选状态。</small></span>
              </label>
            </fieldset>
            {selection.mode === 'existing' && (
              <fieldset className="column-setup-headings" disabled={disabled}>
                <legend>选择二级标题</legend>
                <p>完成列需明确勾选；不会根据标题名称或任务内容推断。</p>
                {options.headings.length === 0 && <p role="status">此来源没有可用的二级标题，可改用默认状态列。</p>}
                {options.headings.map((heading) => {
                  const selected = chosen.find((item) => item.name === heading.name);
                  return (
                    <div className="column-setup-heading" key={`${heading.line}:${heading.name}`}>
                      <label>
                        <input type="checkbox" aria-label={`使用「${heading.name}」（第 ${heading.line} 行）`} checked={Boolean(selected)} disabled={!heading.selectable} onChange={(event) => changeHeading(heading.name, event.target.checked)} />
                        <span><strong>{heading.name}</strong><small>第 {heading.line} 行{!heading.selectable && ` · ${heading.reason || '无法唯一定位，不能选择。'}`}</small></span>
                      </label>
                      {heading.selectable && <label className="column-setup-heading__done">
                        <input type="checkbox" aria-label={`将「${heading.name}」设为完成列`} checked={selected?.done ?? false} disabled={!selected} onChange={(event) => changeHeading(heading.name, true, event.target.checked)} />
                        完成列
                      </label>}
                    </div>
                  );
                })}
              </fieldset>
            )}
          </>
        )}

        {phase === 'previewing' && <p role="status">正在生成差异预览，不会写入文件…</p>}
        {preview && (
          <section className="column-setup-preview" aria-labelledby="column-setup-preview-title">
            <h3 id="column-setup-preview-title">差异预览</h3>
            <ul aria-label="将配置的列">
              {preview.columns.map((column) => <li key={column.id}><strong>{column.name}</strong><span>{props.startTask && column.id === 'doing'
                ? `${column.created ? '新建标题' : '复用现有标题'}，移入所选任务`
                : column.created ? '新建空标题' : '复用现有标题'} · {column.done ? '完成列' : '非完成列'}</span></li>)}
            </ul>
            <DiffView diff={preview.diff} leftLabel={props.sourcePath} rightLabel="配置后" fullContext />
            <p>确认仅提交此来源、选择与预览指纹；若文件已变化，后端会拒绝写入并要求重新预览。</p>
          </section>
        )}
        {writing && <p role="status">正在确认并写入，请勿重复提交…</p>}
      </div>
      <footer className="column-setup-dialog__actions">
        <button type="button" autoFocus className="text-action" disabled={writing} onClick={model.close}>取消</button>
        {preview ? <>
          {!props.startTask && <button type="button" className="text-action" disabled={writing} onClick={() => model.change(selection)}>返回选择</button>}
          <button type="button" className="solid-action" disabled={disabled} onClick={() => void model.apply()}>{writing ? '写入中…' : '确认并写入'}</button>
        </> : <button type="button" className="solid-action" disabled={!model.canPreview} onClick={() => void model.prepare()}>
          {phase === 'previewing' ? '生成预览…' : model.needsRepreview ? '重新预览差异' : '预览差异'}
        </button>}
      </footer>
    </dialog>
  );
}
