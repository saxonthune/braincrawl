import type { ContentBlock, ModelCall, ModelTurnResult } from "../types";

export interface ScriptedTurn {
  content: ContentBlock[];
  stopReason: ModelTurnResult["stopReason"];
  /** If given, passed to onText once, simulating a single streamed delta. */
  text?: string;
}

/** A deterministic ModelCall that yields the next scripted turn on each invocation. */
export function scriptedModelCall(turns: ScriptedTurn[]): ModelCall {
  let index = 0;
  return async ({ onText }) => {
    if (index >= turns.length) {
      throw new Error(
        `scriptedModelCall: invoked ${index + 1} times but only ${turns.length} turn(s) were scripted`,
      );
    }
    const turn = turns[index];
    index++;
    if (turn.text !== undefined) onText(turn.text);
    return { content: turn.content, stopReason: turn.stopReason };
  };
}
