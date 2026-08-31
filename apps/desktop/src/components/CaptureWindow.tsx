import * as React from 'react';
import { desktop } from '../lib/desktop';
import { Icon } from './Icon';

export function CaptureWindow() {
  const [text, setText] = React.useState('');
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState('');
  const [saved, setSaved] = React.useState(false);
  const textareaRef = React.useRef<HTMLTextAreaElement>(null);
  const textRef = React.useRef('');
  const dirtyRef = React.useRef(false);
  const attemptedTextRef = React.useRef('');
  const inFlightRef = React.useRef(false);
  const suppressBlurRef = React.useRef(false);

  const reset = React.useCallback(() => {
    textRef.current = '';
    dirtyRef.current = false;
    attemptedTextRef.current = '';
    setText('');
  }, []);

  const saveOnce = React.useCallback(async () => {
    const value = textRef.current;
    if (!dirtyRef.current || !value.trim() || inFlightRef.current || attemptedTextRef.current === value) return;

    attemptedTextRef.current = value;
    inFlightRef.current = true;
    setSaving(true);
    setError('');
    setSaved(false);
    try {
      await desktop.captureAppend(value);
      if (textRef.current === value) reset();
      setSaved(true);
      suppressBlurRef.current = true;
      await desktop.hideCurrentWindow();
    } catch (reason) {
      attemptedTextRef.current = '';
      setError(reason instanceof Error ? reason.message : String(reason));
      window.requestAnimationFrame(() => textareaRef.current?.focus());
    } finally {
      inFlightRef.current = false;
      setSaving(false);
    }
  }, [reset]);

  const dismiss = React.useCallback(async () => {
    suppressBlurRef.current = true;
    reset();
    setError('');
    await desktop.hideCurrentWindow();
  }, [reset]);

  React.useEffect(() => {
    const focusInput = () => window.requestAnimationFrame(() => textareaRef.current?.focus());
    const saveOnBlur = () => {
      if (suppressBlurRef.current) {
        suppressBlurRef.current = false;
        return;
      }
      void saveOnce();
    };
    const handleWindowKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        void dismiss();
      } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        void saveOnce();
      }
    };
    focusInput();
    window.addEventListener('focus', focusInput);
    window.addEventListener('blur', saveOnBlur);
    window.addEventListener('keydown', handleWindowKeyDown);
    return () => {
      window.removeEventListener('focus', focusInput);
      window.removeEventListener('blur', saveOnBlur);
      window.removeEventListener('keydown', handleWindowKeyDown);
    };
  }, [dismiss, saveOnce]);

  return (
    <main className="capture-window">
      <header className="capture-window__header">
        <div className="capture-window__brand">
          <span>FT</span>
          <strong>快速随记</strong>
        </div>
        <button className="capture-window__close" type="button" onClick={() => void dismiss()} aria-label="关闭速记窗口">
          <kbd>Esc</kbd>
        </button>
      </header>

      <section className="capture-window__sheet">
        <span className="capture-window__number">CAPTURE / 01</span>
        <label htmlFor="capture-window-text">现在想到什么？</label>
        <textarea
          ref={textareaRef}
          id="capture-window-text"
          value={text}
          placeholder="记下原话，不用整理……"
          rows={5}
          disabled={saving}
          onChange={(event) => {
            const value = event.target.value;
            textRef.current = value;
            dirtyRef.current = true;
            attemptedTextRef.current = '';
            setText(value);
            setSaved(false);
            if (error) setError('');
          }}
        />
        <div className="capture-window__rule" />
        <footer className="capture-window__footer">
          <span>{new Blob([text]).size.toLocaleString('zh-CN')} / 65,536 bytes</span>
          <button className="capture-save" type="button" disabled={!text.trim() || saving} onClick={() => void saveOnce()}>
            {saving ? '正在存入…' : '存入随记'}
            <span><kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>↵</kbd></span>
          </button>
        </footer>
      </section>

      {error && (
        <div className="capture-window__error" role="alert">
          <Icon name="alert" />
          <span><strong>没有保存，原文仍在。</strong>{error}</span>
        </div>
      )}
      {saved && !desktop.isTauri && <p className="capture-window__saved" role="status">已存入随记。</p>}
    </main>
  );
}
