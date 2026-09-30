const packageApi = 'https://registry.npmjs.org/@gomul82%2fpk/latest'
const packagePage = 'https://www.npmjs.com/package/@gomul82/pk'

export interface LatestRelease {
  version: string
  url: string
}

function versionParts(version: string): number[] | null {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$/.exec(version)
  return match ? match.slice(1, 4).map(Number) : null
}

export function isNewerVersion(latest: string, installed: string): boolean {
  const next = versionParts(latest)
  const current = versionParts(installed)
  if (!next || !current) return false
  for (let index = 0; index < 3; index += 1) {
    if (next[index] !== current[index]) return next[index] > current[index]
  }
  return installed.includes('-') && !latest.includes('-')
}

export async function getLatestRelease(): Promise<LatestRelease> {
  const response = await fetch(packageApi, {
    cache: 'no-store',
    headers: { Accept: 'application/json' },
    signal: AbortSignal.timeout(8000),
  })
  if (!response.ok) throw new Error(`npm 버전 요청 실패 (${response.status})`)
  const data: unknown = await response.json()
  if (!data || typeof data !== 'object' || !('version' in data) || typeof data.version !== 'string' ||
      !versionParts(data.version)) {
    throw new Error('npm 패키지 버전을 확인할 수 없습니다.')
  }
  return { version: data.version, url: packagePage }
}
