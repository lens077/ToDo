import React from 'react';
import { createRoot } from 'react-dom/client';
import '@fontsource-variable/bricolage-grotesque';
import '@fontsource-variable/jetbrains-mono';
import { App } from './App';
import { CaptureWindow } from './components/CaptureWindow';
import { desktop } from './lib/desktop';
import './style.css';

const root = createRoot(document.getElementById('root')!);

if (desktop.currentWindowLabel() === 'capture') {
  // The capture window intentionally stays outside StrictMode so a blur can never register twice.
  root.render(<CaptureWindow />);
} else {
  root.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
