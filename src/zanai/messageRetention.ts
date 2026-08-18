import type { DisplayMessage } from "../officeai/officeAiTypes";

export const MAX_RENDERED_ZANAI_MESSAGES = 100;

function isActionable(message: DisplayMessage): boolean {
  return Boolean(message.pendingAction || message.pendingBatchActions?.length);
}

export function boundLoadedMessages(
  messages: readonly DisplayMessage[],
  maxMessages = MAX_RENDERED_ZANAI_MESSAGES,
): DisplayMessage[] {
  const limit = Math.max(1, maxMessages);
  if (messages.length <= limit) return [...messages];

  const actionableIndexes = messages
    .map((message, index) => isActionable(message) ? index : -1)
    .filter(index => index >= 0)
    .slice(-limit);
  const keep = new Set(actionableIndexes);
  for (let index = messages.length - 1; index >= 0 && keep.size < limit; index -= 1) {
    keep.add(index);
  }

  return messages.filter((_, index) => keep.has(index));
}

export function appendBoundedMessages(
  current: readonly DisplayMessage[],
  additions: readonly DisplayMessage[],
  maxMessages = MAX_RENDERED_ZANAI_MESSAGES,
): DisplayMessage[] {
  return boundLoadedMessages([...current, ...additions], maxMessages);
}
