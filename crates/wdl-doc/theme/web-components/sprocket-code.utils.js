import { createHighlighterCore } from 'shiki/core';
import materialThemeOcean from '@shikijs/themes/material-theme-ocean';
import { createOnigurumaEngine } from 'shiki/engine/oniguruma';
import wasm from 'shiki/wasm';

// WDL Grammar fetching logic
let wdlGrammarCache = null;
const WDL_GRAMMAR_URL = 'https://raw.githubusercontent.com/stjude-rust-labs/sprocket-vscode/refs/heads/main/syntaxes/wdl.tmGrammar.json';

async function getWdlGrammar() {
  if (wdlGrammarCache) {
    console.log('sprocket-code-utils: using cached WDL grammar');
    return wdlGrammarCache;
  }
  try {
    console.log('sprocket-code-utils: fetching WDL grammar from', WDL_GRAMMAR_URL);
    const response = await fetch(WDL_GRAMMAR_URL);
    if (!response.ok) {
      throw new Error(`Failed to fetch WDL grammar: ${response.status} ${response.statusText}`);
    }
    const grammar = await response.json();
    wdlGrammarCache = grammar;
    console.log('sprocket-code-utils: WDL grammar fetched and cached');
    return wdlGrammarCache;
  } catch (error) {
    console.error('sprocket-code-utils: failed to fetch or parse WDL grammar:', error);
    return null; // Gracefully degrade; WDL highlighting won't work for WDL
  }
}

// Global singleton highlighter promise cache
if (!window.sprocketHighlighterPromise) {
  window.sprocketHighlighterPromise = null;
}

// Highlighter initialization logic
export async function initializeHighlighter() {
  // If we already have a promise (ongoing or completed), return it
  if (window.sprocketHighlighterPromise) {
    console.log('sprocket-code-utils: using cached/ongoing highlighter initialization');
    return await window.sprocketHighlighterPromise;
  }

  console.log('sprocket-code-utils: starting highlighter initialization');
  
  // Create and cache the initialization promise
  window.sprocketHighlighterPromise = (async () => {
    try {
      const wdlLangDefinition = await getWdlGrammar();
      const languagesToLoad = []; // Don't load any languages by default

      if (wdlLangDefinition) {
        languagesToLoad.push(wdlLangDefinition);
      } else {
        // Log a warning if WDL grammar couldn't be loaded
        console.warn('sprocket-code-utils: WDL grammar could not be loaded. WDL syntax highlighting will be unavailable.');
      }

      const highlighter = await createHighlighterCore({
        themes: [materialThemeOcean],
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
