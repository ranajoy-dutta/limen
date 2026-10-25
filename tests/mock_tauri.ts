// Tauri IPC and Events Mock for Limen Testing & macOS Preview
// Intercepts Tauri IPC so the ACTUAL src/App.svelte runs directly in the browser!
import { mockIPC } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';

export interface SessionState {
  status: 'LoggedOut' | 'Registering' | 'AwaitingApproval' | 'Active' | 'Refreshing' | 'Expired' | 'Failed';
  data?: any;
}

export interface ActiveProfile {
  profile_name: string;
  account_id: string;
  account_name: string;
  role_name: string;
  expires_at: string;
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

export let mockActiveProfiles: ActiveProfile[] = [];

// Initialize mock IPC with shouldMockEvents: true
mockIPC(
  async (cmd: string, args: any = {}) => {
    console.log(`[Limen Mock IPC] invoke("${cmd}")`, args);

    switch (cmd) {
      case 'get_session_state':
        return currentMockState;

      case 'get_last_session':
        return {
          start_url: 'https://my-company.awsapps.com/start',
          region: 'us-east-1',
        };

      case 'get_accounts':
      case 'refresh_accounts':
        return mockAccounts;

      case 'get_active_profiles':
        return mockActiveProfiles;

      case 'begin_login': {
        currentMockState = {
          status: 'AwaitingApproval',
          data: {
            user_code: 'ABCD-1234',
            verification_uri: 'https://device.sso.us-east-1.amazonaws.com/',
            expires_at: new Date(Date.now() + 900000).toISOString(),
          },
        };
        await emit('limen://session-state-changed', currentMockState);
        return currentMockState;
      }

      case 'cancel_login': {
        currentMockState = { status: 'LoggedOut' };
        await emit('limen://session-state-changed', currentMockState);
        return null;
      }

      case 'logout': {
        currentMockState = { status: 'LoggedOut' };
        mockActiveProfiles = [];
        await emit('limen://session-state-changed', currentMockState);
        await emit('limen://profiles-changed', mockActiveProfiles);
        return null;
      }

      case 'activate_role': {
        const profileName = (args.customProfileName && args.customProfileName.trim()) || 'default';
        const newProfile: ActiveProfile = {
          profile_name: profileName,
          account_id: args.accountId,
          account_name: args.accountName,
          role_name: args.roleName,
          expires_at: new Date(Date.now() + 3600000).toISOString(),
        };
        mockActiveProfiles = mockActiveProfiles
          .filter((p) => p.profile_name !== profileName)
          .concat(newProfile);
        await emit('limen://profiles-changed', mockActiveProfiles);
        return newProfile;
      }

      case 'deactivate_role': {
        mockActiveProfiles = mockActiveProfiles.filter(
          (p) => p.profile_name !== args.profileName
        );
        await emit('limen://profiles-changed', mockActiveProfiles);
        return null;
      }

      case 'quit_app': {
        console.log('[Limen Mock] quit_app called');
        alert('Quit Limen (in production app, calls app.exit(0))');
        return null;
      }

      default:
        console.warn(`[Limen Mock IPC] Unhandled command: ${cmd}`);
        return null;
    }
  },
  { shouldMockEvents: true }
);

export async function setMockSessionState(status: SessionState['status'], data?: any) {
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
      data: data || { reason: 'Your AWS SSO session has expired. Please sign in again.' },
    };
    mockActiveProfiles = [];
  } else if (status === 'AwaitingApproval') {
    currentMockState = {
      status: 'AwaitingApproval',
      data: data || {
        user_code: 'ABCD-1234',
        verification_uri: 'https://device.sso.us-east-1.amazonaws.com/',
        expires_at: new Date(Date.now() + 900000).toISOString(),
      },
    };
    mockActiveProfiles = [];
  } else if (status === 'Refreshing') {
    currentMockState = { status: 'Refreshing' };
  } else if (status === 'LoggedOut') {
    currentMockState = { status: 'LoggedOut' };
    mockActiveProfiles = [];
  } else {
    currentMockState = { status, data };
  }

  await emit('limen://session-state-changed', currentMockState);
  await emit('limen://profiles-changed', mockActiveProfiles);
}
