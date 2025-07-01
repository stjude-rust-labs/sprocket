import { defineConfig } from "vitepress";
import axios from 'axios';

const grammarUrl = "https://raw.githubusercontent.com/stjude-rust-labs/sprocket-vscode/refs/heads/main/syntaxes/wdl.tmGrammar.json";

export default defineConfig({
  title: "Sprocket",
  description:
    "A bioinformatics workflow engine built on top of the Workflow Description Language (WDL)",
  themeConfig: {
    logo: {
      light: "/sprocket-logo.png",
      dark: "/sprocket-logo-dark.png",
    },
    siteTitle: "",
    nav: [
      { text: "Documentation", link: "/overview" },
      {
        text: "v0.13.0",
        items: [
          {
            text: "Changelog",
            link: "https://github.com/stjude-rust-labs/sprocket/blob/main/CHANGELOG.md",
          },
        ],
      },
    ],
    sidebar: [
      {
        text: "Getting Started",
        items: [
          { text: "Overview", link: "/overview" },
          { text: "Installation", link: "/installation" },
          { text: "Guided Tour", link: "/guided-tour" },
        ],
      },
      {
        text: "Visual Studio Code Extension",
        items: [{ text: "Getting Started", link: "/vscode/getting-started" }],
      },
      {
        text: "Sprocket CLI",
        items: [
          { text: "Configuration", link: "/cli/config" },
          { text: "Doc", link: "/cli/doc" },
          { text: "Lock", link: "/cli/lock" },
        ],
      },
    ],
    socialLinks: [
      { icon: "github", link: "https://github.com/stjude-rust-labs/sprocket" },
    ],
  },
  markdown: {
    theme: 'github-dark',
    shikiSetup: async (shiki) => {
      const response = await axios.get(grammarUrl);
      await shiki.loadLanguage(response.data);
    }
  },
  appearance: 'dark'
});
