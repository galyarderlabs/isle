import { defineConfig } from 'vitepress'

// Notchfolk docs + landing page.
// Built from the markdown already in docs/; the internal working docs
// (assistant-platform-*, gsconnect-*, rebrand-*, router-*) are excluded
// from the site and from the sidebar.
export default defineConfig({
  base: '/notchfolk/',
  title: 'Notchfolk',
  description: 'A little world in your notch.',
  lang: 'en-US',

  // The internal working docs live in docs/ but are not part of the site.
  srcExclude: [
    'assistant-platform-plan.md',
    'assistant-platform-status.md',
    'assistant-platform-tasks.md',
    'gsconnect-plan.md',
    'gsconnect-tasks.md',
    'rebrand-plan.md',
    'rebrand-tasks.md',
    'router-plan.md',
    'router-tasks.md'
  ],

  // Links that intentionally point outside the site (the excluded internal
  // docs, SECURITY.md / README.md at the repo root, and repo-relative
  // GitHub links) must not fail the build. Real dead links inside the
  // site still fail the build.
  // VitePress normalises links before this check (relative, .md stripped),
  // so match the normalised form: ./assistant-platform-status,
  // ./../SECURITY, ./../../releases, ...
  ignoreDeadLinks: [
    (link: string) => {
      // Links that escape docs/ (../SECURITY, ../../releases, ...) are
      // intentionally outside the site.
      if (/(^|\/)\.\.\//.test(link)) return true
      // Links to the excluded internal docs (extension stripped).
      return /(^|\/)(assistant-platform-plan|assistant-platform-status|assistant-platform-tasks|gsconnect-plan|gsconnect-tasks|rebrand-plan|rebrand-tasks|router-plan|router-tasks)(#.*)?$/.test(
        link
      )
    }
  ],

  themeConfig: {
    logo: '/images/logo.png',
    siteTitle: 'Notchfolk',

    nav: [
      { text: 'Home', link: '/' },
      { text: 'Install', link: '/install' },
      { text: 'Usage', link: '/usage' },
      { text: 'MCP', link: '/mcp' },
      { text: 'GitHub', link: 'https://github.com/galyarderlabs/notchfolk' }
    ],

    sidebar: [
      {
        text: 'Getting started',
        items: [
          { text: 'Install', link: '/install' },
          { text: 'Using Notchfolk', link: '/usage' },
          { text: 'How it works', link: '/how-it-works' },
          { text: 'Migrating from Isle', link: '/migrate-from-isle' }
        ]
      },
      {
        text: 'Assistant',
        items: [
          { text: 'The assistant', link: '/assistant' },
          { text: 'Assistant runtimes', link: '/assistant-runtimes' },
          { text: 'MCP server', link: '/mcp' }
        ]
      },
      {
        text: 'Developer',
        items: [
          { text: 'Data and network', link: '/data-and-network' },
          { text: 'D-Bus API', link: '/dbus-api' },
          { text: 'Assistant adapter boundary', link: '/assistant-adapter-boundary' },
          { text: 'Product spec', link: '/product-spec' }
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/galyarderlabs/notchfolk' }
    ],

    footer: {
      message: 'Open source under GPL-3.0. Designed for GNOME 46–51.',
      copyright: 'Copyright © 2026 Galyarder Labs'
    },

    search: {
      provider: 'local'
    }
  }
})
