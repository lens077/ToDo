import * as React from 'react';
import { transportCapabilities } from '../lib/desktop';

function localServiceUrl(value: string): string | null {
  try {
    const url = new URL(value);
    if (url.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname)
      || url.username || url.password || url.pathname !== '/' || url.search || url.hash) return null;
    return url.href;
  } catch { return null; }
}

export function ProjectPickerDialog({ onClose }: { onClose: () => void }) {
  const ref = React.useRef<HTMLDialogElement>(null);
  const [address, setAddress] = React.useState('http://127.0.0.1:3017/');
  const serviceUrl = localServiceUrl(address);
  const capabilities = transportCapabilities();

  React.useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const dialog = ref.current;
    dialog?.showModal();
    return () => {
      dialog?.close();
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  return (
    <dialog ref={ref} className="project-picker" aria-labelledby="project-picker-title" onCancel={(event) => { event.preventDefault(); onClose(); }}>
      <header className="completion-panel__header">
        <h2 id="project-picker-title">打开项目</h2>
        <p>{capabilities ? '当前连接到项目服务' : '当前为演示模式'}</p>
      </header>
      <div className="project-picker__body">
        <p>{capabilities
          ? '浏览器使用服务端启动时指定的项目，不允许网页任意打开本机目录。切换项目请使用桌面端，或在目标仓库启动另一个项目服务。'
          : 'field-notes 是演示数据，不是你的本机项目。浏览器预览无法访问磁盘；点击这里不会选中或改写任何真实目录。'}</p>
        {capabilities?.project && <p>服务端项目：<code>{capabilities.project}</code></p>}
        <section>
          <h3>用桌面端选择本机目录</h3>
          <p>在 ToDo 源码仓库根目录运行以下命令。桌面窗口中的项目按钮会打开系统目录选择器。</p>
          <pre><code>pnpm --filter @todo/desktop tauri dev</code></pre>
        </section>
        <section>
          <h3>在浏览器打开真实项目</h3>
          <p>先在 ToDo 源码根目录运行 <code>pnpm build</code>。然后进入目标仓库，在终端执行以下命令，将示例路径换成构建目录的绝对路径：</p>
          <pre><code>todo serve --host 127.0.0.1 --port 3017 --web-root /path/to/ToDo/apps/desktop/dist</code></pre>
          <p>需已安装 todo CLI。服务以当前目录为项目根，递归发现 TODO.md、TODO 和 TODO.txt（不区分大小写）；没有任务文件也可打开。发现不会自动初始化或接管文件。已有项目服务可直接填写地址。</p>
          <label className="provider-field">
            <span>本机项目服务地址</span>
            <input type="url" value={address} onChange={(event) => setAddress(event.target.value)} spellCheck={false} aria-invalid={!serviceUrl} aria-describedby={!serviceUrl ? 'project-service-error' : undefined} />
          </label>
          {!serviceUrl && <p id="project-service-error" role="alert">请输入本机 HTTP 服务根地址，例如 http://127.0.0.1:3017/，不含账号、路径或查询参数。</p>}
          <p>在新标签页打开，不替换当前项目。若服务未启动，新标签页会显示连接失败；当前页面的数据保持不变。</p>
          {serviceUrl && <a className="solid-action" href={serviceUrl} target="_blank" rel="noopener noreferrer">打开本机项目服务</a>}
        </section>
        <footer className="completion-panel__actions">
          <button autoFocus type="button" className="text-action" onClick={onClose}>取消</button>
        </footer>
      </div>
    </dialog>
  );
}
