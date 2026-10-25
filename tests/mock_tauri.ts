// Lightweight browser IPC mock for Tauri v2
// Allows running actual src/App.svelte directly in tests without Rust backend

export interface SessionState {
  status: 'LoggedOut' | 'Registering' | 'AwaitingApproval' | 'Active' | 'Refreshing' | 'Expired' | 'Failed';
  data?: any;
}

export let currentMockState: SessionState = {
  status: 'Active',
  data: {
    expires_at: new Date(Date.now() + 3600000).toISOString(),
    start_url: 'https://my-company.awsapps.com/start',
    region: 'us-east-1',
  },
};

export let mockAccounts = [
  {
    account_id: '111122223333',
    account_name: 'Production Infrastructure',
    email: 'cloud-infra@example.com',
    roles: [
      { role_name: 'AdministratorAccess', account_id: '111122223333' },
      { role_name: 'ReadOnlyAccess', account_id: '111122223333' },
    ],
  },
  {
    account_id: '444455556666',
    account_name: 'Staging & QA',
    email: 'qa-team@example.com',
    roles: [
      { role_name: 'DeveloperAccess', account_id: '444455556666' },
      { role_name: 'SecOpsAuditor', account_id: '444455556666' },
    ],
  },
];

export let mockActiveProfiles = [
  {
    profile_name: 'default',
    account_id: '111122223333',
    account_name: 'Production Infrastructure',
    role_name: 'AdministratorAccess',
    expires_at: new Date(Date.now() + 3600000).toISOString(),
  },
];

const eventListeners = new Map<string, Set<(event: any) => void>>();

export function emitMockEvent(event: string, payload: any) {
  const listeners = eventListeners.get(event);
  if (listeners) {
    listeners.forEach((fn) => fn({ event, id: 0, payload }));
  }
}

export function setMockSessionState(status: SessionState['status'], data?: any) {
  if (status === 'Active') {
    currentMockState = {
      status: 'Active',
      data: data || {
        expires_at: new Date(Date.now() + 3600000).toISOString(),
        start_url: 'https://my-company.awsapps.com/start',
        region: 'us-east-1',
      },
    };
  } else if (status === 'Expired') {
    currentMockState = {
      status: 'Expired',
      data: { reason: 'Your session token has expired.' },
    };
  } else if (status === 'AwaitingApproval') {
    currentMockState = {
      status: 'AwaitingApproval',
      data: {
        user_code: 'ABCD-1234',
        verification_uri: 'https://device.sso.us-east-1.amazonaws.com/',
        expires_at: new Date(Date.now() + 900000).toISOString(),
      },
    };
  } else {
    currentMockState = { status, data };
  }
  emitMockEvent('session-state-changed', currentMockState);
}

// Attach Tauri internals mock before any Tauri import runs
if (typeof window !== 'undefined') {
  (window as any).__TAURI_EVENT_LISTENERS__ = eventListeners;

  (window as any).__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: any = {}) => {
      console.log(`[Tauri Mock IPC] invoke('${cmd}')`, args);

      switch (cmd) {
        case 'get_session_state':
          return currentMockState;

        case 'get_accounts':
        case 'refresh_accounts':
          return mockAccounts;

        case 'get_active_profiles':
          return mockActiveProfiles;

        case 'activate_role': {
          const profileName = args.customProfileName || 'default';
          const newProfile = {
            profile_name: profileName,
            account_id: args.accountId,
            account_name: args.accountName,
            role_name: args.roleName,
            expires_at: new Date(Date.now() + 3600000).toISOString(),
          };
          mockActiveProfiles = mockActiveProfiles
            .filter((p) => p.profile_name !== profileName)
            .concat(newProfile);
          emitMockEvent('active-profiles-changed', mockActiveProfiles);
          return newProfile;
        }

        case 'deactivate_profile': {
          mockActiveProfiles = mockActiveProfiles.filter(
            (p) => p.profile_name !== args.profileName
          );
          emitMockEvent('active-profiles-changed', mockActiveProfiles);
          return null;
        }

        case 'quit_app':
          alert('Mock Quit called (in production macOS, exits via app.exit(0))');
          return null;

        case 'login': {
          setMockSessionState('Refreshing');
          setTimeout(() => {
            setMockSessionState('Active');
          }, 1200);
          return null;
        }

        case 'logout': {
          setMockSessionState('LoggedOut');
          mockActiveProfiles = [];
          emitMockEvent('active-profiles-changed', mockActiveProfiles);
          return null;
        }

        case 'cancel_login': {
          setMockSessionState('LoggedOut');
          return null;
        }

        default:
          console.warn(`[Tauri Mock IPC] Unhandled command: ${cmd}`);
          return null;
      }
    },
    transformCallback: (callback: any) => callback,
    ipc: () => {},
  };

  // Polyfill window.__TAURI__.event if needed
  const origListen = (window as any).addEventListener;
}
