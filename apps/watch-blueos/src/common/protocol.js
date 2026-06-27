/** 与 vibe-protocol / Vibe Bridge 对齐的消息工具 */

export const ENVELOPE_ACTION_PROMPT = 'action_prompt'
export const ENVELOPE_ACTION_RESPONSE = 'action_response'
export const ENVELOPE_ACTION_CANCELLED = 'action_cancelled'

export function parseMessage(raw) {
  if (!raw) return null
  if (typeof raw === 'string') {
    try {
      return JSON.parse(raw)
    } catch (e) {
      return null
    }
  }
  if (typeof raw === 'object') {
    if (raw.type) return raw
    if (raw.data && raw.data.type) return raw.data
    if (raw.payload && raw.payload.type) return raw.payload
  }
  return null
}

export function isActionPrompt(msg) {
  return msg && msg.type === ENVELOPE_ACTION_PROMPT
}

export function isActionCancelled(msg) {
  return msg && msg.type === ENVELOPE_ACTION_CANCELLED
}

export function buildActionResponse(actionId, choice) {
  return {
    type: ENVELOPE_ACTION_RESPONSE,
    id: actionId,
    ts: Math.floor(Date.now() / 1000),
    data: {
      action_id: actionId,
      choice: choice,
      from: 'watch'
    }
  }
}

export function pickButtons(actionData) {
  const actions = (actionData && actionData.actions) || []
  if (actions.length >= 2) {
    return { approve: actions[0], deny: actions[1] }
  }
  return {
    approve: { id: 'approve', label: '允许' },
    deny: { id: 'deny', label: '拒绝' }
  }
}

export function isExpired(expiresAt) {
  if (!expiresAt) return false
  const ts = Date.parse(expiresAt)
  if (Number.isNaN(ts)) return false
  return Date.now() > ts
}
