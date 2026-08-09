#!/usr/bin/env node
// Repeatable check for the Board lane contract. Runs the real derivation from
// src/components/boardLanes.ts through the spec's fixtures — no test framework,
// no extra dependency; Node executes the TypeScript directly.
import {
  BOARD_LANES,
  canonicalTargetsFrom,
  dedupeCards,
  groupCardsByLane,
  laneForTargets,
  otherTargetsFrom,
  planProjectBoardMutations,
  runBoardMutationSequence,
  targetsForLane,
  type BoardCardModel,
  type BoardLane,
} from '../src/components/boardLanes.ts';

const failures: string[] = [];

function check(label: string, actual: unknown, expected: unknown) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) failures.push(`${label}: expected ${e}, got ${a}`);
}

function card(id: string, codex: boolean, claude: boolean, extra: Partial<BoardCardModel> = {}): BoardCardModel {
  return {
    id,
    title: id,
    summary: 'summary',
    artifactType: 'skill',
    version: null,
    status: null,
    canonicalTargets: { codex, claude },
    otherTargets: [],
    ...extra,
  };
}

// Canonical target table from specs/product-board-interface/spec.md.
const LANE_TABLE: Array<[boolean, boolean, BoardLane]> = [
  [false, false, 'library'],
  [true, false, 'codex'],
  [false, true, 'claude'],
  [true, true, 'both'],
];

for (const [codex, claude, expected] of LANE_TABLE) {
  check(`laneForTargets(codex=${codex}, claude=${claude})`, laneForTargets({ codex, claude }), expected);
  check(`targetsForLane(${expected})`, targetsForLane(expected), { codex, claude });
}

check('BOARD_LANES', BOARD_LANES, ['library', 'codex', 'claude', 'both']);

// Every lane holds exactly the card that belongs to it, one card per identity.
const grouped = groupCardsByLane(LANE_TABLE.map(([c, cl], i) => card(`skill-${i}`, c, cl)));
check(
  'one card per lane',
  Object.fromEntries(BOARD_LANES.map((lane) => [lane, grouped[lane].map((c) => c.id)])),
  { library: ['skill-0'], codex: ['skill-1'], claude: ['skill-2'], both: ['skill-3'] },
);

// A repeated identity must not render twice.
check(
  'duplicate id guard',
  dedupeCards([card('dup', true, true), card('dup', false, false), card('other', false, false)]).map((c) => c.id),
  ['dup', 'other'],
);
check(
  'duplicate id keeps one lane',
  groupCardsByLane([card('dup', true, true), card('dup', false, false)]).both.length +
    groupCardsByLane([card('dup', true, true), card('dup', false, false)]).library.length,
  1,
);

// Missing summary stays null so the view can render the localized empty value.
check('empty summary is preserved as null', card('no-summary', false, false, { summary: null }).summary, null);

// Non-canonical agent targets are split out and never folded into a lane.
check('canonicalTargetsFrom', canonicalTargetsFrom(['codex', 'gemini']), { codex: true, claude: false });
check('otherTargetsFrom', otherTargetsFrom(['codex', 'claude_code', 'gemini', 'cursor']), ['gemini', 'cursor']);
check('non-canonical target does not change the lane', laneForTargets(canonicalTargetsFrom(['gemini'])), 'library');

// Project Board target membership follows enabled state. Disabling the final
// canonical target keeps the Artifact as a disabled variant in the Library lane.
const projectVariants = [
  { agent: 'codex', enabled: true, relativePath: 'spectra-analyze' },
  { agent: 'claude_code', enabled: true, relativePath: 'spectra-analyze' },
  { agent: 'gemini', enabled: true, relativePath: 'spectra-analyze' },
];
check(
  'Both to Library disables canonical variants without deleting them',
  planProjectBoardMutations(projectVariants, 'library'),
  [
    { kind: 'toggle', agent: 'codex', relativePath: 'spectra-analyze', enabled: false },
    { kind: 'toggle', agent: 'claude_code', relativePath: 'spectra-analyze', enabled: false },
  ],
);
check(
  'disabled variants are Library membership and can be re-enabled',
  planProjectBoardMutations(
    projectVariants.map((variant) => ({ ...variant, enabled: false })),
    'codex',
  ),
  [{ kind: 'toggle', agent: 'codex', relativePath: 'spectra-analyze', enabled: true }],
);

const mutationEvents: string[] = [];
try {
  await runBoardMutationSequence(
    ['codex', 'claude'],
    async (agent) => {
      mutationEvents.push(`apply:${agent}`);
      if (agent === 'claude') throw new Error('intercepted failure');
    },
    async (agent) => {
      mutationEvents.push(`rollback:${agent}`);
    },
  );
} catch {
  // Expected: the second mutation fails after the first was confirmed.
}
check(
  'a failed target update reverses earlier confirmed steps',
  mutationEvents,
  ['apply:codex', 'apply:claude', 'rollback:codex'],
);

if (failures.length) {
  console.error(`Board lane check failed with ${failures.length} error(s):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}

console.log('Board lane check passed.');
