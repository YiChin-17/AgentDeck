// The text a frontend-authority rule is allowed to look at.
//
// The Hook and Plugin checkers assert that their IPC wrappers accept no
// filesystem path, executable, argument vector, working directory or
// environment. Both used to take everything from the first wrapper to the end
// of `src/lib/tauri.ts`, which quietly turned every later declaration into a
// Hook or Plugin argument — a response field named `displayPath` and a comment
// containing "environment" were enough to fail the build. Scoping to the named
// wrapper declarations keeps the rule tied to the wrappers rather than to
// where they happen to sit in the file. Uses only the Node standard library.

/**
 * Concatenate the declarations of the named exported wrappers.
 *
 * @param {string} source Contents of the module that declares the wrappers.
 * @param {string[]} wrapperNames Exported `const` wrappers to include.
 * @returns {{ surface: string, missing: string[] }} The joined declarations,
 *   and the wrapper names that are not declared in `source` — a caller that
 *   ignores `missing` would let a renamed wrapper narrow the rule to nothing.
 */
export function wrapperArgumentSurface(source, wrapperNames) {
  const blocks = [];
  const missing = [];

  for (const name of wrapperNames) {
    const start = source.indexOf(`export const ${name} =`);
    if (start === -1) {
      missing.push(name);
      continue;
    }
    // A wrapper declaration is one statement with no blank line inside it, and
    // top-level declarations are separated by blank lines — so the first blank
    // line is the end of this wrapper and the start of whatever follows,
    // including the doc comment belonging to the next declaration.
    const declaration = source.slice(start);
    const end = declaration.indexOf("\n\n");
    blocks.push(end === -1 ? declaration : declaration.slice(0, end));
  }

  return { surface: blocks.join("\n"), missing };
}
