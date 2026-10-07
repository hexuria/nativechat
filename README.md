# OpenGrok

**Infrastructure for persistent AI coworkers.**

OpenGrok is a native AI workspace built in Rust + GPUI for running persistent AI coworkers with their own:

- computers
- tools
- memory
- permissions
- credentials
- plugins
- skills
- schedules
- browser automation
- job-specific harnesses

The desktop is the interface.

The coworkers live on the server.

**Close the app. The work can keep running.**

---

## The Idea

Most AI products still treat the agent as something that lives inside a chat session.

OpenGrok treats the agent as a persistent worker.

```text
Traditional AI

Chat
 └── Model
      └── Temporary tools


OpenGrok

Coworker
 ├── Identity
 ├── Memory
 ├── Computer
 ├── Tools
 ├── Skills
 ├── Plugins
 ├── Credentials
 ├── Permissions
 ├── Schedules
 └── Harness
```

The UI can disappear.

The worker does not have to.

---

## Architecture

```text
                            ┌──────────────────────────┐
                            │        OpenGrok          │
                            │      Rust + GPUI         │
                            └────────────┬─────────────┘
                                         │
                                       AG-UI
                                         │
                                         ▼
                            ┌──────────────────────────┐
                            │     opengrok-server      │
                            │                          │
                            │  Durable Agent Harness   │
                            │  Memory                  │
                            │  Policies                │
                            │  Scheduler               │
                            │  Tools                   │
                            │  Skills                  │
                            │  Plugins                 │
                            │  MCP                     │
                            └───────┬──────┬──────┬────┘
                                    │      │      │
                    ┌───────────────┘      │      └────────────────┐
                    ▼                      ▼                       ▼
             ┌─────────────┐       ┌───────────────┐      ┌─────────────────┐
             │     Box     │       │ Plugin / Skill│      │ open-ai-gateway │
             │             │       │     Layer     │      │                 │
             │ Linux       │       │ Marketplace   │      │ Model Routing   │
             │ Terminal    │       │ Skills        │      │ Provider Auth   │
             │ Files       │       │ MCP Servers   │      │ Usage / Spend   │
             │ Chromium    │       │ Connectors    │      │ Model Pins      │
             │ CDP         │       └───────────────┘      └────────┬────────┘
             │ Computer Use│                                        │
             └──────┬──────┘                                        ▼
                    │                                         Model Providers
          ┌─────────┴─────────┐
          │                   │
          ▼                   ▼
 ┌────────────────┐   ┌────────────────┐
 │ Ultra-Instinct │   │     Allowly    │
 │                │   │                │
 │ Browser Agent  │   │ Human Approval │
 │ Observe        │   │ Remote Control │
 │ Decide         │   │ Hardware HID   │
 │ Gate           │   │ macOS Approval │
 │ Execute        │   └────────────────┘
 │ Verify         │
 └────────────────┘


        Cred-Swap protects sensitive information
        crossing model and tool boundaries.
```

OpenGrok Desktop does **not** communicate directly with model providers.

Model access, durable execution, agent state, computers, plugins, credentials, policies, tools, and scheduling live behind the server infrastructure.

---

# Core Infrastructure

| Repository | Role |
|---|---|
| [`hexuria/opengrok`](https://github.com/hexuria/opengrok) | Native Rust + GPUI interface for working with persistent AI coworkers |
| [`hexuria/opengrok-server`](https://github.com/hexuria/opengrok-server) | Durable agent runtime, harness, memory, policy, scheduling, tools, plugins, AG-UI and orchestration |
| [`hexuria/open-ai-gateway`](https://github.com/hexuria/open-ai-gateway) | Model routing, provider access, credentials, usage, organization keys and model selection |
| [`hexuria/box`](https://github.com/hexuria/box) | Sandboxed Linux computer for agents with terminal, files, Chromium, CDP and computer-use APIs |
| [`hexuria/gpui-agent`](https://github.com/hexuria/gpui-agent) | Semantic control plane for GPUI applications |
| [`hexuria/plugin-marketplace`](https://github.com/hexuria/plugin-marketplace) | Plugin registry for skills, MCP servers and external capabilities |
| [`hexuria/impeccable-skills`](https://github.com/hexuria/impeccable-skills) | Router for high-assurance language-specific engineering skills |
| [`hexuria/cred-swap`](https://github.com/hexuria/cred-swap) | Privacy boundary for credentials and sensitive data crossing model/tool boundaries |
| [`hexuria/ultra-instinct`](https://github.com/hexuria/ultra-instinct) | Rust-native browser-agent runtime with deterministic gating and verification |
| [`hexuria/allowly`](https://github.com/hexuria/allowly) | Human approval and hardware interaction path for actions software should not approve itself |

---

# Agent Computers

[`hexuria/box`](https://github.com/hexuria/box) gives a coworker its own Linux computer.

A Box can provide:

```text
Computer
 ├── Terminal
 ├── Filesystem
 ├── Chromium
 ├── Chrome DevTools Protocol
 ├── Screenshots
 ├── Computer Use
 ├── Persistent Browser Profile
 └── Controlled Network Egress
```

The computer belongs to the coworker runtime.

It does not belong to the desktop window.

That distinction matters.

Closing OpenGrok should not mean terminating the coworker's environment.

---

# Structured Control Surfaces

OpenGrok avoids depending entirely on screenshot → vision model → guessed coordinates.

The stack exposes structured control surfaces at different layers.

```text
                      AI Coworker
                           │
             ┌─────────────┼─────────────┐
             │             │             │
             ▼             ▼             ▼
        gpui-agent    Ultra-Instinct     Box
             │             │             │
             ▼             ▼             ▼
        Native GPUI      Browser         OS
                                         │
                                  Terminal / Files
```

## GPUI Agent

[`hexuria/gpui-agent`](https://github.com/hexuria/gpui-agent) gives agents programmatic control over native GPUI applications.

GPUI does not have a browser DOM.

Playwright and CDP therefore cannot simply attach to it.

Instead, applications embed an `AgentHost`, publish stable semantic IDs, and expose a local control protocol.

```text
AI Agent / MCP / CLI
        │
        ▼
    gpui-agent
        │
        ├── snapshot
        ├── screenshot
        ├── click
        ├── type
        ├── set-value
        ├── key
        ├── invoke
        └── assert
        │
        ▼
     AgentHost
        │
        ▼
   OpenGrok GPUI
```

It serves the same architectural purpose that CDP provides for browsers:

**structured machine control instead of blind coordinate guessing.**

It is not CDP-compatible and does not attach to arbitrary GPUI processes.

The application explicitly exposes the control surface.

### Perceive → Act → Verify

```text
Snapshot
   │
   ▼
Understand semantic state
   │
   ▼
Act on stable IDs
   │
   ▼
Assert resulting state
   │
   └──────► repeat
```

Example:

```sh
gpui-agent snapshot --pretty

gpui-agent click nav-settings

gpui-agent set-value search-input "query"

gpui-agent type composer "hello"

gpui-agent assert --id page-settings
```

Application-defined operations can also be exposed through `invoke`:

```sh
gpui-agent invoke prefs.set --arg theme=dark
```

This means OpenGrok itself can be operated and tested by agents.

---

# Browser Automation

[`hexuria/ultra-instinct`](https://github.com/hexuria/ultra-instinct) is the browser execution layer.

Instead of blindly replaying recorded coordinates, it works against the current browser state.

```text
Goal
 │
 ▼
Observe
 │
 ▼
Build Action Space
 │
 ▼
Decide / Abstain
 │
 ▼
Gate
 │
 ▼
Action Ticket
 │
 ▼
Execute
 │
 ▼
Observe Again
 │
 ▼
Verify Effect
```

Ultra-Instinct combines browser state such as DOM and accessibility information with execution gates and post-action verification.

An action that cannot be justified can abstain instead of guessing.

### Native vs Browser Control

```text
OpenGrok Native UI

GPUI Semantic Tree
       │
       ▼
   gpui-agent
       │
       ▼
OpenGrok


Browser

DOM + Accessibility
       │
       ▼
 Ultra-Instinct
       │
       ▼
   Chromium / CDP
```

`gpui-agent` controls native OpenGrok surfaces.

Ultra-Instinct controls browser surfaces.

Box provides the underlying computer.

---

# Plugins

OpenGrok uses [`hexuria/plugin-marketplace`](https://github.com/hexuria/plugin-marketplace) as its default plugin registry.

A plugin packages capabilities that can be attached to a coworker without baking them permanently into the core runtime.

The plugin format can contain:

```text
Plugin
 ├── Skills
 ├── MCP Servers
 ├── Commands
 ├── Agents
 ├── Hooks
 └── Language Servers
```

OpenGrok only activates the components its runtime safely supports.

Unsupported components should remain visible as unsupported rather than silently executing.

## Pinned Sources

Remote plugin sources are pinned to exact Git commits.

```text
github.com/vendor/plugin
        +
40-character commit SHA
```

A plugin therefore does not silently become different code because somebody moved a branch.

OpenGrok snapshots installed plugin content.

A marketplace update does not automatically rewrite an existing installation.

## Account-Owned Plugins

Plugin installations belong to an account.

Installing a plugin does not automatically enable it for every coworker.

```text
Install
   │
   ▼
Account Plugin Registry
   │
   ▼
Disabled for Coworker
   │
   ▼
Explicitly Enable
   │
   ▼
Capabilities Available
```

Plugin credentials are stored separately from plugin code.

Installed skills are namespaced:

```text
plugin-name.skill-name
```

Skills and plugins remain subject to the coworker's existing policy and capability ceiling.

A plugin cannot grant itself permission.

---

# Skills

Skills tell a coworker **how** to perform work.

Tools determine **what** the coworker can actually execute.

```text
Skill
  │
  │ instructions
  ▼
Agent Harness
  │
  │ policy
  ▼
Allowed Tools / Computer / MCP
```

Skills can be:

- attached to a coworker
- selected for a single turn
- installed through a plugin
- shared across an organization
- packaged with supporting files

A skill is not a permission boundary.

Instructions cannot create a tool, credential, computer, or permission that was not already granted.

---

# Impeccable Skills

[`hexuria/impeccable-skills`](https://github.com/hexuria/impeccable-skills) provides a language-aware verification router.

Instead of requiring the user to remember individual skill names:

```text
impeccable-rust
impeccable-python
```

the user can simply say:

```text
impeccable this

make this faster

verify this properly

harden this

audit this
```

The router identifies the language from repository evidence and delegates the work.

```text
               impeccable
                   │
            Detect Language
                   │
          ┌────────┴────────┐
          ▼                 ▼
 impeccable-rust     impeccable-python
```

Current members:

| Skill | Repository | Focus |
|---|---|---|
| Impeccable Rust | [`hexuria/impeccable-rust`](https://github.com/hexuria/impeccable-rust) | High-assurance Rust development, optimization and verification |
| Impeccable Python | [`hexuria/impeccable-python`](https://github.com/hexuria/impeccable-python) | High-assurance Python development, optimization and verification |

## Impeccable Rust

Impeccable Rust treats:

```text
cargo test
```

as the beginning of verification rather than the end.

Depending on the failure mode it can guide an agent toward techniques and tooling such as:

```text
Miri
Sanitizers
Loom
Kani
Proptest
Fuzzing
Mutation Testing
TLA+
Stateright
Creusot
Verus
Lean
Semver Checks
Supply-Chain Auditing
Benchmark Verification
Differential Testing
```

The goal is not to make vague claims that code is "verified."

The goal is to state precisely:

```text
What was checked?
Under what assumptions?
Under what bounds?
What remains unchecked?
```

## Impeccable Python

The Python counterpart applies the same philosophy to Python systems.

Its verification surface includes techniques and tools such as:

```text
pytest
Hypothesis
CrossHair
Atheris
Mutation Testing
Strict Typing
Free-Threaded CPython
ASan / UBSan
Valgrind
Differential Testing
API Compatibility
Dependency Auditing
Benchmark Verification
```

For mixed Python/Rust systems, the Rust component can be handed to Impeccable Rust instead of pretending Python tooling verifies Rust.

---

# Credential Protection

[`hexuria/cred-swap`](https://github.com/hexuria/cred-swap) protects sensitive information before it leaves the trusted environment.

```text
Real Data
   │
   ▼
Cred-Swap
   │
   ▼
Shape-Preserving Stand-ins
   │
   ▼
Model
   │
   ▼
Restore
   │
   ▼
Real Data
```

Cred-Swap can recognize structured sensitive information including:

- API keys
- cloud credentials
- private keys
- database URLs
- email addresses
- payment information
- IP addresses
- other structured secrets

Detection is deterministic.

A model is not required to discover structured credentials.

## Agent Boundary

Agent systems require more than simple request/response masking.

```text
Tool Result
    │
    ▼
   Scrub
    │
    ▼
   Model
    │
    ▼
 Tool Call
    │
    ▼
  Restore
    │
    ▼
Real Execution
```

The model can reason using safe stand-ins.

The actual tool still receives the real credential, hostname, database URL, or other value immediately before execution.

---

# Human Approval

[`hexuria/allowly`](https://github.com/hexuria/allowly) handles operations where software should not be allowed to approve itself.

```text
Coworker requests action
          │
          ▼
        Policy
          │
      ┌───┴─────────┐
      │             │
      ▼             ▼
   Allowed     Human Required
      │             │
      ▼             ▼
   Execute        Allowly
                    │
                    ▼
                   Human
```

Allowly can expose the Mac's state and approval interaction to a trusted phone.

For macOS permission dialogs that intentionally reject synthetic software clicks, Allowly can use a physical USB HID device.

To macOS, the click is therefore a real hardware input.

The goal is not to bypass the approval boundary.

The goal is to let an autonomous system **reach a human without weakening that boundary**.

---

# Model Gateway

[`hexuria/open-ai-gateway`](https://github.com/hexuria/open-ai-gateway) sits between OpenGrok and model providers.

```text
OpenGrok Server
      │
      ▼
open-ai-gateway
      │
      ├── Provider Routing
      ├── Organization Keys
      ├── Model Selection
      ├── Usage
      ├── Spend
      └── Provider Accounts
      │
      ▼
Model Providers
```

The desktop does not need direct access to provider credentials.

Provider access remains behind the server infrastructure.

---

# Durable Runtime

[`hexuria/opengrok-server`](https://github.com/hexuria/opengrok-server) is where the coworkers actually live.

It owns the durable runtime:

```text
opengrok-server
 ├── Agent Harness
 ├── Conversations
 ├── Memory
 ├── Tools
 ├── Policies
 ├── Computers
 ├── Plugins
 ├── Skills
 ├── MCP
 ├── Scheduler
 ├── Monitoring
 ├── Accounts
 ├── Organizations
 └── AG-UI
```

A desktop client is only one window onto that state.

```text
Desktop closes

     ✕

Agent runtime

     ✓
```

---

# Coming Soon

## Portable Agent Toolchain

Agents can already install software inside their computers.

But software installed into a disposable machine still belongs too much to that machine.

Destroy the computer and the coworker may have to rebuild its working environment.

The **Portable Agent Toolchain** separates the agent's software environment from the compute running it.

```text
                 AI Coworker
                     │
          ┌──────────┼──────────┐
          │          │          │
       Identity    Memory    Permissions
                     │
                   Skills
                     │
                     ▼
          Portable Agent Toolchain
                     │
        ┌────────────┼────────────┐
        │            │            │
        ▼            ▼            ▼
      CLIs        Runtimes    DB Clients
        │            │            │
        └────────────┴────────────┘
                     │
            ┌────────┼────────┐
            ▼        ▼        ▼
          Box A    Box B    Box C
```

Build the environment once.

Attach it to another compatible computer.

Instead of rebuilding the machine around the agent, the agent brings its environment with it.

The target model is:

```text
Identity        persists
Memory          persists
Skills          persist
Credentials     persist
Policies        persist
Toolchain       persists

Compute         replaceable
```

**The environment belongs to the agent, not the sandbox.**

---

# Run OpenGrok

OpenGrok currently expects the server infrastructure to be running.

## 1. Start Open AI Gateway

Follow the setup instructions in:

[`hexuria/open-ai-gateway`](https://github.com/hexuria/open-ai-gateway)

For the local development setup:

```sh
cd ../open-ai-gateway
just dev
```

## 2. Start OpenGrok Server

```sh
cd ../opengrok-server
scripts/serve.sh
```

Verify it:

```sh
curl -fsS http://127.0.0.1:1447/health
```

## 3. Run OpenGrok

```sh
just run
```

Use another OpenGrok server port:

```sh
just run 1448
```

Or run directly:

```sh
OPENGROK_BASE_URL=http://127.0.0.1:1447 \
cargo run
```

---

# Create the First Account

Signup is currently invite/admin controlled.

Create the initial organization and administrator against the same database used by the running OpenGrok server:

```sh
opengrok admin org create \
  --name OpenGrok \
  --admin-email you@opengrok.local \
  --domain opengrok.local \
  --password 'choose-8+'
```

Then sign in through OpenGrok.

Sessions are persisted locally until explicitly signed out.

---

# Agent-Controlled OpenGrok

OpenGrok can expose its native GPUI interface through `gpui-agent`.

Start OpenGrok with the embedded control surface enabled:

```sh
GPUI_AGENT=1 \
GPUI_AGENT_TOKEN=dev-secret \
GPUI_AGENT_SCREENSHOT_DIR=/tmp/opengrok-agent \
cargo run --features agent
```

Then configure the same token for the controlling process:

```sh
export GPUI_AGENT_TOKEN=dev-secret
```

Wait for OpenGrok:

```sh
gpui-agent wait
```

Inspect the semantic interface:

```sh
gpui-agent snapshot --pretty
```

Interact with a stable element:

```sh
gpui-agent click <stable-id>
```

Type into a control:

```sh
gpui-agent type <stable-id> "hello"
```

Verify the resulting state:

```sh
gpui-agent assert --id <expected-id>
```

`gpui-agent` can also run through MCP so development agents can inspect, operate, and verify OpenGrok directly.

---

# Execution Surfaces

The OpenGrok ecosystem is deliberately layered.

```text
                         Coworker
                            │
            ┌───────────────┼───────────────┐
            │               │               │
            ▼               ▼               ▼

       Native Apps       Web Apps        Computer
            │               │               │
            ▼               ▼               ▼
       gpui-agent      Ultra-Instinct       Box
            │               │               │
            ▼               ▼               ▼
       Semantic UI       DOM / CDP      Shell / Files
```

When an operation requires human consent:

```text
Coworker
   │
   ▼
 Policy
   │
   ▼
Allowly
   │
   ▼
 Human
```

When information crosses a model boundary:

```text
Sensitive Data
      │
      ▼
  Cred-Swap
      │
      ▼
    Model
```

When the coworker needs additional capabilities:

```text
Plugin Marketplace
       │
       ├── Skills
       ├── MCP Servers
       └── Connectors
       │
       ▼
     Coworker
```

---

# Design Principles

### The desktop is a window

Do not put durable agent state in the GUI.

### Agents own computers

The execution environment belongs to the coworker runtime.

### Skills are not permissions

Instructions never override policy.

### Plugins are capabilities, not trust

Installation and authorization are separate decisions.

### Credentials should not casually reach models

Protect sensitive information at model and tool boundaries.

### Prefer structured control over pixels

Use semantic trees, CDP, APIs, tools, and stable IDs before reaching for coordinate-based automation.

### Humans remain part of the permission boundary

Some actions should require consent.

### Verify effects, not just actions

A successful click is not proof that the intended task succeeded.

### Compute should become replaceable

Identity, memory, skills, credentials, policy, and eventually the software environment should survive the machine executing them.

---

# Contributing

Contributions are welcome.

If you want to improve OpenGrok, fix a bug, add a capability, or work on one of the surrounding infrastructure projects, open an issue or submit a pull request.

Please keep changes focused, testable, and aligned with the architecture of the project.

For larger changes, open an issue first so the design can be discussed before implementation.

## Project

OpenGrok is a project of [Goldcoders](https://goldcoders.dev/).

Created by **Uriah Galang**  
X: [@codeitlikemiley](https://x.com/codeitlikemiley)



## License

See the repository license for usage, modification, and distribution terms.

Third-party dependencies, plugins, skills, models, and external integrations remain subject to their respective licenses and terms.
