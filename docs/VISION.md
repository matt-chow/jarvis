# Vision

Directory is a single, personal operating environment: one place for notes,
files, tasks, and knowledge. It works on every device the owner uses and keeps
working without a network. Later, JARVIS joins as a voice and AI assistant that
can act across every module.

## Principles

1. **Personal and private.** One owner. Data is private by default, and every
   cloud access is authenticated server-side.
2. **Offline first.** The app opens and core modules work without a connection.
   The cloud is for sync and backup, not a hard dependency.
3. **Modular.** Capabilities ship as modules in one modular monolith with shared
   identity, storage, and UI conventions.
4. **Portable core.** Domain logic is plain Rust, independent of Cloudflare, so it
   can be reused by future Windows and iPhone clients or moved to another host.
5. **Frugal.** It runs on free tiers. A paid service is never enabled without
   explicit approval.
6. **Calm, capable interface.** Modern productivity UI with a subtle retro
   character: monospace accents, terminal-style status, warm paper and amber
   tones.

## Non-goals (for now)

- Multi-user collaboration and sharing.
- Microservices or a separate backend per module.
- Native apps before the web/PWA foundation is solid.
