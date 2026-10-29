import * as React from 'react';
import { desktop, type ContractCheckReport } from '../lib/desktop';

/**
 * Project Contract 规范视图：展示 `.virgo/project.yaml` 的确定性校验结果。
 *
 * 数据来自后端真实校验（todo-core 的 check_contract），不使用硬编码状态。
 * 每条 finding 都带稳定规则 ID（VPC001…）与证据路径，与 CLI
 * `todo contract check` 的输出一致；阻断性发现（错误级）在页头计数提示。
 */
interface ContractViewProps {
  readOnly: boolean;
}

export function ContractView({ readOnly: _readOnly }: ContractViewProps) {
  const [report, setReport] = React.useState<ContractCheckReport | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const refresh = React.useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      setReport(await desktop.contractCheck());
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  React.useEffect(() => {
    void refresh();
  }, [refresh]);

  const errors = report?.findings.filter((finding) => finding.severity === 'error') ?? [];
  const warnings = report?.findings.filter((finding) => finding.severity === 'warning') ?? [];

  return (
    <section className="contract-view" aria-label="Project Contract 规范">
      <header className="review-header">
        <h2>规范</h2>
        <p className="review-subtitle">
          Project Contract（<code>.virgo/project.yaml</code>）声明项目身份、文档角色映射与检查策略。
          校验离线且确定，结果与 <code>todo contract check</code> 一致。
        </p>
        <div className="review-actions">
          <button type="button" className="review-button" onClick={() => void refresh()} disabled={busy}>
            {busy ? '检查中…' : '重新检查'}
          </button>
        </div>
      </header>

      {error && <p className="source-unavailable" role="alert">{error}</p>}

      {report && (
        <>
          <div className="contract-status">
            <span className={`contract-status__pill contract-status__pill--${errors.length ? 'error' : 'ok'}`}>
              {errors.length ? `${errors.length} 项阻断` : '无阻断发现'}
            </span>
            <span className="contract-status__pill contract-status__pill--muted">
              {warnings.length} 项警告
            </span>
            <span className="contract-status__pill contract-status__pill--muted">
              策略：{report.enforcement}
            </span>
            {report.contract_path && (
              <code className="contract-status__path">{report.contract_path}</code>
            )}
          </div>

          {report.findings.length === 0 ? (
            <p className="review-empty" role="status">规范检查通过，没有发现。</p>
          ) : (
            <ul className="review-list">
              {report.findings.map((finding, index) => (
                <li key={`${finding.rule}:${finding.path ?? ''}:${index}`} className="contract-finding">
                  <span className={`review-tag contract-finding__severity--${finding.severity}`}>
                    {finding.severity === 'error' ? '阻断' : finding.severity === 'warning' ? '警告' : '提示'}
                  </span>
                  <code>{finding.rule}</code>
                  <span>{finding.message}</span>
                  {finding.path && <code className="contract-finding__path">{finding.path}</code>}
                </li>
              ))}
            </ul>
          )}

          {!report.contract_path && (
            <p className="review-empty">
              当前项目没有 <code>.virgo/project.yaml</code>。在项目目录运行{' '}
              <code>todo project adopt</code> 可以扫描文档并生成提案。
            </p>
          )}
        </>
      )}
    </section>
  );
}
