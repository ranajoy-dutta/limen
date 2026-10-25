import './mock_tauri';
import { mount } from 'svelte';
import App from '../src/App.svelte';
import '../src/app.css';
import { setMockSessionState, currentMockState } from './mock_tauri';

// Mount the actual production Svelte App component directly into the popover container
const target = document.getElementById('limen-svelte-root');
if (target) {
  mount(App, { target });
}

// Expose state switcher for the macOS test dashboard
(window as any).setSimState = (state: string) => {
  // Update button highlights
  document.querySelectorAll('.sim-controller .sim-pill').forEach((btn) => {
    btn.classList.remove('active', 'green', 'yellow', 'red');
  });

  const activeBtn = document.getElementById(`btn-${state}`);
  if (activeBtn) {
    activeBtn.classList.add('active');
    if (state === 'active') activeBtn.classList.add('green');
    if (state === 'refreshing') activeBtn.classList.add('yellow');
    if (state === 'expired') activeBtn.classList.add('red');
  }

  const menubarDot = document.getElementById('menubarStatusDot');
  if (menubarDot) {
    menubarDot.className = 'menubar-status-dot';
  }

  if (state === 'active') {
    if (menubarDot) menubarDot.classList.add('active');
    setMockSessionState('Active');
  } else if (state === 'refreshing') {
    if (menubarDot) menubarDot.classList.add('refreshing');
    setMockSessionState('Refreshing');
  } else if (state === 'expired') {
    if (menubarDot) menubarDot.classList.add('expired');
    setMockSessionState('Expired');
  } else if (state === 'approval') {
    if (menubarDot) menubarDot.classList.add('refreshing');
    setMockSessionState('AwaitingApproval');
  } else if (state === 'logged_out') {
    if (menubarDot) menubarDot.classList.add('expired');
    setMockSessionState('LoggedOut');
  }
};

export function alignPopoverToTray() {
  const popover = document.getElementById('popoverAnchor');
  const btn = document.getElementById('limenTrayBtn');
  if (popover && btn) {
    const rect = btn.getBoundingClientRect();
    const popoverWidth = popover.offsetWidth || 380;
    const left = rect.left + rect.width / 2 - popoverWidth / 2;
    const clampedLeft = Math.min(Math.max(10, left), window.innerWidth - popoverWidth - 10);
    popover.style.left = `${Math.round(clampedLeft)}px`;
    popover.style.right = 'auto';
    popover.style.top = `${Math.round(rect.bottom + 4)}px`;
  }
}

window.addEventListener('resize', alignPopoverToTray);
window.addEventListener('load', alignPopoverToTray);
document.addEventListener('DOMContentLoaded', alignPopoverToTray);
setTimeout(alignPopoverToTray, 10);
setTimeout(alignPopoverToTray, 100);

// macOS UI controls
(window as any).togglePopover = () => {
  const popover = document.getElementById('popoverAnchor');
  const btn = document.getElementById('limenTrayBtn');
  const menu = document.getElementById('trayContextMenu');
  if (menu) menu.classList.remove('visible');

  if (popover) {
    if (popover.style.display === 'none') {
      popover.style.display = 'block';
      if (btn) btn.classList.add('is-active');
      alignPopoverToTray();
    } else {
      popover.style.display = 'none';
      if (btn) btn.classList.remove('is-active');
    }
  }
};

(window as any).showContextMenu = (e: MouseEvent) => {
  e.preventDefault();
  e.stopPropagation();
  const menu = document.getElementById('trayContextMenu');
  if (menu) menu.classList.toggle('visible');
};

(window as any).openAboutWindow = () => {
  const menu = document.getElementById('trayContextMenu');
  if (menu) menu.classList.remove('visible');
  const about = document.getElementById('aboutWindow');
  if (about) about.classList.add('visible');
};

(window as any).closeAboutWindow = () => {
  const about = document.getElementById('aboutWindow');
  if (about) about.classList.remove('visible');
};

document.addEventListener('click', (e) => {
  const menu = document.getElementById('trayContextMenu');
  if (menu && !menu.contains(e.target as Node)) {
    menu.classList.remove('visible');
  }
});

// Real-time macOS Clock ticking
function updateClock() {
  const clockEl = document.getElementById('macosClock');
  if (clockEl) {
    const now = new Date();
    const options: Intl.DateTimeFormatOptions = { weekday: 'short', month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' };
    clockEl.textContent = now.toLocaleDateString('en-US', options).replace(/,/g, '');
  }
}
setInterval(updateClock, 1000);
updateClock();
