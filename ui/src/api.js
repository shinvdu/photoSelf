export const API = '/api/v1'

// Shared fetch wrapper: sends the CSRF header required by the backend,
// encodes JSON bodies, and turns problem+json responses into Error messages.
export async function api(path, options = {}) {
  const isForm = options.body instanceof FormData
  const headers = {
    Accept: 'application/json',
    'X-Requested-With': 'photoSelf',
    ...(options.headers || {}),
  }
  if (options.body && !isForm) headers['Content-Type'] = 'application/json'
  const response = await fetch(path, { ...options, headers })
  if (!response.ok) {
    let message = response.statusText
    try {
      const problem = await response.json()
      message = problem.detail || problem.title || message
    } catch {
      // ignore non-JSON errors
    }
    throw new Error(message || `HTTP ${response.status}`)
  }
  if (response.status === 204) return null
  return response.json()
}