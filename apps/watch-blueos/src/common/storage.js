const storage = require('@system.storage')

const KEY_PENDING = 'vibe_pending_action'

export function savePending(actionEnvelope) {
  return new Promise((resolve, reject) => {
    storage.set({
      key: KEY_PENDING,
      value: JSON.stringify(actionEnvelope),
      success: resolve,
      fail: reject
    })
  })
}

export function loadPending() {
  return new Promise((resolve) => {
    storage.get({
      key: KEY_PENDING,
      success: function (data) {
        if (!data) {
          resolve(null)
          return
        }
        try {
          resolve(JSON.parse(data))
        } catch (e) {
          resolve(null)
        }
      },
      fail: function () {
        resolve(null)
      }
    })
  })
}

export function clearPending() {
  return new Promise((resolve) => {
    storage.delete({
      key: KEY_PENDING,
      success: resolve,
      fail: resolve
    })
  })
}
