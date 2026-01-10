# Nomad - Rust Implementation

A terminal-based encrypted communication platform built on LXMF and Reticulum.

## Overview

Nomad is a Rust implementation of Nomad Network functionality, providing:
- Encrypted peer-to-peer messaging over mesh networks
- Node hosting (pages, files)
- Text browser for navigating node content
- Works over any Reticulum transport (TCP, UDP, LoRa, serial, etc.)

## Dependencies

Internal crates (in /Users/ejohn/Downloads/nomad/):
- `lxmf` - LXMF protocol implementation with LxmfNode
- `reticulum` - Reticulum transport layer
- `micron` - Micron markup parser and ratatui renderer

## Core Components

### 1. Identity & Configuration
- Generate/load persistent identity from disk
- Configuration file (~/.nomad/ or similar)
- Store known peers and trust levels

### 2. Messaging (Conversations)
- Send/receive encrypted LXMF messages
- Conversation storage (SQLite or flat files)
- Message states (sending, delivered, failed)
- Integration with propagation nodes for offline delivery

### 3. Node Hosting
- Serve Micron pages from filesystem
- Serve files for download
- Handle incoming page/file requests
- Optional: dynamic page generation

### 4. Browser
- Navigate to node pages via destination hash + path
- Render Micron content using existing parser
- Handle links, form fields
- Page cache
- Navigation history (back/forward)

### 5. Directory
- Track announces from network
- Store known nodes/peers
- Display network activity

### 6. TUI Application
- ratatui-based terminal interface
- Tabs/screens: Conversations, Network (Browser + Directory), Guide
- Keyboard navigation

## Architecture

```
┌─────────────────────────────────────────────┐
│                TUI (ratatui)                │
├─────────────────────────────────────────────┤
│  Conversations │ Browser │ Directory │ Guide│
├─────────────────────────────────────────────┤
│              NomadApp (core)                │
├──────────────┬──────────────┬───────────────┤
│   Node       │ Conversation │   Storage     │
│  (hosting)   │  (messages)  │  (identity)   │
├──────────────┴──────────────┴───────────────┤
│                 LXMF Layer                  │
│               (lxmf crate)                  │
├─────────────────────────────────────────────┤
│              Reticulum Layer                │
│            (reticulum crate)                │
├─────────────────────────────────────────────┤
│     Interfaces (TCP, UDP, LoRa, etc.)       │
└─────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Project setup with workspace dependencies
- [ ] Identity management (generate, save, load)
- [ ] Configuration system
- [ ] Connect to network via LxmfNode

### Phase 2: Messaging
- [ ] Send LXMF messages to destination hash
- [ ] Receive incoming messages
- [ ] Conversation storage
- [ ] Basic TUI for conversations

### Phase 3: Browser
- [ ] Request pages from remote nodes
- [ ] Render Micron pages (using micron crate)
- [ ] Link navigation
- [ ] Form field handling

### Phase 4: Node Hosting
- [ ] Serve pages from local filesystem
- [ ] Handle incoming page requests
- [ ] Serve files

### Phase 5: Polish
- [ ] Full TUI with all screens
- [ ] Propagation node integration
- [ ] Trust levels for peers
- [ ] Announce streaming

## File Structure

```
nomad/
├── Cargo.toml
├── src/
│   ├── main.rs           # Entry point
│   ├── app.rs            # NomadApp core
│   ├── config.rs         # Configuration
│   ├── identity.rs       # Identity management
│   ├── conversation/     # Messaging
│   │   ├── mod.rs
│   │   ├── message.rs
│   │   └── storage.rs
│   ├── node/             # Hosting
│   │   ├── mod.rs
│   │   └── handler.rs
│   ├── browser/          # Page navigation
│   │   ├── mod.rs
│   │   └── cache.rs
│   └── tui/              # Terminal UI
│       ├── mod.rs
│       ├── app.rs
│       ├── conversations.rs
│       ├── browser.rs
│       └── directory.rs
└── tests/
```

## MVP Scope

Minimum viable product:
1. Connect to testnet
2. Send/receive messages with another LXMF client (e.g., Sideband)
3. Browse a remote node's pages
4. Basic TUI showing conversations and browser
