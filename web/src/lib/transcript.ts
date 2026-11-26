/**
 * Transcript socket types and helpers.
 *
 * The server streams Claude Code's own transcript; nothing here is stored.
 * Records arrive oldest first, appended as the session runs.
 */

/** One displayable part of a record. */
export type TranscriptBlock =
  | { kind: 'text'; text: string }
  | { kind: 'thinking'; text: string }
  | { kind: 'tool_use'; id: string; name: string; summary: string; input: string }
  | { kind: 'tool_result'; toolUseId?: string | null; isError: boolean; text: string }
  | { kind: 'divider'; label: string }

/** One record from the transcript. */
export interface TranscriptRecord {
  /** Byte offset of the line in the file; stable across pages. */
  offset?: number
  /** Who wrote the record. */
  role: 'user' | 'assistant' | 'system'
  at?: string | null
  blocks: TranscriptBlock[]
}

/** One subagent run of a session. */
export interface SubagentInfo {
  agent: string
  toolUseId?: string | null
  description?: string | null
}

/** Frames the transcript socket sends. */
export type TranscriptFrame =
  | { type: 'page'; records: TranscriptRecord[]; start: number; hasEarlier: boolean }
  | { type: 'appended'; records: TranscriptRecord[] }
  | { type: 'earlier'; records: TranscriptRecord[]; start: number; hasEarlier: boolean }
  | { type: 'subagents'; items: SubagentInfo[] }
  | { type: 'error'; message: string }

/** The socket URL for a session, or one of its subagents. */
export function transcriptSocketUrl(sessionId: string, agent?: string | null): string {
  const protocol = window.location.protocol === 'https:' ? 'wss' : 'ws'
  const base = `${protocol}://${window.location.host}/api/claude/${encodeURIComponent(sessionId)}/transcript/ws`
  return agent ? `${base}?agent=${encodeURIComponent(agent)}` : base
}

/** Parses one socket text frame; null for anything malformed. */
export function parseFrame(data: unknown): TranscriptFrame | null {
  if (typeof data !== 'string') return null
  try {
    const value = JSON.parse(data) as Partial<TranscriptFrame> & Record<string, unknown>
    if (typeof value.type !== 'string') return null
    if (value.type === 'page' || value.type === 'appended' || value.type === 'earlier') {
      return Array.isArray(value.records) ? (value as TranscriptFrame) : null
    }
    if (value.type === 'subagents') {
      return Array.isArray(value.items) ? (value as TranscriptFrame) : null
    }
    if (value.type === 'error' && typeof value.message === 'string') {
      return value as TranscriptFrame
    }
    return null
  } catch {
    return null
  }
}

/** Prepends an earlier page, so the list stays oldest first. */
export function withEarlier(
  records: TranscriptRecord[],
  earlier: TranscriptRecord[],
): TranscriptRecord[] {
  return [...earlier, ...records]
}

/** Appends newly streamed records. */
export function withAppended(
  records: TranscriptRecord[],
  appended: TranscriptRecord[],
): TranscriptRecord[] {
  return appended.length === 0 ? records : [...records, ...appended]
}

/** The subagent run a tool call spawned, when one is known. */
export function subagentFor(items: SubagentInfo[], toolUseId: string): SubagentInfo | undefined {
  return items.find((item) => item.toolUseId === toolUseId)
}

/** Only https images may load, so a transcript cannot ping a plain host. */
export function safeImageSource(source: unknown): string | null {
  if (typeof source !== 'string') return null
  return source.startsWith('https://') ? source : null
}

/** One rendered turn: consecutive records from a single role. */
export interface TranscriptGroup {
  key: number
  role: TranscriptRecord['role']
  /** The first record's time, and the last one's when it differs. */
  at?: string
  end?: string
  records: TranscriptRecord[]
}

/** Whether a block has its own row: a tool result waits inside its call. */
export function isShown(block: TranscriptBlock, callIds: Set<string>): boolean {
  return !(block.kind === 'tool_result' && block.toolUseId && callIds.has(block.toolUseId))
}

/** The call ids and the tool results keyed by the call they answer. */
export function derive(records: TranscriptRecord[]) {
  const callIds = new Set<string>()
  const results = new Map<string, Extract<TranscriptBlock, { kind: 'tool_result' }>>()
  for (const record of records) {
    for (const block of record.blocks) {
      if (block.kind === 'tool_use') callIds.add(block.id)
      if (block.kind === 'tool_result' && block.toolUseId) {
        results.set(block.toolUseId, block)
      }
    }
  }
  return { callIds, results }
}

/** Consecutive records from one role read as one message and share a header. */
export function groupRecords(records: TranscriptRecord[], callIds: Set<string>): TranscriptGroup[] {
  const groups: TranscriptGroup[] = []
  for (const record of records) {
    // A record whose blocks are all hidden (a tool result inside its call)
    // would leave an empty bubble, so it never starts a group.
    if (!record.blocks.some((block) => isShown(block, callIds))) continue
    const last = groups[groups.length - 1]
    if (last && last.role === record.role) {
      last.records.push(record)
      if (record.at) last.end = record.at
    } else {
      groups.push({
        key: record.offset ?? groups.length,
        role: record.role,
        at: record.at ?? undefined,
        records: [record],
      })
    }
  }
  return groups
}

/** Short local time from an ISO timestamp, or the raw value when unparsable. */
export function formatTime(at: string): string {
  const date = new Date(at)
  if (Number.isNaN(date.getTime())) return at
  return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}
