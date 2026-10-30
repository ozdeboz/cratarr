import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { enterCloses } from './lib/enterCloses';
import { markGpu } from './lib/gpu';
// The 0075 spike's switch: the brake's rules from the WebAssembly where this browser asks for them.
import './lib/wasm/brake';
// The visuals opening by themselves where this device says so (lib/visualsAuto).
import './lib/visualsAuto.svelte';
import { inputWay } from './lib/inputWay';
import { watchForUpdates } from './lib/update.svelte';

markGpu();
export default mount(App, { target: document.getElementById('app')! });

enterCloses();
inputWay();

// A new Cratarr live while this page is open: the page reloads itself when nothing would be lost.
watchForUpdates();

// Installable app (PWA). Browsers only allow service workers on HTTPS or localhost; over plain http the page
// still works, it just cannot be installed.
if ('serviceWorker' in navigator && window.isSecureContext) {
  navigator.serviceWorker.register('/sw.js').catch((e) => console.warn('Service worker not registered', e));
}
