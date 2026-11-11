import * as React from 'react';
import { Icon } from './Icon';

/**
 * 可选的分步引导向导（docs/onboarding-spec.md §0 反目标 1 的 2026-11-12 拍板）。
 *
 * 默认不启用：只在设置里点「打开引导向导」时出现，没有持久化状态，也没有
 * 首次运行拦截。每一步都只是说明和既有入口的快捷方式，不新增任何写盘行为。
 */
interface OnboardingWizardProps {
  projectPath: string | null;
  onChooseProject: () => void;
  /** 跳到扫描视图；扫描本身由 ReviewView 负责，这里不重复实现。 */
  onOpenScan: () => void;
  onClose: () => void;
}

const STEPS = [
  {
    title: '任务留在你自己的仓库里',
    body: (
      <>
        <p>清单是一份普通的 TODO.md，跟着代码一起进 Git、一起 review。桌面端只修改它认识的行；文件被删除或移动时会如实报错，而不是猜一个结果。</p>
        <p>这个向导随时可以跳过，跳过不会影响任何功能。</p>
      </>
    ),
  },
  {
    title: '打开一个项目',
    body: null,
  },
  {
    title: '先扫描仓库里已有的 TODO',
    body: (
      <>
        <p>
          <code>todo scan</code> 会找出仓库里早已写下、却从未被跟踪的 TODO / FIXME 注释和 checkbox，整理成一份提案。扫描默认只读：不加
          {' '}<code>--import --yes</code> 不会改动任何文件。
        </p>
        <p>这是最快看到这份清单价值的方式，不需要先手动录入任务。</p>
      </>
    ),
  },
  {
    title: '让 agent 读到这份清单',
    body: (
      <>
        <p>
          安装会话钩子后，agent 每次开始工作都会自动读到任务清单，不必再手动粘贴。
        </p>
        <pre className="onboarding-command"><code>todo hooks install</code></pre>
        <p>
          命令会检测本机已有的 claude / codex / deepseek-harness 配置，只写对应的文件，并在修改前先备份。没有配置的 agent 不会被创建目录。
        </p>
      </>
    ),
  },
  {
    title: '对账：只报告，不改写',
    body: (
      <>
        <p>
          对账引擎用 Git 物证核对任务与代码的实际状态，指出「声称已完成、但代码里没有对应改动」这类不一致。
          <strong>它只报告，不改写你的完成状态</strong>，每条建议都由你确认。
        </p>
        <p>结构化对账不需要任何密钥，开箱可用。语义对账是可选能力：用你自己的模型 API key（BYOK），请求不经过我们的服务器。</p>
      </>
    ),
  },
];

export function OnboardingWizard({ projectPath, onChooseProject, onOpenScan, onClose }: OnboardingWizardProps) {
  const [step, setStep] = React.useState(0);
  const current = STEPS[step];
  const last = step === STEPS.length - 1;

  React.useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose]);

  return (
    <div
      className="provider-dialog"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <section className="settings-panel" role="dialog" aria-modal="true" aria-labelledby="onboarding-title">
        <header className="settings-panel__header">
          <div>
            <h2 id="onboarding-title">引导向导</h2>
            <p className="onboarding-count">第 {step + 1} 步，共 {STEPS.length} 步</p>
          </div>
          <button type="button" className="icon-action settings-panel__close" aria-label="关闭引导向导" onClick={onClose}>
            <Icon name="close" />
          </button>
        </header>

        <div className="onboarding-body">
          <ol className="onboarding-progress" aria-hidden="true">
            {STEPS.map((item, index) => (
              <li key={item.title} className={index === step ? 'is-current' : index < step ? 'is-done' : ''} />
            ))}
          </ol>

          <h3>{current.title}</h3>
          <div className="onboarding-copy">
            {step === 1 ? (
              projectPath ? (
                <>
                  <p>当前项目：</p>
                  <code className="onboarding-path">{projectPath}</code>
                  <p>换项目会重新递归查找 TODO.md、TODO、TODO.txt；桌面端不会自动创建任何文件。</p>
                </>
              ) : (
                <>
                  <p>还没有打开项目。选择一个仓库目录，桌面端会递归查找里面已有的任务文件，保留每项任务的来源。</p>
                  <p>目录里还没有 TODO.md 时不会静默创建：可以在空状态里显式创建，或先在仓库中运行 <code>todo init</code>。</p>
                </>
              )
            ) : (
              current.body
            )}
          </div>

          {step === 1 && (
            <div className="onboarding-inline-actions">
              <button type="button" className="line-action" onClick={onChooseProject}>
                {projectPath ? '换一个项目' : '选择项目目录'}
              </button>
            </div>
          )}
          {step === 2 && (
            <div className="onboarding-inline-actions">
              <button type="button" className="line-action" onClick={onOpenScan}>
                打开扫描视图
              </button>
            </div>
          )}

          <footer className="onboarding-actions">
            <button type="button" className="text-action" onClick={onClose}>
              跳过
            </button>
            <div className="onboarding-actions__nav">
              <button type="button" className="text-action" disabled={step === 0} onClick={() => setStep((value) => value - 1)}>
                上一步
              </button>
              <button
                type="button"
                className="text-action text-action--primary"
                onClick={() => (last ? onClose() : setStep((value) => value + 1))}
              >
                {last ? '完成' : '下一步'}
              </button>
            </div>
          </footer>
        </div>
      </section>
    </div>
  );
}
