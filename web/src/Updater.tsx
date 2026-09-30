import { useCallback, useEffect, useState } from 'react'
import { ArrowClockwiseIcon, ArrowRightIcon } from '@phosphor-icons/react'
import { getUpdateInfo, startNpmUpdate, type UpdateInfo } from './api'
import { getLatestRelease, isNewerVersion, type LatestRelease } from './update'

const interval = 30 * 60 * 1000
const pendingKey = 'pk-npm-update:' + window.location.origin

export default function Updater({ version }: { version: string }) {
  const [latest, setLatest] = useState<LatestRelease | null>(null)
  const [checking, setChecking] = useState(false)
  const [checkError, setCheckError] = useState(false)
  const [info, setInfo] = useState<UpdateInfo | null>(null)
  const [confirming, setConfirming] = useState(false)
  const [starting, setStarting] = useState(false)
  const [updating, setUpdating] = useState(() => { try { return Boolean(sessionStorage.getItem(pendingKey)) } catch { return false } })
  const [message, setMessage] = useState<string | null>(null)
  const [failed, setFailed] = useState(false)
  const available = latest && isNewerVersion(latest.version, version)
  const busy = starting || updating

  const check = useCallback(async () => {
    setChecking(true)
    const results = await Promise.allSettled([getLatestRelease(), getUpdateInfo()])
    if (results[0].status === 'fulfilled') { setLatest(results[0].value); setCheckError(false) }
    else setCheckError(true)
    if (results[1].status === 'fulfilled') {
      setInfo(results[1].value)
      const job = results[1].value.job
      if (job && !['complete', 'failed'].includes(job.phase)) setUpdating(true)
    }
    setChecking(false)
  }, [])

  useEffect(() => {
    void check()
    const timer = window.setInterval(() => void check(), interval)
    return () => window.clearInterval(timer)
  }, [check, version])

  useEffect(() => {
    if (!updating) return
    let cancelled = false
    const deadline = Date.now() + 330_000
    let timer: number | undefined
    const poll = async () => {
      try {
        const result = await getUpdateInfo()
        if (cancelled) return
        const job = result.job
        if (job && ['complete', 'failed'].includes(job.phase)) {
          setUpdating(false); setInfo(result); setMessage(job.message); setFailed(job.phase === 'failed')
          try { sessionStorage.removeItem(pendingKey) } catch { /* Storage is optional. */ }
          if (job.phase === 'complete') window.location.reload()
          return
        }
      } catch { /* The dashboard is offline while npm replaces PK. */ }
      if (cancelled) return
      if (Date.now() > deadline) {
        setUpdating(false); setFailed(true)
        setMessage('업데이트 결과를 확인하지 못했습니다. 터미널에서 pk ui를 실행하세요. 설치가 실패했다면 npm install -g @gomul82/pk@latest를 실행하세요.')
        try { sessionStorage.removeItem(pendingKey) } catch { /* Storage is optional. */ }
        return
      }
      timer = window.setTimeout(() => void poll(), 3000)
    }
    timer = window.setTimeout(() => void poll(), 3000)
    return () => { cancelled = true; window.clearTimeout(timer) }
  }, [updating])

  useEffect(() => {
    if (info?.job && ['complete', 'failed'].includes(info.job.phase) && !updating) {
      setMessage(info.job.message); setFailed(info.job.phase === 'failed')
    }
  }, [info, updating])

  const update = async () => {
    setConfirming(false); setStarting(true); setFailed(false); setMessage(null)
    try {
      const job = await startNpmUpdate()
      try { sessionStorage.setItem(pendingKey, job.id) } catch { /* Storage is optional. */ }
      setUpdating(true)
    } catch (error) {
      setUpdating(false); setFailed(true); setMessage(error instanceof Error ? error.message : '업데이트를 시작하지 못했습니다.')
    } finally { setStarting(false) }
  }

  return <div className={'update-strip ' + (available || busy ? 'available' : '')} role="status">
    <span className="update-version">v{version}</span>
    <span className="update-description">{busy ? 'npm으로 업데이트 중… 완료되면 이 화면이 자동으로 새로고침됩니다.'
      : available ? `새 버전 v${latest.version}을 사용할 수 있습니다.`
      : checking ? '새 버전 확인 중…' : checkError ? '새 버전을 확인할 수 없습니다.' : '최신 버전입니다.'}</span>
    {available && !busy && <button type="button" className="update-check" disabled={!info?.available} onClick={() => setConfirming(true)}>npm으로 업데이트<ArrowRightIcon size={15} aria-hidden="true" /></button>}
    <button type="button" className="update-check" disabled={checking || busy} onClick={() => void check()}><ArrowClockwiseIcon size={15} aria-hidden="true" />다시 확인</button>
    {available && !updating && info && !info.available && <p className="update-note">{info.reason} <code>pk stop</code> 후 <code>npm install -g @gomul82/pk@latest</code></p>}
    {confirming && <div className="update-confirm">
      <p>업데이트 중 프록시 연결이 잠시 끊깁니다. 설치 후 PK가 다시 시작되며, 필요하면 프록시를 다시 연결하세요.</p>
      <button type="button" className="button-primary" onClick={() => void update()}>업데이트 시작</button>
      <button type="button" className="button-secondary" onClick={() => setConfirming(false)}>취소</button>
    </div>}
    {message && !updating && <p className={'update-note' + (failed ? ' error' : '')}>{message}</p>}
  </div>
}
