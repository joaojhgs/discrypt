import React from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';

const guarantees = [
  'Content-private, metadata-minimizing (not anonymous)',
  'Deletion is cooperative: online devices now, offline devices when they reconnect',
  'Relays carry ciphertext only',
];

function App() {
  return <main className="shell">
    <section className="sidebar"><h1>discrypt</h1><p>v1 native shell skeleton</p></section>
    <section className="panel"><h2>Phase 0</h2><p>Identity, device-set, MLS facade, and safety-number foundations are provided by Rust crates.</p><ul>{guarantees.map(g => <li key={g}>{g}</li>)}</ul></section>
  </main>;
}

createRoot(document.getElementById('root')!).render(<App />);
