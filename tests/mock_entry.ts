import './mock_tauri';
import { mount } from 'svelte';
import App from '../src/App.svelte';
import '../src/app.css';
import { setMockSessionState } from './mock_tauri';

// Mount the ACTUAL production Svelte 5 App component directly into the popover container
const target = document.getElementById('limen-svelte-root');
if (target) {
  mount(App, { target });
}

// Update menu bar tray dot based on session state
function updateTrayStatusDot(state: string) {
  const menubarDot = document.getElementById('menubarStatusDot');
  if (!menubarDot) return;
  menubarDot.className = 'menubar-status-dot';
  if (state === 'active') {
    menubarDot.classList.add('active');
  } else if (state === 'refreshing' || state === 'approval') {
    menubarDot.classList.add('refreshing');
  } else if (state === 'expired') {
    menubarDot.classList.add('expired');
  }
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
    if (state === 'refreshing' || state === 'approval') activeBtn.classList.add('yellow');
    if (state === 'expired') activeBtn.classList.add('red');
  }

  updateTrayStatusDot(state);

  if (state === 'active') {
    setMockSessionState('Active');
  } else if (state === 'refreshing') {
    setMockSessionState('Refreshing');
  } else if (state === 'expired') {
    setMockSessionState('Expired');
  } else if (state === 'approval') {
    setMockSessionState('AwaitingApproval');
  } else if (state === 'logged_out') {
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
    popover.style.top = `${Math.round(rect.bottom + 6)}px`;
  }
}

window.addEventListener('resize', alignPopoverToTray);
window.addEventListener('load', alignPopoverToTray);
document.addEventListener('DOMContentLoaded', alignPopoverToTray);
setTimeout(alignPopoverToTray, 10);
setTimeout(alignPopoverToTray, 100);

// macOS UI controls
(window as any).toggleLimenPopover = (e?: MouseEvent) => {
  if (e) {
    e.stopPropagation();
    e.preventDefault();
  }
  const popover = document.getElementById('popoverAnchor');
  const btn = document.getElementById('limenTrayBtn');
  const menu = document.getElementById('trayContextMenu');
  if (menu) menu.classList.remove('visible');

  if (popover) {
    if (popover.style.display === 'none' || !popover.style.display) {
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

(window as any).openAboutWindow = (e?: MouseEvent) => {
  if (e) e.stopPropagation();
  const menu = document.getElementById('trayContextMenu');
  if (menu) menu.classList.remove('visible');
  const about = document.getElementById('aboutWindow');
  if (about) about.classList.add('visible');
};

(window as any).closeAboutWindow = (e?: MouseEvent) => {
  if (e) e.stopPropagation();
  const about = document.getElementById('aboutWindow');
  if (about) about.classList.remove('visible');
};

(window as any).handleMockQuit = (e?: MouseEvent) => {
  if (e) e.stopPropagation();
  alert('Quit Limen (in production app, calls app.exit(0))');
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
    const options: Intl.DateTimeFormatOptions = {
      weekday: 'short',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    };
    clockEl.textContent = now.toLocaleDateString('en-US', options).replace(/,/g, '');
  }
}
setInterval(updateClock, 1000);
updateClock();
