/**
 * Pure Board lane rules shared by the Library and Project Boards.
 *
 * Kept free of React imports so `scripts/check-board-lanes.ts` can execute the
 * real derivation instead of a copy of it.
 */

/** Agent keys the Board treats as canonical lanes. */
export const CODEX_TOOL_KEY = "codex";
export const CLAUDE_TOOL_KEY = "claude_code";

export type BoardLane = "library" | "codex" | "claude" | "both";

/** Lane order as rendered, left to right. */
export const BOARD_LANES: BoardLane[] = ["library", "codex", "claude", "both"];

export interface CanonicalTargets {
  codex: boolean;
  claude: boolean;
}

export interface BoardCardModel {
  /** Stable Artifact identity within one Board context. */
  id: string;
  title: string;
  /** Display summary, clamped to two lines. `null` renders the localized unavailable value. */
  summary: string | null;
  artifactType: string;
  version: string | null;
  status: string | null;
  canonicalTargets: CanonicalTargets;
  /** Agent keys outside Codex and Claude; preserved through every lane change. */
  otherTargets: string[];
}

export interface ProjectBoardVariantState {
  agent: string;
  enabled: boolean;
  relativePath: string;
}

export type ProjectBoardMutation =
  | { kind: "toggle"; agent: string; relativePath: string; enabled: boolean }
  | { kind: "export"; agent: string };

/** Maps Codex/Claude membership onto exactly one lane. */
export function laneForTargets(targets: CanonicalTargets): BoardLane {
  if (targets.codex && targets.claude) return "both";
  if (targets.codex) return "codex";
  if (targets.claude) return "claude";
  return "library";
}

/** The exact Codex/Claude combination a lane represents. */
export function targetsForLane(lane: BoardLane): CanonicalTargets {
  switch (lane) {
    case "codex":
      return { codex: true, claude: false };
    case "claude":
      return { codex: false, claude: true };
    case "both":
      return { codex: true, claude: true };
    case "library":
      return { codex: false, claude: false };
  }
}

/** Derives canonical membership from a list of agent keys. */
export function canonicalTargetsFrom(toolKeys: string[]): CanonicalTargets {
  return {
    codex: toolKeys.includes(CODEX_TOOL_KEY),
    claude: toolKeys.includes(CLAUDE_TOOL_KEY),
  };
}

/** Agent keys that the Board never touches. */
export function otherTargetsFrom(toolKeys: string[]): string[] {
  return toolKeys.filter((key) => key !== CODEX_TOOL_KEY && key !== CLAUDE_TOOL_KEY);
}

/**
 * Plans the file operations for a Project Board lane change. Existing variants
 * are enabled or disabled in place so an Artifact remains visible in Library;
 * only a missing desired variant needs to be exported.
 */
export function planProjectBoardMutations(
  variants: ProjectBoardVariantState[],
  lane: BoardLane,
): ProjectBoardMutation[] {
  const desired = targetsForLane(lane);
  const canonical: Array<[string, boolean]> = [
    [CODEX_TOOL_KEY, desired.codex],
    [CLAUDE_TOOL_KEY, desired.claude],
  ];
  const exports: ProjectBoardMutation[] = [];
  const toggles: ProjectBoardMutation[] = [];

  for (const [agent, shouldEnable] of canonical) {
    const variant = variants.find((item) => item.agent === agent);
    if (!variant) {
      if (shouldEnable) exports.push({ kind: "export", agent });
      continue;
    }
    if (variant.enabled === shouldEnable) continue;
    toggles.push({
      kind: "toggle",
      agent,
      relativePath: variant.relativePath,
      enabled: shouldEnable,
    });
  }

  return [...exports, ...toggles];
}

/** Runs target mutations in order and reverses completed steps on failure. */
export async function runBoardMutationSequence<T>(
  mutations: T[],
  apply: (mutation: T) => Promise<void>,
  rollback: (mutation: T) => Promise<void>,
): Promise<void> {
  const completed: T[] = [];
  try {
    for (const mutation of mutations) {
      await apply(mutation);
      completed.push(mutation);
    }
  } catch (error) {
    for (const mutation of completed.reverse()) {
      try {
        await rollback(mutation);
      } catch {
        // The page owner refreshes server state even when rollback also fails.
      }
    }
    throw error;
  }
}

/**
 * Keeps the first card for each identity. A Board context renders one card per
 * Artifact, so upstream data that repeats an id must not produce two cards.
 */
export function dedupeCards(cards: BoardCardModel[]): BoardCardModel[] {
  const seen = new Set<string>();
  const out: BoardCardModel[] = [];
  for (const card of cards) {
    if (seen.has(card.id)) continue;
    seen.add(card.id);
    out.push(card);
  }
  return out;
}

/** Groups cards by lane, preserving input order inside each lane. */
export function groupCardsByLane(cards: BoardCardModel[]): Record<BoardLane, BoardCardModel[]> {
  const grouped: Record<BoardLane, BoardCardModel[]> = {
    library: [],
    codex: [],
    claude: [],
    both: [],
  };
  for (const card of dedupeCards(cards)) {
    grouped[laneForTargets(card.canonicalTargets)].push(card);
  }
  return grouped;
}
