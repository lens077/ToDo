import * as React from 'react';
import { Icon } from './Icon';

export interface SubViewCrumb {
  label: string;
  /** Present on parent crumbs; the last crumb is the current page and never clickable. */
  onClick?: () => void;
}

interface SubViewHeaderProps {
  /** Path from the primary view down to this page; the last crumb is current. */
  crumbs: SubViewCrumb[];
  /** Back action. Omitted on paths that already carry their own back button. */
  onBack?: () => void;
  backLabel?: string;
  /** Optional right-aligned slot (counts, status chips). */
  trailing?: React.ReactNode;
}

/**
 * 二级路径的页头：面包屑 + 返回。
 *
 * 一级视图（侧栏/底部导航里的那些）不需要它；从一级视图点进去的次级页面——
 * 随记分拣、文章阅读、来源预览——必须能看见自己从哪来、怎么回去，不能只靠
 * 侧栏高亮去猜。
 */
export function SubViewHeader({ crumbs, onBack, backLabel = '返回', trailing }: SubViewHeaderProps) {
  return (
    <div className="sub-view-head">
      {onBack && (
        <button type="button" className="sub-view-head__back text-action" onClick={onBack}>
          <Icon name="chevron" className="sub-view-head__back-icon" /> {backLabel}
        </button>
      )}
      <nav className="sub-view-head__crumbs" aria-label="面包屑">
        {crumbs.map((crumb, index) => {
          const current = index === crumbs.length - 1;
          return (
            <span className="sub-view-head__part" key={`${index}:${crumb.label}`}>
              {!current && crumb.onClick ? (
                <button type="button" className="sub-view-head__crumb" onClick={crumb.onClick}>
                  {crumb.label}
                </button>
              ) : (
                <span className="sub-view-head__crumb is-current" aria-current={current ? 'page' : undefined}>
                  {crumb.label}
                </span>
              )}
              {/* 分隔符跟着前一段：窄屏换行时不会只剩一个孤零零的斜杠。 */}
              {!current && <span className="sub-view-head__sep" aria-hidden="true">/</span>}
            </span>
          );
        })}
      </nav>
      {trailing}
    </div>
  );
}
