/**
 * 与手机 Vibe Bridge（com.vibemonitor.bridge）的 BlueXlink 互联。
 * 参见 https://developers-watch.vivo.com.cn/api/connect/interconnect/
 */

const interconnect = require('@blueos.bluexlink.connectionManager')

const DEFAULT_PHONE_PACKAGE = 'com.vibemonitor.bridge'

let connect = null
let onMessageHandler = null

function readPhonePackage() {
  try {
    const info = require('@system.app').getInfo()
    const custom = (info && info.customData) || {}
    return custom.phonePackage || DEFAULT_PHONE_PACKAGE
  } catch (e) {
    return DEFAULT_PHONE_PACKAGE
  }
}

function readFingerprint() {
  try {
    const info = require('@system.app').getInfo()
    const custom = (info && info.customData) || {}
    return custom.phoneFingerprint || ''
  } catch (e) {
    return ''
  }
}

export function initBridge(onMessage) {
  onMessageHandler = onMessage
  const pkg = readPhonePackage()
  const fingerprint = readFingerprint()
  const opts = { package: pkg }
  if (fingerprint) opts.fingerprint = fingerprint

  connect = interconnect.instance(opts)
  connect.onmessage = function (data) {
    if (onMessageHandler) onMessageHandler(data)
  }
  return connect
}

export function getConnect() {
  return connect
}

export function sendToPhone(payload) {
  if (!connect) {
    console.warn('[bridge] interconnect not ready')
    return Promise.reject(new Error('not_connected'))
  }
  return new Promise((resolve, reject) => {
    connect.send({
      data: payload,
      success: function () {
        resolve()
      },
      fail: function (_data, code) {
        reject(new Error('send_failed_' + code))
      }
    })
  })
}

export function closeBridge() {
  if (!connect) return
  try {
    connect.close({
      success: function () {
        console.info('[bridge] closed')
      },
      fail: function (_d, code) {
        console.warn('[bridge] close fail', code)
      }
    })
  } catch (e) {
    console.warn('[bridge] close error', e)
  }
  connect = null
}
