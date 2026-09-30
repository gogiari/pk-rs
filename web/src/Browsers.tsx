import { useCallback, useEffect, useState } from 'react'
import { ArrowClockwiseIcon, GlobeIcon } from '@phosphor-icons/react'
import { getBrowsers, getBrowserDefault, launchBrowser, launchBrowserDefault, saveBrowser, saveBrowserDefault, type BrowserInfo, type BrowserKind, type BrowserLauncher, type DefaultBrowserInfo } from './api'

function launcherValue(launcher: BrowserLauncher | null) {
  if (!launcher) return ''
  return launcher.mode === 'executable' ? launcher.path : launcher.mode === 'flatpak' ? launcher.app_id : launcher.name
}

function BrowserRow({ browser, connected, refresh, wsl }: { browser: BrowserInfo; connected: boolean; refresh: () => Promise<void>; wsl: boolean }) {
  const savedMode = browser.saved?.mode ?? 'auto'
  const savedValue = launcherValue(browser.saved)
  const [mode, setMode] = useState<'auto' | BrowserLauncher['mode']>(savedMode)
  const [value, setValue] = useState(savedValue)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null)
  useEffect(() => {
    setMode(savedMode)
    setValue(savedValue)
  }, [savedMode, savedValue])
  const dirty = mode !== (browser.saved?.mode ?? 'auto') || (mode !== 'auto' && value !== launcherValue(browser.saved))
  async function save() {
    setBusy(true); setMessage(null)
    try {
      const trimmed = value.trim()
      if (mode !== 'auto' && !trimmed) throw new Error('실행 위치나 앱 이름을 입력하세요.')
      const launcher: BrowserLauncher | null = mode === 'auto' ? null : mode === 'executable' ? { mode, path: trimmed }
        : mode === 'flatpak' ? { mode, app_id: trimmed } : { mode, name: trimmed }
      await saveBrowser(browser.kind, launcher)
      await refresh()
      setMessage({ text: '실행 위치를 저장했습니다.', error: false })
    } catch (error) { setMessage({ text: error instanceof Error ? error.message : '저장에 실패했습니다.', error: true }) }
    finally { setBusy(false) }
  }
  async function launch() {
    setBusy(true); setMessage(null)
    try {
      await launchBrowser(browser.kind)
      setMessage({ text: `${browser.label} 프록시 브라우저를 실행했습니다.`, error: false })
    } catch (error) { setMessage({ text: error instanceof Error ? error.message : '실행에 실패했습니다.', error: true }) }
    finally { setBusy(false) }
  }
  const id = browser.kind
  return <div className={`browser-row${id === 'safari' ? ' safari-row' : ''}`}>
    <div className="browser-row-heading"><strong>{browser.label}{id === 'safari' ? ' (macOS)' : ''}</strong>
      <button type="button" className="button-secondary" disabled={busy || !connected || !browser.available || !browser.supported || (browser.system_proxy !== null && !browser.system_proxy.ready) || dirty} onClick={() => void launch()}>
        <GlobeIcon size={16} aria-hidden="true" />{busy ? '처리 중…' : '실행'}
      </button>
    </div>
    <p className="browser-location">{browser.available ? launcherValue(browser.saved ?? browser.detected) : browser.error}</p>
    {browser.supported && browser.system_proxy && <div className="browser-system-proxy">
      <p className="field-help" role="status">{browser.system_proxy.message}</p>
      <a href="https://support.apple.com/ko-kr/guide/safari/ibrw1053/mac" target="_blank" rel="noopener noreferrer">Safari 프록시 설정 안내</a>
      <p className="field-help">설정을 바꾼 뒤 위의 다시 검색 버튼으로 확인하세요. Safari의 프록시 제외 대상은 macOS에서 관리합니다.</p>
    </div>}
    <code className="browser-cli-command">pk browser {id}</code>
    {browser.supported && <details className="browser-custom"><summary>실행 위치 설정</summary>
      <div className="form-field"><label htmlFor={`browser-mode-${id}`}>실행 방식</label>
        <select id={`browser-mode-${id}`} disabled={busy} value={mode} onChange={(event) => { setMode(event.target.value as typeof mode); setMessage(null) }}>
          <option value="auto">자동 검색</option><option value="executable">실행 파일 직접 지정</option>
          {id !== 'safari' && <><option value="flatpak">Flatpak (Linux)</option><option value="snap">Snap (Linux)</option></>}
        </select>
      </div>
      {mode !== 'auto' && <div className="form-field"><label htmlFor={`browser-path-${id}`}>{mode === 'executable' ? '실행 파일 경로 또는 명령 이름' : mode === 'flatpak' ? 'Flatpak 앱 ID' : 'Snap 이름'}</label>
        <input id={`browser-path-${id}`} type="text" disabled={busy} autoComplete="off" value={value} onChange={(event) => { setValue(event.target.value); setMessage(null) }}
          placeholder={mode === 'executable' ? '예: D:\\Apps\\Browser\\browser.exe' : mode === 'flatpak' ? '예: org.mozilla.firefox' : '예: firefox'} />
        <p className="field-help">{mode === 'executable' ? wsl ? 'Windows 경로(C:\\Apps\\browser.exe)나 WSL 경로(/mnt/c/Apps/browser.exe)를 입력하세요. 경로에 따옴표나 실행 옵션을 붙이지 마세요.' : '경로에 따옴표나 실행 옵션을 붙이지 마세요. macOS에서는 .app 내부 실행 파일을 지정하세요.' : '설치된 패키지의 이름만 입력하세요.'}</p>
      </div>}
      <button type="button" className="button-secondary" disabled={busy} onClick={() => void save()}>실행 위치 저장</button>
    </details>}
    {dirty && <p className="field-help">실행 위치를 저장한 뒤 실행하세요.</p>}
    {message && <p className={message.error ? 'inline-note error' : 'field-help'} role={message.error ? 'alert' : 'status'}>{message.text}</p>}
  </div>
}

function DefaultBrowser({ info, browsers, connected, refresh }: { info: DefaultBrowserInfo; browsers: BrowserInfo[]; connected: boolean; refresh: () => Promise<void> }) {
  const saved = info.preferred ?? 'system'
  const [selected, setSelected] = useState<BrowserKind | 'system'>(saved)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null)
  useEffect(() => { setSelected(saved) }, [saved])
  const dirty = selected !== saved
  const label = (kind: BrowserKind | null) => browsers.find(browser => browser.kind === kind)?.label ?? '확인 불가'
  const effective = browsers.find(browser => browser.kind === info.effective)
  async function save() {
    setBusy(true); setMessage(null)
    try {
      await saveBrowserDefault(selected === 'system' ? null : selected)
      await refresh()
      setMessage({ text: '기본 프록시 브라우저를 저장했습니다.', error: false })
    } catch (error) { setMessage({ text: error instanceof Error ? error.message : '저장에 실패했습니다.', error: true }) }
    finally { setBusy(false) }
  }
  async function launch() {
    setBusy(true); setMessage(null)
    try {
      const result = await launchBrowserDefault()
      setMessage({ text: `${label(result.kind)} 프록시 브라우저를 실행했습니다.`, error: false })
    } catch (error) { setMessage({ text: error instanceof Error ? error.message : '실행에 실패했습니다.', error: true }) }
    finally { setBusy(false) }
  }
  return <div className="browser-default">
    <div className="form-field"><label htmlFor="browser-default">기본 프록시 브라우저</label>
      <select id="browser-default" value={selected} disabled={busy} onChange={event => { setSelected(event.target.value as typeof selected); setMessage(null) }}>
        <option value="system">{info.wsl ? 'Windows 기본 브라우저' : 'OS 기본 브라우저'}{info.system ? ` (${label(info.system)})` : ''}</option>
        {browsers.map(browser => <option key={browser.kind} value={browser.kind} disabled={!browser.supported}>{browser.label}{browser.kind === 'safari' ? ' (macOS)' : ''}</option>)}
      </select>
    </div>
    <div className="browser-default-actions">
      <button type="button" className="button-secondary" disabled={busy || !dirty} onClick={() => void save()}>기본 브라우저 저장</button>
      <button type="button" className="button-secondary" disabled={busy || dirty || !connected || !info.effective || effective?.system_proxy?.ready === false} onClick={() => void launch()}><GlobeIcon size={16} aria-hidden="true" />{busy ? '처리 중…' : '기본 브라우저 실행'}</button>
    </div>
    <p className="field-help"><code>pk browser</code> 실행 시 {info.preferred ? `저장된 ${label(info.preferred)}` : 'OS 기본 브라우저'}를 사용합니다. OS 기본 브라우저 설정은 바뀌지 않습니다.</p>
    {info.wsl && <p className="field-help">WSL에서는 Windows 브라우저를 자동 검색합니다. Windows에 PK를 설치할 필요 없이 WSL의 프록시를 사용하며, Windows 브라우저 프로필은 Windows 드라이브에 저장됩니다.</p>}
    {dirty && <p className="field-help">선택을 저장하면 웹과 CLI에 함께 적용됩니다.</p>}
    {info.error && <p className="inline-note error" role="alert">{info.error}</p>}
    {message && <p className={message.error ? 'inline-note error' : 'field-help'} role={message.error ? 'alert' : 'status'}>{message.text}</p>}
  </div>
}

export default function Browsers({ connected }: { connected: boolean }) {
  const [browsers, setBrowsers] = useState<BrowserInfo[]>([])
  const [defaultInfo, setDefaultInfo] = useState<DefaultBrowserInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const refresh = useCallback(async () => {
    try {
      const [entries, info] = await Promise.all([getBrowsers(), getBrowserDefault()])
      setBrowsers(entries); setDefaultInfo(info); setError(null)
    }
    catch (error) { setError(error instanceof Error ? error.message : '브라우저 목록을 불러오지 못했습니다.') }
    finally { setLoading(false) }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  return <section className="browser-panel" aria-labelledby="browser-title">
    <div className="browser-heading"><h2 id="browser-title" className="section-title"><GlobeIcon size={19} aria-hidden="true" />프록시 브라우저</h2>
      <button type="button" className="text-action" disabled={loading} onClick={() => { setLoading(true); void refresh() }}><ArrowClockwiseIcon size={16} aria-hidden="true" />다시 검색</button>
    </div>
    <p className="field-help">Chrome·Edge·Firefox는 PK 전용 프로필에 로그인과 쿠키가 유지됩니다. 실행 위치나 프록시 설정을 바꾸면 기존 PK 브라우저 창을 모두 닫고 다시 실행하세요.</p>
    {!connected && <p className="field-help">SSH 터널을 연결하면 브라우저를 실행할 수 있습니다.</p>}
    {error && <p className="inline-note error" role="alert">{error}</p>}
    {loading && <p className="field-help" role="status">브라우저 실행 위치를 확인하고 있습니다…</p>}
    {defaultInfo && <DefaultBrowser info={defaultInfo} browsers={browsers} connected={connected} refresh={refresh} />}
    <div className="browser-grid">{browsers.map((browser) => <BrowserRow key={browser.kind} browser={browser} connected={connected} refresh={refresh} wsl={defaultInfo?.wsl ?? false} />)}</div>
  </section>
}
