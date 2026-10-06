import { createHighlighterCore, createCssVariablesTheme } from 'shiki/core';
import { createOnigurumaEngine } from 'shiki/engine/oniguruma';
import wasm from 'shiki/wasm';
import bashGrammar from '@shikijs/langs/bash';

// WDL TextMate grammar, vendored from sprocket-vscode so highlighting works
// offline and cannot break when the upstream file moves. Refresh with:
//   curl -sL https://raw.githubusercontent.com/stjude-rust-labs/sprocket-vscode/refs/heads/main/syntaxes/wdl.tmGrammar.json \
//     -o web-common/grammars/wdl.tmGrammar.json
import wdlGrammar from '../grammars/wdl.tmGrammar.json';

// Shiki's core highlighter does not apply TextMate injection grammars, so to
// highlight WDL `~{...}` placeholders inside shell (bash) command sections the
// bash grammar itself is patched: this rule is prepended to every `patterns`
// list so the placeholder is matched with priority in every context —
// top-level, unquoted arguments, and inside quoted strings.
const WDL_PLACEHOLDER_RULE = {
  name: 'meta.placeholder.shell.wdl',
  match: '~\\{[^{}]*\\}',
};

/// Recursively prepends `WDL_PLACEHOLDER_RULE` to every `patterns` array in a
/// TextMate grammar node, in place, returning the node.
function injectWdlPlaceholders(node, seen = new Set()) {
  if (!node || typeof node !== 'object' || seen.has(node)) {
    return node;
  }
  seen.add(node);
  if (Array.isArray(node)) {
    for (const value of node) injectWdlPlaceholders(value, seen);
    return node;
  }
  if (Array.isArray(node.patterns) && node.patterns[0] !== WDL_PLACEHOLDER_RULE) {
    node.patterns.unshift(WDL_PLACEHOLDER_RULE);
  }
  for (const key of Object.keys(node)) injectWdlPlaceholders(node[key], seen);
  return node;
}

// Bash grammar with WDL placeholder highlighting patched in. Cloned so the
// shared imported grammar is not mutated.
const patchedBashGrammar = structuredClone(bashGrammar).map((grammar) =>
  injectWdlPlaceholders(grammar),
);

// Global singleton highlighter promise cache
if (!window.sprocketHighlighterPromise) {
  window.sprocketHighlighterPromise = null;
}

// Highlighter initialization logic
//
// This sets up a highlighter for the provided languages, as well as WDL (implicitly).
export async function initializeHighlighter(languagesToLoad = []) {
  // If we already have a promise (ongoing or completed), return it
  if (window.sprocketHighlighterPromise) {
    console.log('sprocket-code-utils: using cached/ongoing highlighter initialization');
    return await window.sprocketHighlighterPromise;
  }

  console.log('sprocket-code-utils: starting highlighter initialization');

  // Create and cache the initialization promise
  window.sprocketHighlighterPromise = (async () => {
    try {
      // Load the bundled WDL and bash grammars alongside any caller-provided
      // languages. Bash covers task command sections. `@shikijs/langs/bash`
      // exports an array (bash plus its shell aliases). The injection adds WDL
      // placeholder highlighting on top of shell.
      languagesToLoad.push(wdlGrammar, ...patchedBashGrammar);

      // A single css-variables theme: highlighted spans reference
      // `var(--shiki-*)` custom properties that the site stylesheet maps to
      // brand tokens, so code colors track the light/dark theme with one
      // definition block.
      const theme = createCssVariablesTheme({
        name: 'sprocket',
        variablePrefix: '--shiki-',
        fontStyle: true,
      });
      // WDL-specific scope mappings layered on the css-variables defaults.
      // `entity.name.type.wdl` is Int/File/Array/... (types); `storage.type`
      // and `storage.type.*.wdl` are block/declaration keywords, not types.
      // `createCssVariablesTheme` exposes its rules as `tokenColors` (VS Code
      // shape), each `{ scope, settings: { foreground } }`.
      theme.tokenColors.push(
        {
          scope: ['entity.name.type.wdl'],
          settings: { foreground: 'var(--shiki-token-type)' },
        },
        {
          scope: [
            'storage.type',
            'storage.type.task.wdl',
            'storage.type.workflow.wdl',
            'storage.type.struct.wdl',
            'storage.type.enum.wdl',
            'storage.type.command.wdl',
            'storage.modifier.wdl',
            'keyword.wdl',
            'keyword.other.wdl',
          ],
          settings: { foreground: 'var(--shiki-token-keyword)' },
        },
        {
          scope: [
            'variable.name.task.wdl',
            'variable.name.workflow.wdl',
            'variable.name.struct.wdl',
            'variable.name.enum.wdl',
          ],
          settings: { foreground: 'var(--shiki-token-function)' },
        },
        {
          scope: ['meta.other.placeholder.wdl'],
          settings: { foreground: 'var(--shiki-token-string-expression)' },
        },
        {
          // Placeholders injected into bash command blocks render in the
          // keyword (purple) color to stand out from surrounding shell.
          scope: ['meta.placeholder.shell.wdl'],
          settings: { foreground: 'var(--shiki-token-keyword)' },
        },
      );

      const highlighter = await createHighlighterCore({
        themes: [theme],
        langs: languagesToLoad,
        engine: createOnigurumaEngine(wasm)
      });

      console.log('sprocket-code-utils: highlighter initialized successfully (singleton)');
      return highlighter;
    } catch (error) {
      console.error('sprocket-code-utils: failed to initialize highlighter core:', error);
      // Reset the promise cache on error so retry is possible
      window.sprocketHighlighterPromise = null;
      return null;
    }
  })();

  return await window.sprocketHighlighterPromise;
}
