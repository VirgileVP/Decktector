import type { SimCommand } from '@renderer/model/SimCommand'
import type { SimControlMessage } from '@renderer/model/SimControlMessage'

import { setConnectionStatus, setDeckTrack, setRigState } from '../state/sim-store'
import { publishDecks, publishTrack } from '../state/waveform-bridge'
import { controlSocketUrl, openSimSocket, type SimSocket } from './sim-socket'

let activeSocket: SimSocket | null = null

function applyMessage(message: SimControlMessage): void {
  if (message.type === 'state') {
    setRigState(message.state)
    publishDecks(message.state)
    return
  }
  setDeckTrack(message.deckId, message.track, message.audioUrl)
  publishTrack(message.deckId, message.track)
}

/** Connects the page's stores to the control server; returns the teardown. */
export function connectToSim(): () => void {
  const socket = openSimSocket(controlSocketUrl(window.location), {
    onMessage: applyMessage,
    onStatus: setConnectionStatus
  })
  activeSocket = socket
  return () => {
    socket.close()
    if (activeSocket === socket) activeSocket = null
  }
}

/** Sends a command to the simulator; dropped (and logged) while disconnected. */
export function sendCommand(command: SimCommand): void {
  if (!activeSocket?.send(command)) {
    console.warn('Command dropped, control socket is not open.', command)
  }
}
