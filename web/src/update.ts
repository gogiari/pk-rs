const releasesApi = 'https://api.github.com/repos/gogiari/pk-rs/releases/latest'
const releasesPage = 'https://github.com/gogiari/pk-rs/releases'

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
  const response = await fetch(releasesApi, {
    headers: { Accept: 'application/vnd.github+json' },
    signal: AbortSignal.timeout(8000),
  })
  if (!response.ok) throw new Error(`GitHub Releases 요청 실패 (${response.status})`)
  const data: unknown = await response.json()
  if (!data || typeof data !== 'object' || !('tag_name' in data) || typeof data.tag_name !== 'string' ||
      !versionParts(data.tag_name)) {
    throw new Error('릴리스 버전을 확인할 수 없습니다.')
  }
  return { version: data.tag_name.replace(/^v/, ''), url: releasesPage + '/tag/' + encodeURIComponent(data.tag_name) }
}
