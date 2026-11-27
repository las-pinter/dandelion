import { useState } from 'react'
import { isLiveSample, type Snapshot } from '@/lib/metrics'
import { useHubTopics } from './use-hub-topics'

const TOPIC = 'system.metrics'

export interface LiveSampleState {
  /** The newest snapshot from the live stream. */
  snapshot: Snapshot | null
  /** Whether the hub socket is open. */
  connected: boolean
}

/**
 * The newest metrics snapshot alone: the live topic, no history.
 *
 * The dashboard needs the history windows and fetches the latest sample; the
 * shell's host card needs a few numbers, so it rides the same shared socket
 * and fills in with the next sample (the collector publishes every second).
 */
export function useLiveSample(): LiveSampleState {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null)

  const connected = useHubTopics([TOPIC], {
    onEvent: (topic, data) => {
      if (topic !== TOPIC || !isLiveSample(data)) return
      setSnapshot((previous) =>
        !previous || data.snapshot.timestamp_ms >= previous.timestamp_ms ? data.snapshot : previous,
      )
    },
  })

  return { snapshot, connected }
}
