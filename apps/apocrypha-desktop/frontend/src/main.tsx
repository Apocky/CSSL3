import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App.tsx';
import './styles.css';

const host = document.getElementById('root');
if (!host) throw new Error('Apocrypha could not find its window root.');
createRoot(host).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
