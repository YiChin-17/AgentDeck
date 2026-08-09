#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const css = fs.readFileSync(path.join(root, 'src', 'index.css'), 'utf8');
const layout = fs.readFileSync(path.join(root, 'src', 'components', 'Layout.tsx'), 'utf8');
const sidebar = fs.readFileSync(path.join(root, 'src', 'components', 'Sidebar.tsx'), 'utf8');
const board = fs.readFileSync(path.join(root, 'src', 'components', 'ArtifactBoard.tsx'), 'utf8');
const detailSheet = fs.readFileSync(path.join(root, 'src', 'components', 'DetailSheet.tsx'), 'utf8');
const library = fs.readFileSync(path.join(root, 'src', 'views', 'MySkills.tsx'), 'utf8');
const project = fs.readFileSync(path.join(root, 'src', 'views', 'ProjectDetail.tsx'), 'utf8');

const errors = [];
const toolbarRule = css.match(/\.app-board-toolbar\s*\{([^}]+)\}/)?.[1] ?? '';
const pageRule = css.match(/\.app-page-board\s*\{([^}]+)\}/)?.[1] ?? '';
const contentRule = css.match(/\.app-board-content\s*\{([^}]+)\}/)?.[1] ?? '';
const viewRule = css.match(/\.app-board-view\s*\{([^}]+)\}/)?.[1] ?? '';
const listRule = css.match(/\.app-list-scroll\s*\{([^}]+)\}/)?.[1] ?? '';

if (!pageRule.includes('min-h-0') || !pageRule.includes('flex-1') || !pageRule.includes('overflow-hidden')) {
  errors.push('Board pages must consume the remaining shell height without becoming a page scroll container');
}
if (!toolbarRule.includes('shrink-0') || toolbarRule.includes('sticky')) {
  errors.push('the Board toolbar must remain outside the internal content scroller');
}
if (!contentRule.includes('min-h-0') || !contentRule.includes('overflow-hidden')) {
  errors.push('the Board content row must clip its children to the available viewport height');
}
if (!viewRule.includes('min-h-0') || !viewRule.includes('overflow-hidden')) {
  errors.push('the central Board view must pass a bounded height to its lane scrollers');
}
if (!listRule.includes('overflow-y-auto')) {
  errors.push('Grid and List views must retain vertical scrolling inside the bounded content region');
}
if (/className="flex flex-wrap items-center gap-1 px-1 -mt-2 -mb-3"/.test(library)) {
  errors.push('Library secondary filters must not use negative margins against the sticky toolbar');
}
if (!library.includes('app-board-toolbar') || !project.includes('app-board-toolbar')) {
  errors.push('Library and Project must both use the shared Board toolbar');
}
if (!library.includes('app-page-board') || !project.includes('app-page-board')) {
  errors.push('Library and Project must both opt into the bounded Board page layout');
}
if (!library.includes('app-board-content') || !project.includes('app-board-content')) {
  errors.push('Library and Project must both use the bounded Board content row');
}
if (!library.includes('app-board-view') || !project.includes('app-board-view')) {
  errors.push('Library and Project must both bound the central Board view');
}
if (!library.includes('app-list-scroll') || !project.includes('app-list-scroll')) {
  errors.push('Library and Project Grid/List content must use the internal list scroller');
}
if (!layout.includes('isBoardRoute') || !layout.includes('flex min-h-0 flex-1 flex-col overflow-hidden')) {
  errors.push('Board routes must disable scrolling on the shell content viewport');
}
if (!layout.includes('flex-1 overflow-y-auto')) {
  errors.push('non-Board routes must keep the existing page scroll behavior');
}
if (!sidebar.includes('const isPresetContextActive = location.pathname === "/my-skills"')) {
  errors.push('Skill Pack selection must be scoped to the central Library route');
}
if (!sidebar.includes('isPresetContextActive && viewedPreset?.id === preset.id')) {
  errors.push('Skill Packs must not retain selected styling in Project or Agent routes');
}
if (!layout.includes('className="fixed inset-x-0 top-0 z-50 h-[28px]')) {
  errors.push('the opaque window drag strip must stay fixed above every scroll layer');
}
if (!board.includes('overflow-x-auto overflow-y-hidden')) {
  errors.push('the four-lane Board must scroll horizontally without becoming a vertical page scroller');
}
if (!board.includes('h-full min-h-0') || !board.includes('overflow-hidden')) {
  errors.push('each Board lane must clip cards to its bounded height');
}
if (!board.includes('min-h-0 flex-1 flex-col gap-2 overflow-y-auto')) {
  errors.push('each Board lane card list must own vertical scrolling');
}
if (!board.includes('libraryLaneLabel') || !board.includes('lane === "library" ? libraryLaneLabel')) {
  errors.push('ArtifactBoard must support a context-specific label for the false/false lane');
}
if (!board.includes('boardScrollRef') || !board.includes('data-board-lane={lane}')) {
  errors.push('ArtifactBoard must expose its horizontal scroller and lane positions');
}
if (!board.includes('selectedLane') || !board.includes('scrollTo({')) {
  errors.push('ArtifactBoard must keep the selected lane visible after Inspector layout changes');
}
if (!board.includes('ResizeObserver')) {
  errors.push('ArtifactBoard must re-check the selected lane when Inspector resizing changes the Board width');
}
if (!project.includes('libraryLaneLabel={t("board.lanes.undeployed")}')) {
  errors.push('Project Board must label the false/false lane as undeployed');
}
if (!detailSheet.includes('h-full max-h-full')) {
  errors.push('the docked Inspector must stay within the bounded Board content row');
}

if (errors.length) {
  console.error(`Board layout check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log('Board layout check passed.');
