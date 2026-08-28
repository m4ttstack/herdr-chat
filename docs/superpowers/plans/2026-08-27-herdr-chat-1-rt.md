# herdr-chat, part 1: `rt pane send` (repo-tools)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the one rt verb the herdr-chat plugin's broadcast needs: `rt pane send <pane> --text <text>`, which injects arbitrary text into one Claude pane and reports delivery. Extract the injection core from the invite feature's `chat:invite` so both verbs share one implementation.

**Architecture:** The invite feature already implements the herdr injection delivery model (blocked is refused, working is queued, otherwise prompt then a single Enter nudge then queued) inside `chat:invite` in `lib/daemon/handlers/chat.ts`. This plan lifts that logic into a standalone `lib/herdr/inject.ts` helper, re-points `chat:invite` at it (its tests stay green), then builds `pane:send` as a second, thinner caller that delivers a caller-supplied multi-line string instead of a fixed slash command. The verb is cataloged in rt-client, handled in `lib/daemon/handlers/pane.ts`, routed, exposed as `rt pane send`, and wrapped as `paneSend()`.

**Tech Stack:** Bun (`Bun.connect` unix sockets, `bun:test`, `bun:sqlite`), TypeScript, the rt daemon's typed handler contract (`TypedHandlers`), rt-client.

**Spec:** `docs/superpowers/specs/2026-08-27-herdr-chat-design.md` (the herdr-chat repo), section "The rt addition: `rt pane send`". Builds on `2026-08-26-rt-chat-invite-design.md` and its part-1 plan.

## Global Constraints

- **This plan executes only after the invite feature (`spec/rt-chat-invite`) has merged to `main`.** It refactors invite's `chat:invite`; the exact line anchors in Task 1 are read from the merged code, not guessed. Confirm `lib/herdr/client.ts`, `lib/daemon/handlers/chat.ts`'s `chat:invite`, and `lib/daemon/handlers/pane.ts` exist on `main` before starting.
- Work in a fresh repo-tools worktree off `main` (branch `spec/herdr-chat-rt`), never on repo-tools' main checkout. This plan doc lives in the herdr-chat repo; the code it describes lives in repo-tools.
- **No sync-exec on the daemon thread (MAT-222).** Every herdr call is the async socket client (`lib/herdr/client.ts`); no `execSync`/`spawnSync` under `lib/daemon/` or `lib/herdr/`.
- The herdr socket path and `herdr unavailable` string (exactly `herdr unavailable`, optionally `: <detail>`) are inherited from `lib/herdr/client.ts`; do not restate them.
- Every cataloged verb needs, in the same task: the `Commands` entry, the `COMMAND_NAMES` entry, the handler, and the router registration; `lib/daemon/__tests__/rt-client-commands.test.ts` fails otherwise.
- Types rt-client cannot import are duplicated in `packages/rt-client/src/commands.ts` with the existing "Duplicated shape on purpose" comment convention.
- Tests: `bun test <path>` per file while working; `bun test lib commands packages scripts` before every commit.
- After touching `lib/command-tree-def.ts`, run `bun scripts/gen-docs.ts` and commit the regenerated `website/docs/reference` (`bun scripts/check-docs.ts` gates it).
- After touching anything under `packages/rt-client/src`, run `cd packages/rt-client && bun run build` (the dist-freshness test).
- No em dashes or en dashes in code, comments, docs, or commit messages. Comments only for constraints the code cannot show.
- Commit after every task with a short imperative message.

---

### Task 1: extract the injection core into `lib/herdr/inject.ts`

Lift the delivery logic out of `chat:invite` so `pane:send` can share it. `chat:invite`'s behavior and its tests must not change.

**Files:**
- Create: `lib/herdr/inject.ts`
- Modify: `lib/daemon/handlers/chat.ts` (the `chat:invite` handler: replace its inline delivery with a call to the helper)
- Test: `lib/herdr/__tests__/inject.test.ts`

**Interfaces:**
- Produces:

```ts
export type InjectDelivery = "accepted" | "queued" | "refused";
export interface InjectResult { paneId: string; delivered: InjectDelivery; reason?: string }

export interface InjectOptions {
  paneId: string;
  text: string;                 // one line for a slash command; may be multi-line for a prompt
  callerPane?: string;          // HERDR_PANE_ID of the caller; a match is refused
  herdr?: typeof herdrRequest;  // test seam
  promptWaitMs?: number;        // default 5000
}

/**
 * herdr's injection delivery, shared by chat:invite and pane:send.
 * agent.get first: blocked is refused, working is queued (no wait), else
 * agent.prompt with a wait until working, one Enter nudge on a stall, then queued.
 */
export function injectIntoPane(opts: InjectOptions): Promise<InjectResult>;
```

- Consumes: `herdrRequest`, `herdrError`/`HERDR_UNAVAILABLE` from `lib/herdr/client.ts`.

- [ ] **Step 1: Read the merged `chat:invite` delivery block**

Open `lib/daemon/handlers/chat.ts` and locate the `chat:invite` handler. Identify the block that: calls `agent.get`, branches on `blocked`/`working`, does the `agent.prompt` with a `wait`, sends the single Enter nudge on a stall, and builds the `{ paneId, delivered, reason }` result. That block is what moves. Note its exact line range for the commit.

- [ ] **Step 2: Write the failing helper tests**

Create `lib/herdr/__tests__/inject.test.ts`, driving the helper against the existing `fakeHerdr` (`lib/herdr/__tests__/fake-herdr.ts`):

```ts
import { afterEach, expect, test } from "bun:test";
import { herdrRequest } from "../client.ts";
import { fakeHerdr, HerdrFakeError, type FakeHerdrHandler } from "./fake-herdr.ts";
import { injectIntoPane } from "../inject.ts";

const stops: Array<() => void> = [];
afterEach(() => { for (const s of stops) s(); stops.length = 0; });

function on(handler: FakeHerdrHandler) {
  const { sock, seen, stop } = fakeHerdr(handler);
  stops.push(stop);
  const herdr: typeof herdrRequest = (m, p, o) => herdrRequest(m, p, { ...o, sockPath: sock });
  return { seen, herdr };
}

test("an idle pane accepts: prompt sent, reached working", async () => {
  const { herdr, seen } = on((method) => {
    if (method === "agent.get") return { type: "agent", agent: { pane_id: "w1:p1", status: "idle" } };
    if (method === "agent.prompt") return { type: "agent", agent: { pane_id: "w1:p1", status: "working" } };
    return new HerdrFakeError("invalid_request", method);
  });
  const res = await injectIntoPane({ paneId: "w1:p1", text: "do the thing", herdr });
  expect(res).toEqual({ paneId: "w1:p1", delivered: "accepted" });
  expect(seen.map((s) => s.method)).toContain("agent.prompt");
});

test("a blocked pane is refused, nothing sent", async () => {
  const { herdr, seen } = on((method) =>
    method === "agent.get" ? { type: "agent", agent: { pane_id: "w1:p1", status: "blocked" } } : new HerdrFakeError("invalid_request", method));
  const res = await injectIntoPane({ paneId: "w1:p1", text: "hi", herdr });
  expect(res).toEqual({ paneId: "w1:p1", delivered: "refused", reason: "at a prompt" });
  expect(seen.map((s) => s.method)).not.toContain("agent.prompt");
});

test("a working pane is queued: prompt sent without a wait", async () => {
  const { herdr } = on((method) => {
    if (method === "agent.get") return { type: "agent", agent: { pane_id: "w1:p1", status: "working" } };
    if (method === "agent.prompt") return { type: "agent", agent: { pane_id: "w1:p1", status: "working" } };
    return new HerdrFakeError("invalid_request", method);
  });
  const res = await injectIntoPane({ paneId: "w1:p1", text: "later", herdr });
  expect(res).toEqual({ paneId: "w1:p1", delivered: "queued" });
});

test("a stall after the prompt gets one Enter nudge, then queued", async () => {
  let prompts = 0;
  const { herdr, seen } = on((method) => {
    if (method === "agent.get") return { type: "agent", agent: { pane_id: "w1:p1", status: "idle" } };
    if (method === "agent.prompt") { prompts++; return { type: "agent", agent: { pane_id: "w1:p1", status: "idle" } }; }
    if (method === "pane.send_input") return { type: "ok" };
    return new HerdrFakeError("invalid_request", method);
  });
  const res = await injectIntoPane({ paneId: "w1:p1", text: "x", herdr, promptWaitMs: 20 });
  expect(res.delivered).toBe("queued");
  expect(seen.map((s) => s.method)).toContain("pane.send_input");
  expect(prompts).toBe(1);
});

test("the caller's own pane is refused before any herdr call", async () => {
  const { herdr, seen } = on(() => new HerdrFakeError("invalid_request", "unreachable in this test"));
  const res = await injectIntoPane({ paneId: "w1:p1", text: "x", callerPane: "w1:p1", herdr });
  expect(res).toEqual({ paneId: "w1:p1", delivered: "refused", reason: "that is this pane" });
  expect(seen).toHaveLength(0);
});

test("multi-line text is delivered verbatim as the prompt", async () => {
  const { herdr, seen } = on((method) =>
    method === "agent.get" ? { type: "agent", agent: { pane_id: "w1:p1", status: "idle" } }
    : method === "agent.prompt" ? { type: "agent", agent: { pane_id: "w1:p1", status: "working" } }
    : new HerdrFakeError("invalid_request", method));
  await injectIntoPane({ paneId: "w1:p1", text: "line one\nline two", herdr });
  const prompt = seen.find((s) => s.method === "agent.prompt")!;
  expect(prompt.params.text).toBe("line one\nline two");
});
```

- [ ] **Step 3: Run to verify failure**

Run: `bun test lib/herdr/__tests__/inject.test.ts`
Expected: FAIL, `Cannot find module "../inject.ts"`.

- [ ] **Step 4: Move the logic into `lib/herdr/inject.ts`**

Create `lib/herdr/inject.ts` with the delivery block cut from `chat:invite`, generalized so the injected string is `opts.text` (not a hardcoded `/chat:join`). Preserve the exact status branching, the `wait` shape, the single `pane.send_input` Enter nudge, and the reason strings (`"at a prompt"`, `"that is this pane"`, `"not a claude pane"` if `chat:invite` checked that). The caller-pane guard runs first, before any herdr call. Use `herdrError` from the client for herdr-unavailable and error passthrough.

- [ ] **Step 5: Re-point `chat:invite` at the helper**

In `lib/daemon/handlers/chat.ts`, delete the moved block and call `injectIntoPane({ paneId, text: joinLine, callerPane, herdr })`, where `joinLine` is the `/chat:join <room> [note ...]` string `chat:invite` already builds. Keep `chat:invite`'s payload, its `from`/note assembly, and its result shape unchanged.

- [ ] **Step 6: Run both suites**

Run: `bun test lib/herdr/__tests__/inject.test.ts lib/daemon/__tests__/chat-handlers.test.ts`
Expected: the new inject suite passes and the existing `chat:invite` tests still pass unchanged. If a `chat:invite` test asserted an exact internal call order that the extraction reordered, the delivery is identical; update only the assertion, never the behavior.

- [ ] **Step 7: Commit**

```bash
git add lib/herdr/inject.ts lib/herdr/__tests__/inject.test.ts lib/daemon/handlers/chat.ts
git commit -m "herdr: extract injectIntoPane, the delivery core shared by chat:invite and pane:send"
```

---

### Task 2: the `pane:send` daemon verb

**Files:**
- Modify: `packages/rt-client/src/commands.ts` (the `InjectResult` type, one `Commands` entry, `COMMAND_NAMES`)
- Modify: `lib/daemon/handlers/pane.ts` (the `pane:send` handler)
- Modify: `lib/daemon/command-router.ts` if the pane factory's return type widened (usually already spread)
- Test: `lib/daemon/__tests__/pane-handlers.test.ts`

**Interfaces:**
- Produces (rt-client):

```ts
/** Duplicated shape on purpose: mirrors lib/herdr/inject.ts's InjectResult. */
export type PaneDelivery = "accepted" | "queued" | "refused";
export interface PaneSendResult { paneId: string; delivered: PaneDelivery; reason?: string }
  "pane:send": { payload: { paneId: string; text: string; callerPane?: string }; data: PaneSendResult };
```

- Consumes: `injectIntoPane` (Task 1); the existing `createPaneHandlers` factory and its `herdr` seam.

- [ ] **Step 1: Add the catalog entry**

In `packages/rt-client/src/commands.ts`, add the `PaneDelivery`/`PaneSendResult` types near the other pane types, the `"pane:send"` entry in `Commands`, and `"pane:send"` in `COMMAND_NAMES`.

Run: `bun test lib/daemon/__tests__/rt-client-commands.test.ts`
Expected: FAIL: the exhaustiveness test names `pane:send` as a catalog entry with no handler. Steps 2 to 4 satisfy it.

- [ ] **Step 2: Write the failing handler tests**

Append to `lib/daemon/__tests__/pane-handlers.test.ts`:

```ts
test("pane:send delivers caller text and returns accepted", async () => {
  const { pane } = harness((method) =>
    method === "agent.get" ? { type: "agent", agent: { pane_id: "w1:p1", status: "idle" } }
    : method === "agent.prompt" ? { type: "agent", agent: { pane_id: "w1:p1", status: "working" } }
    : new HerdrFakeError("invalid_request", method));
  const res = await pane["pane:send"]({ paneId: "w1:p1", text: "broadcast: standup in 5" });
  expect(res).toEqual({ ok: true, data: { paneId: "w1:p1", delivered: "accepted" } });
});

test("pane:send refuses the caller's own pane", async () => {
  const { pane } = harness(() => new HerdrFakeError("invalid_request", "unused"));
  const res = await pane["pane:send"]({ paneId: "w1:p1", text: "x", callerPane: "w1:p1" });
  expect(res).toEqual({ ok: true, data: { paneId: "w1:p1", delivered: "refused", reason: "that is this pane" } });
});

test("pane:send is herdr unavailable when the socket is missing", async () => {
  const db = freshDb();
  const herdr: typeof herdrRequest = (m, p, o) => herdrRequest(m, p, { ...o, sockPath: join(tmpdir(), "absent-herdr.sock") });
  const pane = createPaneHandlers({ db, repoIndex: () => ({}), herdr });
  const res = await pane["pane:send"]({ paneId: "w1:p1", text: "x" });
  expect(res.ok).toBe(false);
  if (res.ok) throw new Error("unreachable");
  expect(res.error.startsWith(HERDR_UNAVAILABLE)).toBe(true);
});
```

- [ ] **Step 3: Run to verify failure**

Run: `bun test lib/daemon/__tests__/pane-handlers.test.ts`
Expected: FAIL, `pane["pane:send"] is not a function`.

- [ ] **Step 4: Implement the handler**

In `lib/daemon/handlers/pane.ts`: import `injectIntoPane` from `../../herdr/inject.ts`, widen the factory's return type to include `"pane:send"`, and add:

```ts
    "pane:send": async (payload: Commands["pane:send"]["payload"]): Promise<CommandResult<"pane:send">> => {
      const result = await injectIntoPane({
        paneId: payload.paneId,
        text: payload.text,
        callerPane: payload.callerPane,
        herdr,
      });
      return { ok: true, data: result };
    },
```

`injectIntoPane` maps a missing socket to a `herdr unavailable` throw-free result; if the invite implementation returns herdr-unavailable as an `InjectResult` rather than an error, mirror `chat:invite`'s choice here so the two verbs agree. (Check how `chat:invite` surfaces herdr-unavailable and match it exactly.)

- [ ] **Step 5: Run the tests**

Run: `bun test lib/daemon/__tests__/pane-handlers.test.ts lib/daemon/__tests__/rt-client-commands.test.ts`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add packages/rt-client/src/commands.ts lib/daemon/handlers/pane.ts lib/daemon/command-router.ts lib/daemon/__tests__/pane-handlers.test.ts
git commit -m "daemon: pane:send, arbitrary text injection over the shared delivery core"
```

---

### Task 3: the `rt pane send` CLI

**Files:**
- Modify: `commands/pane.ts` (the `pane` group from the invite feature; add the `send` subcommand)
- Modify: `lib/command-tree-def.ts` (register `pane send` for docs)
- Test: `commands/__tests__/pane.test.ts`

**Interfaces:**
- Consumes: `paneSend()` from rt-client (Task 4 finalizes the wrapper; the CLI can call the cataloged verb directly through the existing client the other `pane` subcommands use).
- Behavior: `rt pane send <pane> --text <text>`; `--text -` reads the body from stdin (so multi-line briefs pipe in); forwards `process.env.HERDR_PANE_ID` as `callerPane`; prints the delivery line and exits non-zero only on a daemon/herdr failure, not on `refused` (a refused pane is a reported outcome, matching `chat:invite`).

- [ ] **Step 1: Write the failing CLI test**

Append to `commands/__tests__/pane.test.ts` (mock the client the way the other `pane` subcommand tests do):

```ts
test("rt pane send forwards text and HERDR_PANE_ID as callerPane", async () => {
  const calls: any[] = [];
  const client = fakeClient({ "pane:send": (p: any) => { calls.push(p); return { ok: true, data: { paneId: p.paneId, delivered: "accepted" } }; } });
  await runPane(["send", "w1:p2", "--text", "standup in 5"], { client, env: { HERDR_PANE_ID: "w1:p1" } });
  expect(calls[0]).toEqual({ paneId: "w1:p2", text: "standup in 5", callerPane: "w1:p1" });
});

test("rt pane send prints the delivery outcome and does not exit non-zero on refused", async () => {
  const client = fakeClient({ "pane:send": () => ({ ok: true, data: { paneId: "w1:p2", delivered: "refused", reason: "at a prompt" } }) });
  const { stdout, code } = await runPane(["send", "w1:p2", "--text", "x"], { client });
  expect(stdout).toContain("refused");
  expect(stdout).toContain("at a prompt");
  expect(code).toBe(0);
});
```

(Reuse `fakeClient` / `runPane` helpers already in `commands/__tests__/pane.test.ts` from the invite feature; if their names differ, match the existing ones.)

- [ ] **Step 2: Run to verify failure**

Run: `bun test commands/__tests__/pane.test.ts`
Expected: FAIL, `unknown subcommand "send"`.

- [ ] **Step 3: Implement the subcommand**

In `commands/pane.ts`, add a `send` subcommand beside `list`/`peek`/`spawn`: parse `<pane>` and `--text` (with `-` meaning read stdin to end), read `HERDR_PANE_ID` from env into `callerPane`, call the `pane:send` verb, and print `<paneId> <delivered>[ (<reason>)]`. Do not set a non-zero exit for `refused`.

- [ ] **Step 4: Register for docs**

In `lib/command-tree-def.ts`, add the `pane send` node (positional `pane`, flag `--text`), mirroring the neighboring `pane` entries.

- [ ] **Step 5: Run tests and regenerate docs**

Run: `bun test commands/__tests__/pane.test.ts`
Expected: PASS.
Run: `bun scripts/gen-docs.ts && bun scripts/check-docs.ts`
Expected: docs regenerate clean.

- [ ] **Step 6: Commit**

```bash
git add commands/pane.ts lib/command-tree-def.ts commands/__tests__/pane.test.ts website/docs/reference
git commit -m "cli: rt pane send, inject text into a pane; regen docs"
```

---

### Task 4: the `paneSend()` rt-client wrapper

**Files:**
- Modify: `packages/rt-client/src/index.ts` (or wherever the invite feature added `paneList`/`chatInvite` wrappers)
- Test: `packages/rt-client/test/pane-send.test.ts`

**Interfaces:**
- Produces: `paneSend(args: { paneId: string; text: string; callerPane?: string }, opts?: { timeoutMs?: number }): Promise<Result<PaneSendResult>>`, default `timeoutMs: 30_000` (matching `chatInvite`, since a working target can hold the connection through the prompt wait).

- [ ] **Step 1: Write the failing wrapper test**

Create `packages/rt-client/test/pane-send.test.ts`, modeled on the invite feature's `chat-invite` wrapper test (fake transport asserting the verb name, payload, and the 30s timeout override).

```ts
import { expect, test } from "bun:test";
import { makeFakeClient } from "./helpers.ts";
import { paneSend } from "../src/index.ts";

test("paneSend calls pane:send with a 30s timeout and returns the result", async () => {
  const { client, calls } = makeFakeClient({ "pane:send": { paneId: "w1:p2", delivered: "queued" } });
  const res = await paneSend.call(client, { paneId: "w1:p2", text: "hi" });
  expect(res).toEqual({ ok: true, data: { paneId: "w1:p2", delivered: "queued" } });
  expect(calls[0]).toMatchObject({ name: "pane:send", payload: { paneId: "w1:p2", text: "hi" }, timeoutMs: 30_000 });
});
```

(Match the invite feature's actual client/test-helper shape; if wrappers are methods on a client class rather than free functions, follow that.)

- [ ] **Step 2: Run to verify failure**

Run: `cd packages/rt-client && bun test test/pane-send.test.ts`
Expected: FAIL, `paneSend is not exported`.

- [ ] **Step 3: Implement the wrapper**

Add `paneSend` beside the invite feature's `paneList`/`chatInvite`, passing `timeoutMs: 30_000` by default, overridable via `opts`.

- [ ] **Step 4: Run tests and build**

Run: `cd packages/rt-client && bun test test/pane-send.test.ts && bun run build`
Expected: PASS and a clean build (the dist-freshness test).

- [ ] **Step 5: Commit**

```bash
git add packages/rt-client/src packages/rt-client/test/pane-send.test.ts packages/rt-client/dist
git commit -m "rt-client: paneSend wrapper with a 30s timeout"
```

---

### Task 5: document `rt pane send` in `rt:chat`

**Files:**
- Modify: `skills/rt-chat/SKILL.md` (the `rt pane` group section the invite feature added)

- [ ] **Step 1: Add the verb**

In `skills/rt-chat/SKILL.md`, add `rt pane send <pane> --text <text>` to the `rt pane` group (beside `list`, `peek`, `spawn`): one line describing that it injects text into a pane and reports `accepted` / `queued` / `refused`, and that a working pane queues the text until its turn ends. Note it is the primitive the herdr-chat plugin's broadcast uses.

- [ ] **Step 2: Commit**

```bash
git add skills/rt-chat/SKILL.md
git commit -m "docs(rt:chat): rt pane send in the pane group"
```

---

## Self-review

- **Spec coverage:** the spec's "The rt addition: `rt pane send`" section is covered by Tasks 1 (shared helper + refactor), 2 (verb), 3 (CLI), 4 (client), 5 (docs). The scrub of `HERDR_PANE_ID` is the plugin's responsibility (part 2); this side only honors `callerPane` when present, which Task 2 and Task 3 do.
- **Placeholders:** none. Task 1's exact cut anchors and Task 3/4's helper names are read from the merged invite code by design, since this plan runs after invite merges (see Global Constraints).
- **Type consistency:** `InjectResult` (inject.ts) and `PaneSendResult` (rt-client) carry the same `{ paneId, delivered, reason? }` shape; `delivered` is `"accepted" | "queued" | "refused"` in both. `paneSend`'s payload matches the `pane:send` catalog payload exactly (`paneId`, `text`, `callerPane?`).

## Delivery note

This is part 1 of two. Part 2 (`2026-08-27-herdr-chat-2-plugin.md`) is the plugin itself and consumes `rt pane send` as a shell command, not as imported code, so it does not depend on this plan's internal names, only on the CLI contract `rt pane send <pane> --text <text>` printing a delivery line.
