<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  type SessionState =
    | { status: 'LoggedOut' }
    | { status: 'Registering' }
    | { status: 'AwaitingApproval'; data: { user_code: string; verification_uri: string; expires_at: string } }
    | { status: 'Active'; data: { expires_at: string; start_url: string; region: string } }
    | { status: 'Refreshing' }
    | { status: 'Expired'; data: { reason: string } }
    | { status: 'Failed'; data: { message: string } };

  interface Role {
    role_name: string;
    account_id: string;
  }

  interface Account {
    account_id: string;
    account_name: string;
    email?: string;
    roles: Role[];
  }

  interface ActiveProfile {
    profile_name: string;
    account_id: string;
    account_name: string;
    role_name: string;
    expires_at: string;
  }

  let sessionState: SessionState = $state({ status: 'LoggedOut' });
  let startUrl: string = $state('');
  let region: string = $state('us-east-1');
  let isLoading: boolean = $state(false);
  let errorMessage: string | null = $state(null);
  let copyFeedback: boolean = $state(false);

  let isInitialized = false;

  $effect(() => {
    if (!isInitialized) return;
    if (startUrl.trim()) {
      localStorage.setItem('limen_start_url', startUrl.trim());
    }
    if (region.trim()) {
      localStorage.setItem('limen_region', region.trim());
    }
  });

  // Discovery & Multi-Profile Activation state
  let accounts: Account[] = $state([]);
  let activeProfiles: ActiveProfile[] = $state([]);
  let isFetchingAccounts: boolean = $state(false);
  let activatingRole: string | null = $state(null);
  let searchQuery: string = $state('');
  let expandedAccounts: Record<string, boolean> = $state({});
  let openMenuAccountId: string | null = $state(null);
  let copiedAccountId: string | null = $state(null);
  let copiedExportProfile: string | null = $state(null);

  // Custom profile naming per role
  let customProfileInputs: Record<string, string> = $state({});
  let editingProfileKey: string | null = $state(null);

  function formatErrorMessage(err: any, fallback: string = 'An unexpected error occurred'): string {
    if (!err) return fallback;
    if (typeof err === 'string') return err;
    if (err.detail?.message) return err.detail.message;
    if (err.message) return err.message;
    if (err.kind) {
      switch (err.kind) {
        case 'NotAuthenticated':
          return 'Not authenticated. Please sign in with AWS SSO.';
        case 'SessionExpired':
          return 'Session expired. Please sign in again.';
        case 'Cancelled':
          return 'Sign-in operation was cancelled.';
        case 'Internal':
          return err.detail || 'An internal error occurred.';
        case 'Aws':
          return err.detail?.message || err.detail?.code || 'AWS service error.';
        case 'ConfigFile':
          return err.detail?.reason || 'Configuration file error.';
        default:
          return `${err.kind}`;
      }
    }
    return typeof err === 'object' ? JSON.stringify(err) : String(err);
  }

  function toggleMenu(e: MouseEvent, accountId: string) {
    e.stopPropagation();
    openMenuAccountId = openMenuAccountId === accountId ? null : accountId;
  }

  function copyAccountId(accountId: string) {
    navigator.clipboard.writeText(accountId);
    copiedAccountId = accountId;
    setTimeout(() => {
      if (copiedAccountId === accountId) copiedAccountId = null;
    }, 2000);
    openMenuAccountId = null;
  }

  function copyExportCommand(profileName: string) {
    const cmd = `export AWS_PROFILE=${profileName}`;
    navigator.clipboard.writeText(cmd);
    copiedExportProfile = profileName;
    setTimeout(() => {
      if (copiedExportProfile === profileName) copiedExportProfile = null;
    }, 2000);
  }

  function toggleEditProfile(accountId: string, roleName: string) {
    const key = `${accountId}:${roleName}`;
    if (editingProfileKey === key) {
      editingProfileKey = null;
    } else {
      editingProfileKey = key;
      if (!customProfileInputs[key]) {
        customProfileInputs[key] = 'default';
      }
    }
  }

  let pollTimer: ReturnType<typeof setInterval> | null = null;

  async function fetchState() {
    try {
      const res = await invoke<SessionState>('get_session_state');
      const wasNotActive = sessionState.status !== 'Active';
      sessionState = res;

      if (res.status === 'Failed') {
        errorMessage = res.data.message;
      }

      if (res.status === 'Active' && wasNotActive) {
        await loadAccountsAndActiveProfiles();
      }
    } catch (e: any) {
      console.error('Failed to get session state:', e);
    }
  }

  async function loadAccountsAndActiveProfiles() {
    isFetchingAccounts = true;
    errorMessage = null;
    try {
      const [accts, profiles] = await Promise.all([
        invoke<Account[]>('get_accounts'),
        invoke<ActiveProfile[]>('get_active_profiles'),
      ]);
      accounts = accts;
      activeProfiles = profiles;
      expandedAccounts = {};
    } catch (err: any) {
      console.error('Failed to load accounts:', err);
      errorMessage = formatErrorMessage(err, 'Failed to list AWS accounts');
    } finally {
      isFetchingAccounts = false;
    }
  }

  async function handleRefreshAccounts() {
    isFetchingAccounts = true;
    errorMessage = null;
    try {
      accounts = await invoke<Account[]>('refresh_accounts');
      activeProfiles = await invoke<ActiveProfile[]>('get_active_profiles');
    } catch (err: any) {
      errorMessage = formatErrorMessage(err, 'Failed to refresh accounts');
    } finally {
      isFetchingAccounts = false;
    }
  }

  async function handleActivateRole(account: Account, role: Role, customName?: string) {
    const roleKey = `${account.account_id}:${role.role_name}`;
    const profileToUse = customName !== undefined ? customName.trim() : (customProfileInputs[roleKey] || 'default');
    const finalProfileName = profileToUse && profileToUse.trim() ? profileToUse.trim() : 'default';

    activatingRole = roleKey;
    errorMessage = null;
    try {
      const active = await invoke<ActiveProfile>('activate_role', {
        accountId: account.account_id,
        accountName: account.account_name,
        roleName: role.role_name,
        customProfileName: finalProfileName,
        setAsDefault: false,
      });

      const filtered = activeProfiles.filter((p) => p.profile_name !== active.profile_name);
      activeProfiles = [...filtered, active];
      editingProfileKey = null;
    } catch (err: any) {
      console.error('Activation failed:', err);
      errorMessage = formatErrorMessage(err, 'Failed to assume role');
    } finally {
      activatingRole = null;
    }
  }

  async function handleDeactivateProfile(profileName: string) {
    try {
      await invoke('deactivate_role', { profileName });
      activeProfiles = activeProfiles.filter((p) => p.profile_name !== profileName);
    } catch (err: any) {
      console.error('Deactivation failed:', err);
    }
  }

  function handleWindowClick() {
    openMenuAccountId = null;
  }

  onMount(async () => {
    window.addEventListener('click', handleWindowClick);

    // Restore saved startUrl and region from localStorage or backend last session
    const savedUrl = localStorage.getItem('limen_start_url');
    const savedRegion = localStorage.getItem('limen_region');
    if (savedUrl) {
      startUrl = savedUrl;
    }
    if (savedRegion) {
      region = savedRegion;
    }

    try {
      const lastSession = await invoke<{ start_url: string; region: string } | null>('get_last_session');
      if (lastSession) {
        if (!savedUrl && lastSession.start_url) {
          startUrl = lastSession.start_url;
        }
        if (!savedRegion && lastSession.region) {
          region = lastSession.region;
        }
      }
    } catch (e) {
      console.error('Failed to get last session:', e);
    }
    isInitialized = true;

    await fetchState();
    if (sessionState.status === 'Active') {
      await loadAccountsAndActiveProfiles();
    }
    pollTimer = setInterval(fetchState, 1500);
  });

  onDestroy(() => {
    window.removeEventListener('click', handleWindowClick);
    if (pollTimer) clearInterval(pollTimer);
  });

  async function handleLogin() {
    errorMessage = null;
    isLoading = true;
    try {
      localStorage.setItem('limen_start_url', startUrl.trim());
      localStorage.setItem('limen_region', region.trim());
      const res = await invoke<SessionState>('begin_login', {
        startUrl: startUrl.trim(),
        region: region.trim(),
      });
      sessionState = res;
    } catch (err: any) {
      errorMessage = formatErrorMessage(err, 'Sign in failed');
    } finally {
      isLoading = false;
    }
  }

  async function handleCancel() {
    try {
      await invoke('cancel_login');
      sessionState = { status: 'LoggedOut' };
    } catch (e: any) {
      console.error('Cancel failed:', e);
    }
  }

  async function handleLogout() {
    try {
      await invoke('logout');
      sessionState = { status: 'LoggedOut' };
      accounts = [];
      activeProfiles = [];
    } catch (e: any) {
      console.error('Logout failed:', e);
    }
  }

  async function handleQuit() {
    await invoke('quit_app');
  }

  function copyUserCode(code: string) {
    navigator.clipboard.writeText(code);
    copyFeedback = true;
    setTimeout(() => {
      copyFeedback = false;
    }, 2000);
  }

  function toggleAccount(accountId: string) {
    expandedAccounts[accountId] = !expandedAccounts[accountId];
  }

  // Filter accounts strictly by account name or 12-digit ID, and sort accounts + roles alphabetically
  let filteredAccounts = $derived(
    accounts
      .filter((acct) => {
        const q = searchQuery.toLowerCase().trim();
        if (!q) return true;
        const matchAcctName = acct.account_name.toLowerCase().includes(q);
        const matchAcctId = acct.account_id.includes(q);
        return matchAcctName || matchAcctId;
      })
      .map((acct) => ({
        ...acct,
        roles: [...acct.roles].sort((a, b) =>
          a.role_name.localeCompare(b.role_name, undefined, { sensitivity: 'base' })
        ),
      }))
      .sort((a, b) =>
        a.account_name.localeCompare(b.account_name, undefined, { sensitivity: 'base' })
      )
  );

  function getActiveProfileForRole(accountId: string, roleName: string): ActiveProfile | undefined {
    return activeProfiles.find(
      (p) => p.account_id === accountId && p.role_name === roleName
    );
  }
</script>

  <div class="popover-container">
    <!-- Header -->
    <header class="header">
    <div class="header-brand">
      <div class="brand-icon-pill">
        <svg viewBox="0 0 24 24" width="13" height="13" fill="white" stroke="white" stroke-width="1.1" stroke-linejoin="round" stroke-linecap="round">
          <polygon points="13.2,2.4 7.2,12 11.5,12 9.6,21.6 16.8,10.5 12.5,10.5" />
        </svg>
      </div>
      <span class="header-title">Limen</span>
    </div>

    <div class="status-badge">
      {#if sessionState.status === 'Active'}
        <span class="status-dot active"></span>
        <span>Connected</span>
      {:else if sessionState.status === 'AwaitingApproval' || sessionState.status === 'Registering' || sessionState.status === 'Refreshing'}
        <span class="status-dot pending"></span>
        <span>{sessionState.status}</span>
      {:else if sessionState.status === 'Failed' || sessionState.status === 'Expired'}
        <span class="status-dot error"></span>
        <span>{sessionState.status}</span>
      {:else}
        <span class="status-dot"></span>
        <span>Offline</span>
      {/if}
    </div>
  </header>

  <!-- Main Content Area -->
  <main class="content">
    {#if errorMessage}
      <div class="error-banner">
        <span>⚠️</span>
        <span style="flex: 1;">{errorMessage}</span>
      </div>
    {/if}

    {#if sessionState.status === 'LoggedOut' || sessionState.status === 'Failed'}
      <div class="card" style="display: flex; flex-direction: column; gap: 12px;">
        <h3 style="font-size: 13.5px; font-weight: 600; letter-spacing: -0.01em;">AWS IAM Identity Center</h3>
        
        <div style="display: flex; flex-direction: column; gap: 4px;">
          <label style="font-size: 11px; color: var(--text-secondary); font-weight: 500;" for="start-url">SSO Start URL</label>
          <input
            id="start-url"
            class="form-input"
            type="url"
            placeholder="https://my-company.awsapps.com/start"
            bind:value={startUrl}
          />
        </div>

        <div style="display: flex; flex-direction: column; gap: 4px;">
          <label style="font-size: 11px; color: var(--text-secondary); font-weight: 500;" for="sso-region">SSO Region</label>
          <select id="sso-region" class="form-select" bind:value={region}>
            <option value="us-east-1">us-east-1 (N. Virginia)</option>
            <option value="us-east-2">us-east-2 (Ohio)</option>
            <option value="us-west-2">us-west-2 (Oregon)</option>
            <option value="eu-west-1">eu-west-1 (Ireland)</option>
            <option value="eu-central-1">eu-central-1 (Frankfurt)</option>
            <option value="ap-southeast-1">ap-southeast-1 (Singapore)</option>
            <option value="ap-southeast-2">ap-southeast-2 (Sydney)</option>
            <option value="ap-northeast-1">ap-northeast-1 (Tokyo)</option>
          </select>
        </div>

        <button
          class="primary-btn"
          onclick={handleLogin}
          disabled={isLoading || !startUrl}
        >
          {isLoading ? 'Signing In...' : 'Sign In with AWS SSO'}
        </button>
      </div>

      <div style="font-size: 11px; color: var(--text-muted); line-height: 1.5; margin-top: auto; display: flex; justify-content: space-between; align-items: center; padding-top: 8px;">
        <span>~/.aws/credentials</span>
        <button
          class="action-btn-mini"
          style="color: var(--text-muted);"
          onclick={handleQuit}
          title="Quit Limen (⌘Q)"
        >
          Quit Limen
        </button>
      </div>

    {:else if sessionState.status === 'Registering'}
      <div class="card" style="text-align: center; padding: 28px 16px; display: flex; flex-direction: column; align-items: center; gap: 12px;">
        <svg class="spinner-icon" viewBox="0 0 24 24" width="22" height="22" style="color: #3b82f6; width: 22px; height: 22px;">
          <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" fill="none" stroke-dasharray="32" stroke-linecap="round" />
        </svg>
        <div>
          <div style="font-size: 14px; font-weight: 600; margin-bottom: 4px;">Registering Client...</div>
          <div style="font-size: 12px; color: var(--text-muted);">
            Establishing secure connection with AWS SSO portal.
          </div>
        </div>
        <div style="display: flex; gap: 8px; width: 100%; margin-top: 8px;">
          <button
            class="action-btn-mini"
            style="flex: 1; height: 30px; justify-content: center;"
            onclick={handleCancel}
          >
            Cancel
          </button>
          <button
            class="action-btn-mini"
            style="flex: 1; height: 30px; justify-content: center; color: var(--text-muted);"
            onclick={handleQuit}
            title="Quit Limen (⌘Q)"
          >
            Quit Limen
          </button>
        </div>
      </div>

    {:else if sessionState.status === 'AwaitingApproval'}
      <div class="card" style="text-align: center; padding: 20px;">
        <div style="font-size: 12px; font-weight: 500; color: var(--text-secondary); margin-bottom: 10px;">
          Confirm this code in your browser:
        </div>

        <div style="font-size: 26px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-weight: 800; letter-spacing: 3px; color: #60a5fa; background: rgba(0,0,0,0.35); padding: 10px; border-radius: 8px; margin-bottom: 14px; border: 1px dashed rgba(59, 130, 246, 0.4);">
          {sessionState.data.user_code}
        </div>

        <div style="display: flex; gap: 8px; margin-bottom: 12px;">
          <button
            class="action-btn-mini"
            style="flex: 1; height: 30px; justify-content: center;"
            onclick={() => copyUserCode(sessionState.status === 'AwaitingApproval' ? sessionState.data.user_code : '')}
          >
            {copyFeedback ? '✓ Copied' : 'Copy Code'}
          </button>
          
          <a
            class="action-btn-mini"
            style="flex: 1; height: 30px; justify-content: center; background: #3b82f6; border-color: #3b82f6; color: white; text-decoration: none;"
            href={sessionState.data.verification_uri}
            target="_blank"
            rel="noreferrer"
          >
            Open Browser
          </a>
        </div>

        <div style="font-size: 11px; color: var(--text-muted); margin-bottom: 10px;">
          Waiting for your approval in AWS portal...
        </div>

        <button
          class="btn-signout"
          style="width: 100%;"
          onclick={handleCancel}
        >
          Cancel
        </button>
      </div>

    {:else if sessionState.status === 'Active'}
      <!-- Active Profiles Card (Multiple Simultaneous Profiles) -->
      {#if activeProfiles.length > 0}
        <div class="active-profiles-card">
          <div class="active-profiles-header">
            <span>● Active Profiles ({activeProfiles.length})</span>
            <span style="font-size: 9.5px; opacity: 0.8;">~/.aws/credentials</span>
          </div>

          {#each activeProfiles as profile (profile.profile_name)}
            <div class="active-profile-row">
              <div style="overflow: hidden;">
                <div class="profile-tag">[{profile.profile_name}]</div>
                <div class="profile-role-subtext">{profile.account_name} &bull; {profile.role_name}</div>
              </div>

              <div class="profile-actions">
                <button
                  class="action-btn-mini"
                  onclick={() => copyExportCommand(profile.profile_name)}
                  title="Copy export AWS_PROFILE command"
                >
                  {#if copiedExportProfile === profile.profile_name}
                    <span style="color: var(--success); font-weight: 600;">✓ Copied</span>
                  {:else}
                    <span>export</span>
                  {/if}
                </button>

                <button
                  class="btn-remove-profile"
                  onclick={() => handleDeactivateProfile(profile.profile_name)}
                  title="Stop / Remove from ~/.aws/credentials"
                >
                  <svg viewBox="0 0 24 24" width="10" height="10" fill="currentColor">
                    <rect x="5" y="5" width="14" height="14" rx="2.5" />
                  </svg>
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}

      <!-- Search & Refresh Toolbar -->
      <div class="search-container">
        <div class="search-input-wrapper">
          <svg class="search-icon" viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>

          <input
            class="search-box"
            type="text"
            placeholder="Search accounts by name or ID..."
            bind:value={searchQuery}
          />

          {#if searchQuery}
            <button
              class="clear-search-btn"
              onclick={() => (searchQuery = '')}
              title="Clear search"
            >
              ✕
            </button>
          {/if}
        </div>

        <button
          class="refresh-btn"
          class:is-spinning={isFetchingAccounts}
          onclick={handleRefreshAccounts}
          disabled={isFetchingAccounts}
          title="Refresh account inventory"
          aria-label="Refresh accounts"
        >
          <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.2" />
          </svg>
        </button>
      </div>

      <!-- Accounts & Roles List -->
      {#if isFetchingAccounts && accounts.length === 0}
        <div style="text-align: center; padding: 28px 16px; color: var(--text-muted); font-size: 12px;">
          Discovering AWS accounts & roles...
        </div>
      {:else if accounts.length === 0}
        <div style="text-align: center; padding: 28px 16px; color: var(--text-muted); font-size: 12px;">
          No AWS accounts found.
          <div style="margin-top: 8px;">
            <button class="action-btn-mini" onclick={handleRefreshAccounts}>
              Refresh Inventory
            </button>
          </div>
        </div>
      {:else}
        <div style="display: flex; flex-direction: column; gap: 6px; margin-bottom: 10px;">
          {#each filteredAccounts as account (account.account_id)}
            <div class="account-card">
              <!-- Account Header -->
              <div
                class="account-header"
                onclick={() => toggleAccount(account.account_id)}
                role="button"
                tabindex="0"
                onkeydown={(e) => e.key === 'Enter' && toggleAccount(account.account_id)}
              >
                <div class="account-title">
                  <svg
                    class="chevron-icon"
                    class:is-expanded={expandedAccounts[account.account_id]}
                    viewBox="0 0 24 24"
                    width="12"
                    height="12"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.5"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                  >
                    <polyline points="9 18 15 12 9 6" />
                  </svg>
                  <span class="account-name-text">{account.account_name}</span>
                </div>

                <div class="account-header-actions">
                  <button
                    class="menu-dots-btn"
                    onclick={(e) => {
                      e.stopPropagation();
                      toggleMenu(e, account.account_id);
                    }}
                    title="Account options"
                    aria-label="Account options"
                  >
                    •••
                  </button>

                  {#if openMenuAccountId === account.account_id}
                    <div class="dropdown-menu">
                      <button
                        class="dropdown-item"
                        onclick={() => copyAccountId(account.account_id)}
                      >
                        {#if copiedAccountId === account.account_id}
                          <span style="color: var(--success); font-weight: 600;">✓ Copied ID</span>
                        {:else}
                          <span>Copy Account ID</span>
                        {/if}
                      </button>
                    </div>
                  {/if}
                </div>
              </div>

              <!-- Roles Under Account -->
              {#if expandedAccounts[account.account_id]}
                <div class="roles-container">
                  <div class="account-meta-row">
                    <span class="account-id-label">ID: {account.account_id}</span>
                    <button
                      class="copy-id-btn"
                      onclick={() => copyAccountId(account.account_id)}
                      title="Copy Account ID"
                    >
                      {copiedAccountId === account.account_id ? '✓ Copied' : 'Copy'}
                    </button>
                  </div>

                  {#each account.roles as role (role.role_name)}
                    {@const roleKey = `${account.account_id}:${role.role_name}`}
                    {@const activeMatch = getActiveProfileForRole(account.account_id, role.role_name)}
                    {@const isActivating = activatingRole === roleKey}
                    {@const isEditing = editingProfileKey === roleKey}

                    <div style="display: flex; flex-direction: column; gap: 3px;">
                      <div class="role-item">
                        <div style="display: flex; align-items: center; gap: 6px; flex: 1; overflow: hidden;">
                          <span style="font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                            {role.role_name}
                          </span>
                          
                          {#if activeMatch}
                            <span style="font-size: 10px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--success); font-weight: 700;">
                              [{activeMatch.profile_name}]
                            </span>
                          {:else}
                            <button
                              class="profile-badge-btn"
                              onclick={() => toggleEditProfile(account.account_id, role.role_name)}
                              title="Click to customize profile name (defaults to [default])"
                            >
                              [{customProfileInputs[roleKey] || 'default'}]
                            </button>
                          {/if}
                        </div>

                        <div style="display: flex; align-items: center; gap: 5px;">
                          {#if activeMatch}
                            <button
                              class="action-btn-mini"
                              onclick={() => copyExportCommand(activeMatch.profile_name)}
                              title="Copy export AWS_PROFILE command"
                            >
                              {#if copiedExportProfile === activeMatch.profile_name}
                                <span style="color: var(--success); font-weight: 600;">✓</span>
                              {:else}
                                <span>export</span>
                              {/if}
                            </button>

                            <button
                              class="play-btn active-stop-btn"
                              onclick={() => handleDeactivateProfile(activeMatch.profile_name)}
                              title="Stop / Deactivate role"
                              aria-label={`Stop ${role.role_name}`}
                            >
                              <svg viewBox="0 0 24 24" width="10" height="10" fill="currentColor">
                                <rect x="5" y="5" width="14" height="14" rx="2.5" />
                              </svg>
                            </button>
                          {:else}
                            <button
                              class="play-btn"
                              onclick={() => handleActivateRole(account, role)}
                              disabled={isActivating}
                              title={`Activate as [${customProfileInputs[roleKey] || 'default'}]`}
                              aria-label={`Activate ${role.role_name}`}
                            >
                              {#if isActivating}
                                <svg class="spinner-icon" viewBox="0 0 24 24">
                                  <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" fill="none" stroke-dasharray="32" stroke-linecap="round" />
                                </svg>
                              {:else}
                                <svg viewBox="0 0 24 24" width="11" height="11" fill="currentColor">
                                  <polygon points="6 4 20 12 6 20 6 4" />
                                </svg>
                              {/if}
                            </button>
                          {/if}
                        </div>
                      </div>

                      {#if isEditing && !activeMatch}
                        <div class="profile-inline-editor">
                          <span style="font-size: 10px; color: var(--text-muted);">Profile:</span>
                          <input
                            class="profile-name-input"
                            type="text"
                            placeholder="default"
                            bind:value={customProfileInputs[roleKey]}
                            onkeydown={(e) => {
                              if (e.key === 'Enter') {
                                handleActivateRole(account, role);
                              } else if (e.key === 'Escape') {
                                editingProfileKey = null;
                              }
                            }}
                          />
                          <button
                            class="action-btn-mini"
                            style="background: #3b82f6; border-color: #3b82f6; color: white;"
                            onclick={() => handleActivateRole(account, role)}
                            disabled={isActivating}
                          >
                            Activate
                          </button>
                          <button
                            class="btn-remove-profile"
                            onclick={() => (editingProfileKey = null)}
                            title="Cancel"
                          >
                            ✕
                          </button>
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <!-- Footer -->
      <footer class="footer">
        <div class="region-chip">
          <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
          </svg>
          <span>{sessionState.data.region}</span>
        </div>
        
        <div style="display: flex; align-items: center; gap: 6px;">
          <button
            class="btn-signout"
            onclick={handleLogout}
          >
            Sign Out
          </button>

          <button
            class="action-btn-mini"
            style="color: var(--text-muted);"
            onclick={handleQuit}
            title="Quit Limen (⌘Q)"
          >
            Quit
          </button>
        </div>
      </footer>

    {:else if sessionState.status === 'Expired'}
      <div class="card" style="text-align: center; padding: 24px 16px; display: flex; flex-direction: column; gap: 12px;">
        <div style="font-size: 14px; font-weight: 600; color: var(--warning);">
          Session Expired
        </div>
        <div style="font-size: 12px; color: var(--text-secondary);">
          {sessionState.data.reason || 'Your AWS SSO session has expired. Please sign in again.'}
        </div>
        <button class="primary-btn" onclick={handleLogin} disabled={isLoading}>
          {isLoading ? 'Signing In...' : 'Sign In Again'}
        </button>
        <div style="display: flex; gap: 8px; justify-content: center; margin-top: 4px;">
          <button
            class="action-btn-mini"
            style="flex: 1; height: 28px; justify-content: center;"
            onclick={handleCancel}
          >
            Edit SSO Details
          </button>
          <button
            class="action-btn-mini"
            style="flex: 1; height: 28px; justify-content: center; color: var(--text-muted);"
            onclick={handleQuit}
            title="Quit Limen (⌘Q)"
          >
            Quit Limen
          </button>
        </div>
      </div>
    {/if}
  </main>
</div>
