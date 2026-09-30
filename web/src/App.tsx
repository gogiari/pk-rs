import { useCallback, useEffect, useState, type FormEvent, type ReactNode } from 'react'
import {
  ArrowClockwiseIcon, ArrowRightIcon, CaretDownIcon, CopyIcon,
  EyeIcon, EyeSlashIcon, FileTextIcon, GearSixIcon, GlobeIcon, InfoIcon,
  KeyIcon, LockKeyIcon, MoonIcon, RocketLaunchIcon, SignOutIcon, SunIcon, TerminalWindowIcon,
} from '@phosphor-icons/react'
import { getConfig, getLogs, getStatus, saveConfig, tunnelAction, type Settings, type Status } from './api'
import { getLatestRelease, isNewerVersion, type LatestRelease } from './update'
import rocketMark from './assets/rocket-mark.png'

const defaultSettings: Settings = {
  ssh_target: '', ssh_password: '', ssh_key_path: '', http_port: 3128,
  socks_port: 1080, no_proxy: '', auto_connect: false, auto_open_browser: true,
}

type AuthMethod = 'password' | 'key'
type Pending = 'save' | 'connect' | 'disconnect' | null
type ConnectionState = 'loading' | 'unavailable' | 'connected' | 'disconnected'
type ServiceState = 'online' | 'offline' | 'unknown' | 'waiting'
type ThemePreference = 'system' | 'dark' | 'light'
type UpdateState = 'checking' | 'current' | 'available' | 'error'

function errorMessage(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback
}

function parseLog(line: string): { time: string; level: 'error' | 'warn' | 'info'; message: string } {
  const structured = /^\[([^\]]+)\]\s+\[(INFO|WARN|ERROR)\]\s*(.*)$/i.exec(line)
  if (structured) return { time: structured[1], level: structured[2].toLowerCase() as 'error' | 'warn' | 'info', message: structured[3] }
  const dated = /^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d)\s+(.*)$/.exec(line)
  const message = dated?.[2] ?? line
  const level = /error|fail|실패|오류|거부|종료 코드/i.test(message) ? 'error'
    : /warn|경고|대기|재시도/i.test(message) ? 'warn' : 'info'
  return { time: dated?.[1] ?? '', level, message }
}

function ServiceRow({ icon, name, state, detail }: {
  icon: ReactNode; name: string; state: ServiceState; detail: string
}) {
  const stateText = { online: '작동 중', offline: '연결 안 됨', unknown: '확인 불가', waiting: '대기 중' }[state]
  return <div className="service-row">
    <span className="service-icon" aria-hidden="true">{icon}</span>
    <span className="service-name">{name}</span>
    <strong className={`service-state ${state}`}>{stateText}</strong>
    <span className="sr-only">{detail}</span>
  </div>
}

export default function App() {
  const [themePreference, setThemePreference] = useState<ThemePreference>(() => {
    const mode = document.documentElement.dataset.themeMode
    return mode === 'light' || mode === 'dark' ? mode : 'system'
  })
  const [settings, setSettings] = useState<Settings>(defaultSettings)
  const [authMethod, setAuthMethod] = useState<AuthMethod>('password')
  const [rememberPassword, setRememberPassword] = useState(false)
  const [configLoading, setConfigLoading] = useState(true)
  const [configError, setConfigError] = useState(false)
  const [status, setStatus] = useState<Status | null>(null)
  const [statusError, setStatusError] = useState(false)
  const [logs, setLogs] = useState<string[]>([])
  const [logsLoading, setLogsLoading] = useState(true)
  const [logsError, setLogsError] = useState(false)
  const [showAllLogs, setShowAllLogs] = useState(false)
  const [passwordVisible, setPasswordVisible] = useState(false)
  const [pending, setPending] = useState<Pending>(null)
  const [actionError, setActionError] = useState<string | null>(null)
  const [toast, setToast] = useState<{ message: string; success: boolean } | null>(null)
  const [activeSection, setActiveSection] = useState<'overview' | 'connection' | 'logs'>('connection')
  const [updateState, setUpdateState] = useState<UpdateState>('checking')
  const [latestRelease, setLatestRelease] = useState<LatestRelease | null>(null)

  useEffect(() => {
    const systemTheme = window.matchMedia('(prefers-color-scheme: dark)')
    const applyTheme = () => {
      const theme = themePreference === 'system' ? (systemTheme.matches ? 'dark' : 'light') : themePreference
      document.documentElement.dataset.theme = theme
      document.documentElement.dataset.themeMode = themePreference
      document.documentElement.style.colorScheme = theme
    }
    applyTheme()
    systemTheme.addEventListener('change', applyTheme)
    try { window.localStorage.setItem('pk-theme-preference', themePreference) } catch { /* Browser storage may be disabled. */ }
    return () => systemTheme.removeEventListener('change', applyTheme)
  }, [themePreference])

  const refreshStatus = useCallback(async () => {
    try { setStatus(await getStatus()); setStatusError(false) }
    catch { setStatusError(true) }
  }, [])
  const refreshLogs = useCallback(async () => {
    try { setLogs(await getLogs()); setLogsError(false) }
    catch { setLogsError(true) }
    finally { setLogsLoading(false) }
  }, [])

  const checkForUpdates = useCallback(async (installedVersion: string) => {
    setUpdateState('checking')
    try {
      const release = await getLatestRelease()
      setLatestRelease(release)
      setUpdateState(isNewerVersion(release.version, installedVersion) ? 'available' : 'current')
    } catch { setUpdateState('error') }
  }, [])

  useEffect(() => {
    if (status?.version) void checkForUpdates(status.version)
  }, [status?.version, checkForUpdates])

  useEffect(() => {
    void getConfig().then((config) => {
      setSettings({ ...config, ssh_password: config.ssh_password ?? '', ssh_key_path: config.ssh_key_path ?? '' })
      setAuthMethod(config.ssh_key_path?.trim() ? 'key' : 'password')
      setRememberPassword(Boolean(config.ssh_password?.trim()))
      setConfigError(false)
    }).catch(() => setConfigError(true)).finally(() => setConfigLoading(false))
    void refreshStatus(); void refreshLogs()
    const statusTimer = window.setInterval(() => void refreshStatus(), 3000)
    const logsTimer = window.setInterval(() => void refreshLogs(), 4000)
    return () => { window.clearInterval(statusTimer); window.clearInterval(logsTimer) }
  }, [refreshStatus, refreshLogs])

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(null), 4000)
    return () => window.clearTimeout(timer)
  }, [toast])

  const updateSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((current) => ({ ...current, [key]: value }))
    setActionError(null)
  }
  const busy = pending !== null
  const connectionState: ConnectionState = statusError ? 'unavailable' : status === null ? 'loading' : status.ssh_alive ? 'connected' : 'disconnected'
  const connected = connectionState === 'connected'
  const canAutoConnect = authMethod === 'key' || rememberPassword
  const summary = { loading: '프록시 서비스의 상태를 확인하고 있습니다.', unavailable: '로컬 서비스가 응답하지 않습니다. 서비스가 실행 중인지 확인한 뒤 다시 시도하세요.', connected: '원격 SSH 터널이 작동 중입니다.', disconnected: '원격 터널이 연결되지 않았습니다. 설정을 확인한 뒤 연결하세요.' }[connectionState]
  const sshState: ServiceState = connectionState === 'loading' || connectionState === 'unavailable' ? 'unknown' : connected ? 'online' : 'offline'
  const httpState: ServiceState = connectionState === 'loading' || connectionState === 'unavailable' ? 'unknown'
    : !status?.http_alive ? 'offline' : connected ? 'online' : 'waiting'

  function configPayload(): Settings {
    return {
      ...settings,
      ssh_password: authMethod === 'password' && rememberPassword ? settings.ssh_password : null,
      ssh_key_path: authMethod === 'key' ? settings.ssh_key_path : null,
      auto_connect: canAutoConnect && settings.auto_connect,
    }
  }

  async function saveOnly() {
    if (busy || configLoading) return
    setPending('save'); setActionError(null)
    try {
      await saveConfig(configPayload())
      setToast({ message: connected ? '설정을 저장했습니다. 재연결하면 적용됩니다.' : '설정을 저장했습니다.', success: true })
    } catch (error) { setActionError(errorMessage(error, '설정을 저장하지 못했습니다.')) }
    finally { setPending(null) }
  }

  async function connect(event?: FormEvent<HTMLFormElement>) {
    event?.preventDefault()
    if (busy || configLoading) return
    setPending('connect'); setActionError(null)
    try {
      await saveConfig(configPayload())
      const transientPassword = authMethod === 'password' && !rememberPassword ? settings.ssh_password?.trim() || undefined : undefined
      await tunnelAction('connect', transientPassword)
      setToast({ message: connected ? '터널을 재연결했습니다.' : '프록시가 연결되었습니다.', success: true })
    } catch (error) { setActionError(errorMessage(error, '연결하지 못했습니다. 설정과 로그를 확인하세요.')) }
    finally { await Promise.all([refreshStatus(), refreshLogs()]); setPending(null) }
  }

  async function disconnect() {
    if (busy) return
    setPending('disconnect'); setActionError(null)
    try { await tunnelAction('disconnect'); setToast({ message: '연결을 해제했습니다.', success: true }) }
    catch (error) { setActionError(errorMessage(error, '연결을 해제하지 못했습니다.')) }
    finally { await Promise.all([refreshStatus(), refreshLogs()]); setPending(null) }
  }

  async function copyCommand(command: string) {
    try { await navigator.clipboard.writeText(command); setToast({ message: `${command} 명령을 복사했습니다.`, success: true }) }
    catch { setToast({ message: '명령을 복사하지 못했습니다.', success: false }) }
  }

  const visibleLogs = (showAllLogs ? logs : logs.slice(-3)).slice().reverse()

  return <main className="app-shell">
    <header className="site-header">
      <div className="brand">
        <img src={rocketMark} width="35" height="35" className="brand-icon" alt="" />
        <div className="brand-copy"><h1>PK Proxy Manager</h1><p>로컬 SSH 터널 및 HTTP-to-SOCKS5 프록시 관리자</p></div>
      </div>
      <nav className="header-links" aria-label="화면 도구">
        <div className="theme-picker">
          <SunIcon size={17} className="theme-icon-light" aria-hidden="true" />
          <MoonIcon size={17} className="theme-icon-dark" aria-hidden="true" />
          <label className="sr-only" htmlFor="theme-preference">테마</label>
          <select id="theme-preference" value={themePreference} onChange={(event) => setThemePreference(event.target.value as ThemePreference)}>
            <option value="system">시스템</option>
            <option value="light">라이트</option>
            <option value="dark">다크</option>
          </select>
          <CaretDownIcon size={13} className="theme-caret" aria-hidden="true" />
        </div>
        <a href="#connection-settings" onClick={() => setActiveSection('connection')}><GearSixIcon size={18} weight="fill" aria-hidden="true" />설정</a>
        <a href="#connection-guide"><InfoIcon size={18} weight="fill" aria-hidden="true" />연결 안내</a>
      </nav>
    </header>

    <nav className="sidebar" aria-label="주요 메뉴">
      <a href="#overview" className={activeSection === 'overview' ? 'active' : ''} aria-current={activeSection === 'overview' ? 'page' : undefined}
        onClick={() => setActiveSection('overview')}><InfoIcon size={20} aria-hidden="true" />개요</a>
      <a href="#connection-settings" className={activeSection === 'connection' ? 'active' : ''} aria-current={activeSection === 'connection' ? 'page' : undefined}
        onClick={() => setActiveSection('connection')}><TerminalWindowIcon size={20} aria-hidden="true" />연결</a>
      <a href="#recent-logs" className={activeSection === 'logs' ? 'active' : ''} aria-current={activeSection === 'logs' ? 'page' : undefined}
        onClick={() => { setActiveSection('logs'); setShowAllLogs(true) }}><FileTextIcon size={20} aria-hidden="true" />로그</a>
    </nav>

    <div className="workspace">
      {toast && <div className={'toast ' + (toast.success ? 'success' : 'error')} role="status">{toast.message}</div>}
      {status?.version && <div className={'update-strip ' + updateState} role="status">
        <span className="update-version">v{status.version}</span>
        <span className="update-description">{updateState === 'checking' ? '새 버전 확인 중…'
          : updateState === 'available' ? status.install_source === 'npm'
            ? `새 버전 v${latestRelease?.version} · pk stop 후 npm install -g @gomul82/pk@latest`
            : `새 버전 v${latestRelease?.version}을 사용할 수 있습니다.`
            : updateState === 'current' ? '최신 버전입니다.' : '새 버전을 확인할 수 없습니다.'}</span>
        {updateState === 'available' && latestRelease && <a className="update-link" href={latestRelease.url} target="_blank" rel="noopener noreferrer">
          {status.install_source === 'npm' ? '릴리스 정보' : '다운로드'}<ArrowRightIcon size={15} aria-hidden="true" /></a>}
        {updateState !== 'available' && <button type="button" className="update-check" disabled={updateState === 'checking'}
          onClick={() => void checkForUpdates(status.version)}><ArrowClockwiseIcon size={15} aria-hidden="true" />다시 확인</button>}
      </div>}
      <section className={'status-section ' + connectionState} id="overview" aria-labelledby="connection-headline">
        <h2 id="connection-headline">연결</h2>
        <div className="status-line">
          <span className={'status-dot ' + connectionState} aria-hidden="true" />
          <p>{summary}</p>
          <div className="status-actions">
            {connectionState === 'unavailable' || connectionState === 'loading' ?
              <button type="button" className="text-action" onClick={() => void refreshStatus()}><ArrowClockwiseIcon size={18} weight="bold" aria-hidden="true" />다시 확인</button> :
              <>
                <button type="submit" form="connection-form" className="button-primary" disabled={busy || configLoading}>
                  {connected ? <ArrowClockwiseIcon size={17} aria-hidden="true" /> : <RocketLaunchIcon size={17} aria-hidden="true" />}
                  {pending === 'connect' ? '연결 중...' : connected ? '재연결' : '연결하기'}
                </button>
                {connected && <button type="button" className="button-secondary" disabled={busy} onClick={() => void disconnect()}>
                  <SignOutIcon size={17} aria-hidden="true" />{pending === 'disconnect' ? '해제 중...' : '연결 해제'}</button>}
              </>}
          </div>
        </div>
        <div className="service-list" aria-label="서비스 상태">
          <ServiceRow icon={<TerminalWindowIcon size={20} weight="bold" />} name="원격 SSH 터널" state={sshState}
            detail={connected ? 'SOCKS5 포트 ' + status?.socks_port : sshState === 'unknown' ? '연결 상태를 확인하지 못했습니다.' : '원격 서버에 SSH로 연결합니다.'} />
          <ServiceRow icon={<GlobeIcon size={20} weight="bold" />} name="로컬 HTTP 프록시" state={httpState}
            detail={httpState === 'online' ? 'HTTP 포트 ' + status?.http_port : httpState === 'unknown' ? '서비스 상태를 확인하지 못했습니다.' : 'HTTP 요청을 SOCKS5로 전달합니다.'} />
        </div>
      </section>

      <section className="settings-panel" id="connection-settings" aria-labelledby="settings-title">
        <h2 id="settings-title" className="section-title">접속 설정</h2>
        {configError && !statusError && <p className="inline-note error" role="alert">설정을 불러오지 못했습니다. 서비스가 실행 중인지 확인하세요.</p>}
        {actionError && <p className="inline-note error" role="alert">{actionError}</p>}
        <form id="connection-form" onSubmit={(event) => void connect(event)}>
          <div className="form-field"><label htmlFor="ssh_target">원격 접속 대상 <span>(SSH User@Host)</span></label>
            <input id="ssh_target" name="ssh_target" type="text" autoComplete="off" required placeholder="user@example-host"
              value={settings.ssh_target} onChange={(event) => updateSetting('ssh_target', event.target.value)} />
            <p className="field-help">예: user@hostname 또는 user@123.45.67.89</p></div>
          <fieldset className="auth-methods"><legend>인증 방법</legend><div className="auth-choice-grid">
            <label className={'auth-choice ' + (authMethod === 'password' ? 'selected' : '')}>
              <input type="radio" name="auth_method" value="password" checked={authMethod === 'password'} onChange={() => setAuthMethod('password')} />
              <LockKeyIcon size={20} weight="fill" aria-hidden="true" /><span><strong>비밀번호</strong><small>계정 비밀번호로 인증합니다.</small></span>
            </label>
            <label className={'auth-choice ' + (authMethod === 'key' ? 'selected' : '')}>
              <input type="radio" name="auth_method" value="key" checked={authMethod === 'key'} onChange={() => setAuthMethod('key')} />
              <KeyIcon size={20} weight="fill" aria-hidden="true" /><span><strong>SSH 개인키</strong><small>개인키 파일로 인증합니다.</small></span>
            </label>
          </div></fieldset>
          {authMethod === 'password' ? <div className="form-field"><label htmlFor="ssh_password">비밀번호 <span>(Password)</span></label><div className="password-wrap">
            <input id="ssh_password" name="ssh_password" type={passwordVisible ? 'text' : 'password'} autoComplete="current-password" required placeholder="비밀번호를 입력하세요"
              value={settings.ssh_password ?? ''} onChange={(event) => updateSetting('ssh_password', event.target.value)} />
            <button type="button" className="icon-button" aria-label={passwordVisible ? '비밀번호 숨기기' : '비밀번호 보기'}
              onClick={() => setPasswordVisible((value) => !value)}>{passwordVisible ? <EyeSlashIcon size={19} /> : <EyeIcon size={19} />}</button>
          </div></div> : <div className="form-field"><label htmlFor="ssh_key_path">SSH 개인키 경로</label>
            <input id="ssh_key_path" name="ssh_key_path" type="text" required placeholder="~/.ssh/id_rsa"
              value={settings.ssh_key_path ?? ''} onChange={(event) => updateSetting('ssh_key_path', event.target.value)} /></div>}
          <div className="preference-grid">
            {authMethod === 'password' && <label className="preference"><input type="checkbox" checked={rememberPassword} onChange={(event) => {
              setRememberPassword(event.target.checked); if (!event.target.checked) updateSetting('auto_connect', false)
            }} /><span><strong>비밀번호 저장</strong><small>다음 접속 시 비밀번호를 자동으로 입력합니다.</small></span></label>}
            <label className="preference"><input type="checkbox" checked={canAutoConnect && settings.auto_connect} disabled={!canAutoConnect}
              onChange={(event) => updateSetting('auto_connect', event.target.checked)} />
              <span><strong>시작 시 자동 연결</strong><small>{canAutoConnect ? '앱 실행 시 자동으로 연결을 시도합니다.' : '비밀번호를 저장하면 사용할 수 있습니다.'}</small></span></label>
          </div>
          <details className="advanced-settings"><summary><GearSixIcon size={18} weight="fill" aria-hidden="true" />
            <span className="summary-label">고급 네트워크 설정 <small>(포트, NO_PROXY 등)</small></span><CaretDownIcon size={17} className="summary-caret" aria-hidden="true" /></summary>
            <div className="advanced-content"><div className="port-grid">
              <div className="form-field"><label htmlFor="http_port">로컬 HTTP 프록시 포트</label><input id="http_port" type="number" min="1024" max="65535" required value={settings.http_port} onChange={(event) => updateSetting('http_port', Number(event.target.value))} /></div>
              <div className="form-field"><label htmlFor="socks_port">로컬 SOCKS5 포트</label><input id="socks_port" type="number" min="1024" max="65535" required value={settings.socks_port} onChange={(event) => updateSetting('socks_port', Number(event.target.value))} /></div>
            </div><div className="form-field"><label htmlFor="no_proxy">프록시 제외 대상 <span>(NO_PROXY)</span></label><input id="no_proxy" type="text" value={settings.no_proxy} onChange={(event) => updateSetting('no_proxy', event.target.value)} /></div>
              <label className="preference browser-preference"><input type="checkbox" checked={settings.auto_open_browser} onChange={(event) => updateSetting('auto_open_browser', event.target.checked)} />
                <span><strong>미연결 상태에서 브라우저 자동 열기</strong><small>로그인 정보가 없을 때 설정 화면을 엽니다.</small></span></label>
            </div></details>
          <div className="form-actions"><button type="button" className="button-primary" disabled={busy || configLoading} onClick={() => void saveOnly()}>
            {pending === 'save' ? '저장 중...' : '설정 저장'}</button></div>
        </form>
      </section>
    </div>

    <aside className="inspector">
      <section className="guide-panel" id="connection-guide" aria-labelledby="guide-title">
        <h2 id="guide-title" className="inspector-title"><InfoIcon size={18} weight="fill" aria-hidden="true" />연결 안내</h2>
        <ol className="guide-steps">
          <li className={!connected ? 'current' : 'complete'}><span className="step-number">1</span><div><strong>설정 확인</strong><p>원격 SSH 접속 정보와 인증 방법을 입력하고 설정을 확인합니다.</p></div></li>
          <li className={connected ? 'complete' : ''}><span className="step-number">2</span><div><strong>연결 시작</strong><p>연결하기 버튼을 눌러 SSH 터널에 연결하고 로컬 포트를 구성합니다.</p></div></li>
          <li className={connected ? 'complete' : ''}><span className="step-number">3</span><div><strong>연결 완료</strong><p>원격 터널과 로컬 HTTP 프록시가 정상 작동하는지 확인합니다.</p></div></li>
        </ol>
      </section>
      <section className="logs-panel" id="recent-logs" aria-labelledby="logs-title">
        <div className="logs-heading"><h2 id="logs-title" className="inspector-title"><FileTextIcon size={18} weight="fill" aria-hidden="true" />최근 로그</h2>
          <button type="button" className="text-action" onClick={() => setShowAllLogs((value) => !value)}>
            {showAllLogs ? '간략히 보기' : '전체 보기'}<ArrowRightIcon size={16} aria-hidden="true" /></button></div>
        <div className={'logs-content ' + (showAllLogs ? 'expanded' : '')} role="log" aria-live="off">
          {logsLoading ? <p className="empty-log">로그를 불러오는 중입니다.</p> : logsError ? <p className="empty-log">로그를 불러올 수 없습니다. 서비스 상태를 확인하세요.</p>
            : visibleLogs.length ? visibleLogs.map((line, index) => {
              const entry = parseLog(line)
              return <div className="log-entry" key={index + '-' + line}>
                <time className="log-time">{entry.time}</time><span className={'log-level ' + entry.level}>{entry.level.toUpperCase()}</span><span className="log-message">{entry.message}</span>
              </div>
            }) : <p className="empty-log">아직 기록된 로그가 없습니다. 연결을 시작하면 여기에 표시됩니다.</p>}
        </div>
        {showAllLogs && <button type="button" className="text-action refresh-logs" onClick={() => void refreshLogs()}><ArrowClockwiseIcon size={16} />로그 새로고침</button>}
      </section>
    </aside>

    <section className="cli-section" aria-labelledby="cli-title"><div className="cli-intro"><h2 id="cli-title">CLI 바로가기</h2><p>터미널에서 프록시가 적용되는 명령어를 복사하세요.</p></div>
      <div className="cli-list">{['codex-proxy', 'grok-proxy', 'claude-proxy', 'agy-proxy', 'ocx-proxy'].map((name) =>
        <button type="button" key={name} className="cli-command" onClick={() => void copyCommand(name)} aria-label={name + ' 복사'}><code>{name}</code><CopyIcon size={15} aria-hidden="true" /></button>)}</div>
    </section>
  </main>
}
