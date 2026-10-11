# Online play, the deck screens and the server — the person's design notes, 11 October 2026

The person's own notes and wireframes, kept verbatim as the design the roadmap entries
resolve to: Phase 4 §8–§12 (formats, queues, the store, events, administration) and
Phase 7 §4bo and §14–§16 (the prompt's words, the dashboard, the P2P hub, the deck
builder and the deck picker) in `docs/roadmap/`. The decisions taken on them the same
day are recorded in those entries, not here; where an entry and a wireframe differ, the
entry is the decision.

---

# Cards and Issues and Display to fix

**Cultivate**
 * This card in particular and many like it will have options like "select card to trash" or "select card to put in hand" but the UI doesn't tell you which option you are performing - am I trashing or putting into hand?
 * The other part of Cultivate is that it didn't allow me to re-order the remaining cards which was crucial because the next thing to trigger was my identity card that is about revealing the top card of my R&D and option to trash it - if I could actually have reordered then I could put a card I wanted to trash.

 * When Identity cards trigger I don't seem to recall seeing any notification why I am being presented a choice. Similar to the Options about what I am selecting and why - what card is causing me to make a choice - this could be an Identity card (mine or my opponents), it could by an ICE, anything - name the card - name the choice/action that is occuring "trashing", "adding to HQ/Grip", "reordering", etc.

---

# Server Ideas

Change Deck type legality to just be STANDARD, STARTUP, SNAPSHOT, ETERNAL - no CASUAL (casusal will be mode of play but still use legality)

Servers need to hold data. Public Key and rank and any other server side data to be tracked.

```
[ Server Dashboard ]
       │
       ├──► [ 1. Select Dual Decks ] ──► (Client-side validation of Corp & Runner decks)
       │
       ├──► [ 2. Choose Play Mode ]
       │     ├── Ranked Ladder (Competitive MMR, legal decks only)
       │     ├── Casual / Unranked (Relaxed matching, legal decks only)
       │     └── Tournaments & Event Brackets (Structured or rolling events)
       │
       ├──► [ 3. Configure Queue Parameters ]
       │     ├── Format Selection (Standard, Startup, Snapshot, Eternal - auto-grayed if decks mismatch)
       │     └── Role Soft Filter (Any/Balanced, Prefer Corp, Prefer Runner)
       │
       ├──► [ 4. "Find Match" Action ] ──► [ Matchmaking Queue State Spinner ] ──► [ Game Board ]
       │
       └──► [ Server Profile Manager ] ──► (View earned achievement titles/badges & select active display tags)
```

The Dashboard: A clean, cyberpunk-styled dashboard displaying a prominent primary action button ("Play Mode") and prominent Active Decks.

1. Main Dashboard & Deck Assignment

```
┌────────────────────────────────────────────────────────────────────────┐
│ [NETRUNNER LOGO]                     [User Profile / Tag] [Settings]   │
├────────────────────────────────────────────────────────────────────────┤
│ ACTIVE DECKS                           NAVIGATION & PROFILES           │
│ ┌─────────────────────────────────┐    ┌─────────────────────────────┐ │
│ | CORP: [ Weyland Fast Advance ]  │    │ • [ Deckbuilder ] (Local)   │ │
│ | RUNNER: [ Reina Roja Eco ]      │    │ • [ Collection ]  (Local)   │ │
│ └─────────────────────────────────┘    │ • [ Server Profile Page ]   │ │
│                                        └─────────────────────────────┘ │
│ PLAY MODES                                                             │
│  [ RANKED LADDER ]      [ CASUAL / UNRANKED ]      [ TOURNAMENTS ]     │
└────────────────────────────────────────────────────────────────────────┘
```

Key UX Note: The active deck is persistently visible on the main screen. Clicking [ PLAY GAME ] triggers a modal rather than forcing a page transition, keeping friction to an absolute minimum.

2. Format & Role Selection Modal (Triggered by selecting a mode)

```
┌────────────────────────────────────────────────────────────────────────┐
│ CONFIGURE QUEUE: RANKED LADDER                                         │
├────────────────────────────────────────────────────────────────────────┤
│ SELECT FORMAT (Validated against active Corp & Runner decks):          │
│  [◉] STANDARD         [ ] STARTUP (Grayed: Deck contains non-Startup)  │
│  [ ] SNAPSHOT         [ ] ETERNAL (Grayed: Restricted list violation)  │
│                                                                        │
│ ROLE SOFT FILTER (Prevents queue deadlock):                            │
│  [ ] Prefer Corp    [◉] Any / Balanced (Recommended)  [ ] Prefer Runner│
├────────────────────────────────────────────────────────────────────────┤
│          [ CANCEL ]                    [ FIND MATCH ]                  │
└────────────────────────────────────────────────────────────────────────┘
```

3. Matchmaking Queue State

```
┌────────────────────────────────────────────────────────────────────────┐
│ JACKING INTO THE MAINNET...                                            │
├────────────────────────────────────────────────────────────────────────┤
│                                  (o)                                   │
│                       [ Terminal Pulse Spinner ]                       │
│                                                                        │
│                       Searching for opponent...                        │
│                       Format: Standard | Role Preference: Any          │
│                       Elapsed Time: 00:18                              │
│                                                                        │
│                         ┌────────────────────┐                         │
│                         │    [ ABORT ]       │                         │
│                         └────────────────────┘                         │
└────────────────────────────────────────────────────────────────────────┘
```

4. Server Profile Page & Achievement Display

A dedicated server-side view where players manage their non-profit lightweight incentives (titles and badges earned via event brackets).

```
┌────────────────────────────────────────────────────────────────────────┐
│ SERVER PROFILE: Operator_X           [ Back to Main Dashboard ]        │
├────────────────────────────────────────────────────────────────────────┤
│ ACTIVE IN-GAME DISPLAY:                                                │
│  Selected Title: [ "Net-Slinger" ]    Selected Badge: [ Weyland Retro ]│
│                                                                        │
│ UNLOCKED ACHIEVEMENTS & EVENT INCENTIVES:                              │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ ☑ Title: "Net-Slinger" (Unlocked via Red Planet Gauntlet Win 3)  │  │
│  │ ☑ Badge: Weyland Retro 06 (Unlocked via Startup Gauntlet)        │  │
│  │ ☐ Title: "Icebreaker Specialist" (Locked: 0/5 Standard Gauntlets)│  │
│  │ ☐ Badge: Anarch Amber Emblem (Locked: Event Tier 2)              │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│ [ SAVE PROFILE SELECTIONS ]                                            │
└────────────────────────────────────────────────────────────────────────┘
```

5. Event Bracket UI Wireframe & Lightweight Incentives

Because this is a non-profit project, event brackets can use client-side progression milestones, cosmetic unlocks, and achievement badges instead of monetary rewards.

Tournaments vs. Event Brackets

 * Tournaments (Structured & Synchronized):
   * Structure: Fixed start-time events featuring multi-round Swiss pairings and/or elimination cuts.
   * Flow: The tournament engine pauses round generation until all active table matches finish, advancing all players in lockstep to the next round. Decks are strictly locked at registration.
   * Strict Regulation: Usually locked to a single competitive format (e.g., Standard) with decklists locked prior to registration.
 * Event Brackets (Asynchronous & Rolling):
   * Structure: On-demand, gauntlet-style mini-events that players can enter anytime.
   * Flow: Operates on a rolling queue. When you click find match, it immediately pairs you against any active participant in that event pool without waiting for external tables to finish.
   * Flexible / Variant Rules: Frequently uses quirky rule modifications, experimental Snapshot formats, or restricted factional constraints to test the meta.

```
┌────────────────────────────────────────────────────────────────────────┐
│ TOURNAMENT: Weekend Cyber-Clash [Status: Round 2 of 4]                 │
├────────────────────────────────────────────────────────────────────────┤
│ ROUND 2 PAIRINGS                          TOURNAMENT STANDINGS         │
│ ┌──────────────────────────────┐          ┌──────────────────────────┐ │
│ │ Table 1: [Corp] you vs       │          │ 1. UserA (2-0, 6 pts)    │ │
│ │          [Runner] opponentX  │          │ 2. you   (1-1, 3 pts)    │ │
│ │ Status: [ ENTER MATCH ]      │          │ 3. UserB (1-1, 3 pts)    │ │
│ └──────────────────────────────┘          └──────────────────────────┘ │
│                                                                        │
│ ┌──────────────────────────────┐          ┌──────────────────────────┐ │
│ │ Table 2: UserY vs UserZ      │          │                          │ │
│ │ Status: In Progress (12:45)  │          │ [ View Full Bracket ]    │ │
│ └──────────────────────────────┘          └──────────────────────────┘ │
│                                                                        │
│ Next round auto-generates once all active matches submit results.      │
└────────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────────┐
│ SPECIAL EVENT: "The Red Planet Gauntlet" [Format: Startup]             │
├────────────────────────────────────────────────────────────────────────┤
│ PROGRESSION TRACK: 3 Wins Required to Clear Gauntlet                   │
│                                                                        │
│  [ WIN 1 ] ──► [ WIN 2 ] ──► [ WIN 3 (FINAL) ]                         │
│  (Completed)     (Completed)     (Active Match)                        │
│                                                                        │
│ UNLOCKED INCENTIVES: Server Side Profile                               │
│  ☑ Milestone 1: Custom Profile Title ["Net-Slinger"]                   │
│  ☑ Milestone 2: Custom Icon [ White ]                                  │
│  ☐ Milestone 3: Custom Icon [ Purple ]                                 │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│          [ QUIT EVENT ]                 [ FIND EVENT MATCH ]           │
└────────────────────────────────────────────────────────────────────────┘
```

___

# End-to-End User Journey: Local / P2P Connection Flow

```
[ Main Dashboard ] ──► [ Click "Local / P2P Play" ]
                              │
                              ├──► [ HOST GAME ] ──► Select Decks ──► Choose Protocol (LAN / Port / Iroh) ──► Waiting Room Lobby ──► [ Game Board ]
                              │
                              └──► [ JOIN GAME ] ──► Select Decks ──► Input Connection String / IP / Iroh Ticket ──► Handshake / Sync ──► [ Game Board]
```

UI Wireframe 1: Local / P2P Mode Hub (Host vs. Join)

When a player clicks "Local / P2P Play" from the main dashboard, they choose whether they are spinning up a hosted session or connecting to an existing peer.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [← Back to Dashboard]  LOCAL / P2P CONNECTION HUB                      │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌────────────────────────────────┐  ┌──────────────────────────────┐ │
│   │ HOST A SESSION                 │  │ JOIN A SESSION               │ │
│   │ Spin up a local node and invite│  │ Connect directly to a peer   │ │
│   │ a challenger via LAN, Port,    │  │ via IP, Port, or Iroh Ticket │ │
│   │ or Iroh secure tunnel.         │  │                              │ │
│   │                                │  │                              │ │
│   │ [ HOST NEW GAME ]              │  │ [ JOIN EXISTING GAME ]       │ │
│   └────────────────────────────────┘  └──────────────────────────────┘ │
│                                                                        │
│   SAVED PEERS / RECENT CONNECTIONS:                                    │
│   • Local LAN Node (192.168.1.50:7777)               [ Quick Connect ] │
│   • Iroh Ticket: `iroh://7g8f...dx2a`                [ Quick Connect ] │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

UI Wireframe 2: Hosting a P2P Session (Protocol & Deck Setup)

When the host clicks "Host New Game", they configure their active dual decks and choose how they want to expose their connection.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [← Back]  HOST P2P SESSION CONFIGURATION                               │
├────────────────────────────────────────────────────────────────────────┤
│ 1. ACTIVE DECKS:                                                       │
│    • Corp: [ Weyland Fast Advance ]   [ Change Deck ]                  │
│    • Runner: [ Reina Roja Eco ]       [ Change Deck ]                  │
│                                                                        │
│ 2. CONNECTION PROTOCOL:                                                │
│    [ ] Local LAN Only (Zero-config broadcast on local subnet)          │
│    [ ] Port Forwarding / Direct IP (Open Port: [ 7777 ])               │
│    [◉] Iroh Secure Tunnel (End-to-end encrypted internet P2P)          │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│          [ CANCEL ]                    [ INITIALIZE HOST NODE ]        │
└────────────────────────────────────────────────────────────────────────┘
```

UI Wireframe 3: Host Waiting Room / Lobby State

Once initialized, the host views the connection ticket or IP string to share with their opponent while waiting for the handshake.

```
┌────────────────────────────────────────────────────────────────────────┐
│ P2P HOST LOBBY [Status: Listening for Peer...]                         │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│ CONNECTION TICKET (Share with Opponent):                               │
│ ┌────────────────────────────────────────────────────────────────────┐ │
│ │ iroh://netrunner-rs.node.v2.7g8f9a2b3c4d5e6f7g8h9i0j1k2l3m4n5o6p.. │ │
│ └────────────────────────────────────────────────────────────────────┘ │
│ [ Copy Connection Ticket ]                 [ QR / Direct IP: 7777 ]    │
│                                                                        │
│ PARTICIPANTS:                                                          │
│ • Host (You): Corp [Weyland Fast Advance]      [ READY ]               │
│ • Opponent:   Waiting for connection...        [ ---   ]               │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│          [ ABORT HOSTING ]             [ START GAME (Locked) ]         │
└────────────────────────────────────────────────────────────────────────┘
```

UI Wireframe 4: Joining a P2P Session

When the joining player clicks "Join Existing Game", they input the peer's connection string and lock in their own active decks.

```
┌────────────────────────────────────────────────────────────────────────┤
│ [← Back]  JOIN P2P SESSION                                             │
├────────────────────────────────────────────────────────────────────────┤
│ 1. ACTIVE DECKS:                                                       │
│    • Corp: [ NBN Controlling the Message ]  [ Change Deck ]            │
│    • Runner: [ Shaper Kate McCaffrey ]      [ Change Deck ]            │
│                                                                        │
│ 2. CONNECTION TARGET:                                                  │
│    Enter Peer IP, LAN Address, or Iroh Ticket String:                  │
│    ┌────────────────────────────────────────────────────────────────┐  │
│    │ iroh://7g8f9a2b3c4d5e6f7g8h9i0j1k2l3m4n5o6p...                 │  │
│    └────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│    Detected Protocol: Iroh Secure P2P Tunnel (Ready)                   │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│          [ CANCEL ]                    [ CONNECT & HANDSHAKE ]         │
└────────────────────────────────────────────────────────────────────────┘
```

---

# Deck Builder Layout

Thin Left column for Search and Filters
Most of space for Card Catalogue Grid
Deck stats

```
┌───────────────────────────────────────────────────────────────────────────┐
│ [← Exit to Hub]   DECKBUILDER: "Reina Roja - Anarch Eco"   [Save] [Export]│
├───────────────────────────┬───────────────────────────┬───────────────────┤
│ 1. SEARCH & FILTERS       │ 2. CARD CATALOGUE GRID    │ 3. LIVE DECK STATS│
│                           │                           │     & BREAKDOWN   │
│ Search: [ type card name] │ [All] [Prog] [Hard] [Ev]  │                   │
│ Format Pool: [ Standard ▼]│ ┌──────┐ ┌──────┐         │ IDENTITY:         │
│ Faction: [Anarch / Neut]  │ │ Card │ │ Card │         │ Reina Roja + image│
│ Sort: [Cost ▾] [Name]     │ │      │ │      │         │                   │
│                           │ └──────┘ └──────┘         │ LEGALITY:         │
│                           │ ┌──────┐ ┌──────┐         │ • Standard: ✔     │
│                           │ │ Card │ │ Card │         │ • Startup:  ✘     │
│                           │ │      │ │      │         │                   │
│                           │ └──────┘ └──────┘         │ METRICS:          │
│                           │                           │ Size: 45 / 45     │
│                           │ [Hover Card Preview]      │ Influence: 12/15  │
│                           │                           │                   │
│                           │                           │ CURRENT LIST:     │
│                           │                           │ • Datasucker [3]  │
│                           │                           │ • Djinn      [2]  │
│                           │                           │ • Corroder   [1]  │
│                           │                           │ • Medium     [2]  │
│                           │                           │ • Sure Gamble[3]  │
└───────────────────────────┴───────────────────────────┴───────────────────┘
```

## All Deck selection

Here is the wireframe mockup for the Deck Selection Modal, designed for both local bot matches and online queues. Instead of a plain text dropdown, this modal uses rich visual cards showcasing the identity, deck name, custom name, and faction styling.

```
┌────────────────────────────────────────────────────────────────────────┐
│ SELECT ACTIVE DECK (CORPORATION)                     [X Close / Back]  │
├────────────────────────────────────────────────────────────────────────┤
│ Filter Format: [ All Formats ▼ ]     Search: [ Filter by name... ]     │
├────────────────────────────────────────────────────────────────────────┤
│ ┌────────────────────────────────────────────────────────────────────┐ │
│ │ 🎴 [ID Card Art]    Weyland Consortium: Builder of Nations          │ │
│ │                    Custom Name: "Fast Advance Lockdown"            │ │
│ │                     Faction: Weyland (Green) | Format: STANDARD 🟢 │ │
│ │                    Status: Valid (49 Cards, 18 Agenda Points)      │ │
│ │                    [ SELECT THIS DECK ]                            │ │
│ └────────────────────────────────────────────────────────────────────┘ │
│                                                                        │
│ ┌────────────────────────────────────────────────────────────────────┐ │
│ │ 🎴 [ID Card Art]   NBN: Controlling the Message                     │ │
│ │                    Custom Name: "TAGstorm 2026"                    │ │
│ │                    Faction: NBN (Yellow) | Format: STARTUP 🟢      │ │
│ │                    Status: Valid (45 Cards, 20 Agenda Points)      │ │
│ │                    [ SELECT THIS DECK ]                            │ │
│ └────────────────────────────────────────────────────────────────────┘ │
│                                                                        │
│ ┌────────────────────────────────────────────────────────────────────┐ │
│ │ 🎴 [ID Card Art]   Haas-Bioroid: Engineering the Future             │ │
│ │                    Custom Name: "Bioroid Asset Spam"               │ │
│ │                    Faction: HB (Blue) | Format: ETERNAL ⚠️          │ │
│ │                    Status: Invalid (Contains 2 Rotated Cards)      │ │
│ │                    [ GRAYED OUT / UNSELECTABLE ]                   │ │
│ └────────────────────────────────────────────────────────────────────┘ │
├────────────────────────────────────────────────────────────────────────┤
│ [ + Create New Deck in Builder ]                  [ Confirm Selection] │
└────────────────────────────────────────────────────────────────────────┘
```

Key UI & Data Attributes Included
 * Identity Card Thumbnail: Displays a mini visual preview of the Corp or Runner identity card, giving instant visual recognition.
 * Custom Deck Name: Highlights the user's custom-labeled name (e.g., "Fast Advance Lockdown") alongside the official identity title.
 * Faction & Format Badges: Clear color-coded faction indicators and legality tags (Standard, Startup, Snapshot, Eternal) with green checkmarks or red warning icons.
 * Validation Guardrails: Invalid decks are visually dimmed and unselectable, with a brief explanation text showing why they fail (e.g., rotation or card count rules).

Immediate Next Steps
 * Implement this modal component as a reusable overlay triggered whenever a player clicks [ Change Corp Deck ] or [ Change Runner Deck ] from either the Main Dashboard or the local game setup screen.
 * Bind the [ + Create New Deck in Builder ] action button directly to your full-page deck builder route.

Profile Page can carry a way to configure, save "Server Profiles" - give them a name. A "Default" may be able to automatically connect to. Player can only be connect to one so they can "Disconnect" and select -> other profile and "Connect". This will also allow the user to pull data from the remote server to see and change (if possible).

```
┌────────────────────────────────────────────────────────────────────────┐
│ [← Back to Main Menu]    PROFILE & SERVER CONNECTIONS MANAGER          │
├────────────────────────────────────────────────────────────────────────┤
│ 1. LOCAL CLIENT IDENTITY & PATHS                                       │
│    • Key Pair: `~/.config/netrunner-rs/identity.key` (Ed25519 Active)  │
│    • Local Card Cache: Synced (v2.7.4)                                 │
│    • Local Bot Stats: 42 Wins / 12 Losses                              │
├────────────────────────────────────────────────────────────────────────┤
│ 2. SAVED REMOTE SERVERS & PROFILES                                     │
│                                                                        │
│  [◉] [ OFFICIAL COMMUNITY MAINNET (Default) ▼ ]  [ NEW ]               │
│      Address: `netrunner.rs.community:7777`                            │
│      Status: Connected (Ping: 24ms)                                    │
│      Server Profile Data:                                              │
│       • Active Title: [ "Net-Slinger" ]                                │
│       • Active Badge: [ Weyland Retro 06 ]                             │
│       • MMR / Rank: 1450 (Standard Ladder)                             │
│      [ DISCONNECT ] [ CONFIGURE PROFILE ] [ REMOVE ]                   │
│                                                                        │
│  [ ] LOCAL LAN TEST SERVER                                             │
│      Address: `192.168.1.50:7777`                                      │
│      Status: Offline                                                   │
│      [ CONNECT ] [ EDIT ]                                              │
│                                                                        │
├────────────────────────────────────────────────────────────────────────┤
│ [ + Add New Server Connection ]                                        │
└────────────────────────────────────────────────────────────────────────┘
```

Server side needs to own and keep secure in database and link up public-key to achievements, name, and other things for the game. Game DB backend should be a zero touch easy setup of SQLite but also be able to use a PostGres setup if a server actually becomes popular.

There probably needs to be a server admin client too - allow remote connections to setup tournaments, setup the low effort prizes for any of the server stuff.

---

**Said in conversation the same day, beside the notes:** Casual keeps its own path — a
global unranked queue, and "friend" queues a player sets up as a unique lobby and waits
in for their group, where a two-person queue simply plays each other. Every queue keeps
the sides balanced across the asynchronous play, so two people who meet again swap
seats. Bots sit only in Casual, and only when a lobby asks for one. A chat may come
later, optional, in Casual and in a friend lobby. No backwards compatibility is owed to
anything: nothing has been released.

**Later the same day, when the formats were built (Phase 4 §8):** told that no published
format admits every card — Eternal bans seven and holds a deck to a points budget — the
person kept the every-card format after all, and named it Unrestricted so that "casual"
means only the queue.
