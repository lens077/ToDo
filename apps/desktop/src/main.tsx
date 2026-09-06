import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { CaptureWindow } from './components/CaptureWindow';
import { desktop, initTransport } from './lib/desktop';
import './style.css';

const root = createRoot(document.getElementById('root')!);

if (desktop.currentWindowLabel() === 'capture') {
  // The capture window intentionally stays outside StrictMode so a blur can never register twice.
  root.render(<CaptureWindow />);
} else {
  // Resolve the backend before the first render. Mounting first would let the
  // initial snapshot load against one transport and then swap underneath it.
  void initTransport().then(() => {
    root.render(
      <React.StrictMode>
        <App />
      </React.StrictMode>,
    );
  });
}
