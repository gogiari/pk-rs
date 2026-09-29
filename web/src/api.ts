export interface Settings {
  ssh_target: string
  ssh_password: string | null
  ssh_key_path: string | null
  http_port: number
  socks_port: number
  no_proxy: string
  auto_connect: boolean
  auto_open_browser: boolean
}

export interface Status {
  version: string
  install_source: 'npm' | 'native'
  ssh_alive: boolean
  http_alive: boolean
  ssh_target: string
  http_port: number
  socks_port: number
  has_password: boolean
}

async function checkResponse(response: Response): Promise<Response> {
  if (response.ok) return response

  const body = await response.text()
  let message = body
  try {
    const parsed = JSON.parse(body) as { error?: string; message?: string }
    message = parsed.error ?? parsed.message ?? body
  } catch {
    // Axum errors are often plain text.
  }
  throw new Error(message || `요청 실패 (${response.status})`)
}

async function getJson<T>(path: string): Promise<T> {
  const response = await checkResponse(await fetch(path))
  return response.json() as Promise<T>
}

export const getConfig = () => getJson<Settings>('/api/config')
export const getStatus = () => getJson<Status>('/api/status')
export const getLogs = () => getJson<string[]>('/api/logs')

export async function saveConfig(settings: Settings): Promise<void> {
  await checkResponse(
    await fetch('/api/config', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(settings),
    }),
  )
}

export async function tunnelAction(action: 'connect' | 'disconnect' | 'restart', temporaryPassword?: string): Promise<void> {
  await checkResponse(await fetch(`/api/tunnel/${action}`, {
    method: 'POST',
    ...(action === 'connect' ? {
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ password: temporaryPassword ?? null }),
    } : {}),
  }))
}
