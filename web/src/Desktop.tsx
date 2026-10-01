import { useCallback, useEffect, useState } from 'react'
import { ArrowClockwiseIcon, DesktopIcon, RocketLaunchIcon } from '@phosphor-icons/react'
import { getDesktop, saveDesktop, launchDesktop, type DesktopInfo, type DesktopSettings, type DesktopEnvironment } from './api'

export default function Desktop({ connected }: { connected: boolean }) {
  const [info, setInfo] = useState<DesktopInfo | null>(null)
  const [settings, setSettings] = useState<DesktopSettings>({ environment: 'auto', executable: null, distribution: null })
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null)
  const refresh = useCallback(async () => {
    try { const result = await getDesktop(); setInfo(result); setSettings(result.settings); setMessage(null); return true }
    catch (error) { setMessage({ text: error instanceof Error ? error.message : '실행 환경을 확인하지 못했습니다.', error: true }); return false }
  }, [])
  useEffect(() => { void refresh() }, [refresh])
  const dirty = info !== null && JSON.stringify(settings) !== JSON.stringify(info.settings)
  function changeEnvironment(environment: DesktopEnvironment) {
    setSettings({ environment, executable: null, distribution: null }); setMessage(null)
  }
  async function save() {
    setBusy(true); setMessage(null)
    try { await saveDesktop(settings); if (await refresh()) setMessage({ text: '데스크톱 실행 환경을 저장했습니다.', error: false }) }
    catch (error) { setMessage({ text: error instanceof Error ? error.message : '저장에 실패했습니다.', error: true }) }
    finally { setBusy(false) }
  }
  async function launch() {
    setBusy(true); setMessage(null)
    try { await launchDesktop(); setMessage({ text: 'ChatGPT/Codex 데스크톱 앱을 실행했습니다.', error: false }) }
    catch (error) { setMessage({ text: error instanceof Error ? error.message : '실행에 실패했습니다.', error: true }) }
    finally { setBusy(false) }
  }
  return <section className="desktop-panel" aria-labelledby="desktop-title">
    <div className="browser-heading"><h2 id="desktop-title" className="section-title"><DesktopIcon size={19} aria-hidden="true" />ChatGPT / Codex 데스크톱</h2>
      <button type="button" className="text-action" disabled={busy} onClick={() => void refresh()}><ArrowClockwiseIcon size={16} aria-hidden="true" />다시 확인</button>
    </div>
    <p className="field-help">앱에만 PK 프록시를 적용합니다. PK 전용 앱 프로필에 로그인과 대화가 유지됩니다.</p>
    {info && <>
      <div className="desktop-targets"><div className="form-field"><label htmlFor="desktop-environment">실행 환경</label>
        <select id="desktop-environment" value={settings.environment} disabled={busy} onChange={event => changeEnvironment(event.target.value as DesktopEnvironment)}>
          <option value="auto">현재 OS (자동)</option>
          {info.environments.map(entry => <option key={entry.environment} value={entry.environment} disabled={!entry.supported}>{entry.label}{!entry.supported ? ' — 다른 OS 필요' : ''}</option>)}
        </select>
      </div>
      {settings.environment === 'wsl' && <div className="form-field"><label htmlFor="desktop-distribution">WSL 배포판</label>
        <select id="desktop-distribution" value={settings.distribution ?? ''} disabled={busy} onChange={event => { setSettings(current => ({ ...current, distribution: event.target.value || null })); setMessage(null) }}>
          <option value="">기본 배포판</option>{info.distributions.map(name => <option key={name} value={name}>{name}</option>)}
        </select>
      </div>}</div>
      <details className="browser-custom"><summary>앱 실행 위치 설정</summary>
        <div className="form-field"><label htmlFor="desktop-executable">실행 파일 경로 또는 명령 이름</label>
          <input id="desktop-executable" disabled={busy} value={settings.executable ?? ''} autoComplete="off" placeholder={settings.environment === 'wsl' || settings.environment === 'linux' ? '자동 검색 (chatgpt)' : '비워두면 설치된 앱을 검색합니다.'}
            onChange={event => { setSettings(current => ({ ...current, executable: event.target.value || null })); setMessage(null) }} />
          <p className="field-help">선택한 환경의 실행 파일을 지정하세요. WSL은 Linux 경로, macOS는 .app 내부 실행 파일을 사용합니다.</p>
        </div>
      </details>
      <div className="browser-default-actions">
        <button type="button" className="button-secondary" disabled={busy || !dirty} onClick={() => void save()}>실행 환경 저장</button>
        <button type="button" className="button-primary" disabled={busy || dirty || !connected} onClick={() => void launch()}><RocketLaunchIcon size={16} aria-hidden="true" />{busy ? '처리 중…' : '데스크톱 실행'}</button>
      </div>
      <p className="field-help"><code>codex-app-proxy</code>는 저장한 환경을 사용합니다. <code>codex-app-proxy wsl --distro Ubuntu</code>처럼 이번 실행만 바꿀 수 있습니다.</p>
      {dirty && <p className="field-help">실행 환경을 저장한 뒤 실행하세요.</p>}
    </>}
    {message && <p className={message.error ? 'inline-note error' : 'field-help'} role={message.error ? 'alert' : 'status'}>{message.text}</p>}
  </section>
}
