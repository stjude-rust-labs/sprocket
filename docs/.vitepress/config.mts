import { defineConfig } from "vitepress";

export default defineConfig({
  title: "Sprocket",
  description:
    "A bioinformatics workflow orchestration engine and package manager built on top of the Workflow Description Language (WDL)",
  base: "/sprocket/",
  themeConfig: {
    logo: {
      light: "/sprocket-logo.png",
      dark: "/sprocket-logo-dark.png",
    },
    siteTitle: "",
    nav: [
      { text: "Documentation", link: "/overview" },
      {
        text: "v0.7.0",
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
        ],
      },
      {
        text: "Visual Studio Code Extension",
        items: [{ text: "Getting Started", link: "/vscode/getting-started" }],
      },
    ],

    socialLinks: [
      { icon: "github", link: "https://github.com/stjude-rust-labs/sprocket" },
    ],
  },
});
