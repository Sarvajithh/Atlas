# Atlas prompt-quality & latency audit -- summary of changes

**Important caveat, read first:** this sandbox only has Rust 1.75 available
(via `apt`), and the project's real dependency tree (Tauri 2.x and its
transitive deps) requires a modern edition-2024-capable toolchain to compile.
I was not able to run `cargo build`/`cargo test` against this project here.
Every change below was written and then manually re-read line-by-line against
the actual struct/trait definitions in the repo (field names, types, trait
methods, error-type variants) to catch the kind of mistakes a compiler would
normally catch -- and I did find and fix two real errors that way (an
`AppError` `{e}` format that doesn't compile since `AppError` has no `Display`
impl, and an `ErrorCategory::System` that should be `ErrorCategory::SystemError`).
That said, **please run `cargo build` yourself before trusting this**; I have
higher confidence than an untested guess, but this is not compiler-verified.

---

## 1. Root cause analysis

**Prompt quality ("just citing answers"):** `PromptBuilder::build` took only
`context` (the retrieved chunks) -- the user's actual question, and any
system instruction, never reached the model. Ollama received a bare,
unlabeled dump of numbered chunks and nothing else, so it had no task to
perform beyond continuing/summarizing that text -- which reads exactly like
"just citing."

**Latency:**
- `num_ctx` was requested at the model's *full* advertised context window
  (e.g. 131072) on every call regardless of actual prompt size, forcing a
  KV-cache allocation sized for the maximum rather than what was needed --
  on an 8GB card this pushed total runtime memory past free VRAM and caused
  a silent CPU fallback (confirmed live via `ollama ps` showing
  `100% CPU`).
- No `keep_alive` was sent, so Ollama fell back to its own 5-minute default
  and reloaded the model from disk on every request that came after even a
  short gap.
- Retrieval truncated to the caller's final `limit` immediately after the
  hybrid merge, before reranking ever ran -- so the reranker could never
  promote a chunk that scored just outside the initial cut.
- `assistant_ask_stream` was a synchronous Tauri command that blocked for
  the full duration of retrieval + generation (confirmed up to ~225s in
  logs), which is consistent with the reported "can't click anything while
  it's thinking."

## 2. Files modified

- `app/src-tauri/crates/atlas-models/src/prompt_builder.rs`
- `app/src-tauri/crates/atlas-models/src/context_builder.rs`
- `app/src-tauri/crates/atlas-models/src/retriever.rs`
- `app/src-tauri/crates/atlas-models/src/scheduler.rs`
- `app/src-tauri/crates/atlas-models/src/ollama.rs`
- `app/src-tauri/crates/atlas-core/src/facade.rs`
- `app/src-tauri/crates/app-tauri/src/commands/assistant.rs`

Full diff: `CHANGES.diff` in this delivery. Full modified source tree:
`Atlas/` in this delivery.

## 3. Functions modified

- `PromptBuilder::build` -- now `build(&self, query: &str, context: AssembledContext)`, assembles a structured SYSTEM / WORKSPACE CONTEXT / USER QUESTION / ANSWER prompt instead of raw concatenation. New `system_prompt()` helper reads a settings-driven template with a documented fallback default.
- `ContextBuilder::assemble` -- now also calls two new private methods, `remove_near_duplicates` and `merge_adjacent`, between dedup and final ordering.
- `ContextBuilder::remove_near_duplicates` (new) -- drops chunks whose normalized text overlaps an already-kept chunk above `NEAR_DUPLICATE_OVERLAP_THRESHOLD` (0.9).
- `ContextBuilder::merge_adjacent` (new) -- combines consecutive same-document chunks into one passage.
- `Retriever::retrieve` -- no longer truncates to the caller's `limit` immediately; returns a wider candidate pool (`limit * CANDIDATE_POOL_MULTIPLIER`) so `ContextBuilder`'s reranker can actually reorder/promote candidates.
- `GenerateOptions::for_model_context` (ollama.rs) -- now takes `prompt: &str` too; sizes `num_ctx` to what the prompt actually needs (word-count proxy + `num_predict`, floored at `MIN_NUM_CTX`, capped at the model's real window) instead of always requesting the full window.
- `GenerateRequest` (ollama.rs) -- new `keep_alive` field, set to `OLLAMA_KEEP_ALIVE` ("30m") on every request.
- `AppFacade::chat_stream` (facade.rs) -- passes `message` into `PromptBuilder::build`; captures per-stage durations into named variables and logs one consolidated `[Timing Report]` line before returning.
- `AppFacade::search` (facade.rs) -- passes `query` into `PromptBuilder::build`.
- `ModelScheduler::execute` (scheduler.rs) -- passes `query` into `PromptBuilder::build`.
- `assistant_ask_stream` (commands/assistant.rs) -- converted from a synchronous `fn` to `async fn`; the actual blocking work (`facade.chat_stream`) now runs inside `tauri::async_runtime::spawn_blocking`, taking `app_handle: AppHandle` instead of `State<'_, AppFacade>` so the facade can be re-resolved inside the spawned closure (`AppFacade` isn't `Clone`).

## 4. Why each change was necessary

See the doc comments added directly above each change in the source --
every one explains the specific bug/gap it closes, not just what it does.
Kept there rather than duplicated here so the reasoning stays attached to
the code it explains.

## 5. Benchmark before/after

I can't produce real numbers without running this against your Ollama
instance -- I don't have GPU/Ollama access in this sandbox. What I can give
you is the *predicted* effect of each change, and exactly what to check to
confirm it:

| Stage | Before (from your logs) | Expected after | What to check |
|---|---|---|---|
| Model load / connection establishment | 180-225s (CPU fallback) | Should drop sharply once `num_ctx` sizing + GPU offload actually fits in 8GB VRAM | `ollama ps` mid-request should show `100% GPU`, not `100% CPU` |
| Repeat-request load | Same 180-225s every turn | Near-zero after the first request in a session | `ollama ps`'s `UNTIL` column should show a real future time after `keep_alive` |
| First token | Indistinguishable from total (buffered) | Target 3-5s once the above are fixed | New `[Timing Report]` line's `first_token` field |
| Retrieval | ~0.6-11s (highly variable, likely memory pressure) | More consistent once VRAM pressure from oversized `num_ctx` is gone | `[Timing Report]`'s `retrieval` field |
| Prompt build | <3ms | Still <3ms (string formatting, not I/O-bound) | `[Timing Report]`'s `prompt_build` field |

Please run a few real requests and paste me the new `[Timing Report]` lines --
I can tell you immediately whether each target is being hit.

## 6. Sample generated prompt

Hand-assembled using the new `PromptBuilder::build` logic, for
`query = "What is differential privacy?"` against two retrieved chunks:

```
SYSTEM

You are Atlas, an expert AI tutor.
Treat the retrieved workspace context below as authoritative course material and your primary source.
However, you are NOT limited to it: whenever the workspace context does not fully answer the question, use your own well-established general knowledge to explain the concept -- never contradict the workspace, but do supplement it freely for facts, history, or definitions that are well established and simply weren't retrieved.
Never simply repeat or quote retrieved passages back verbatim. Teach naturally: explain ideas, give intuition, connect related concepts, compare similar topics, and provide examples where they help understanding.
Cite workspace evidence inline using [1], [2], ... matching the numbered context below, wherever you actually draw on it. If your answer comes entirely from general knowledge rather than the workspace context, say so plainly.

---

WORKSPACE CONTEXT

[1] Differential privacy is a mathematical framework for quantifying the privacy loss when releasing statistics about a dataset. It works by adding calibrated random noise to query results.

[2] The privacy budget, denoted epsilon, controls the tradeoff between privacy and accuracy: smaller epsilon means stronger privacy guarantees but noisier results.

---

USER QUESTION

What is differential privacy?

---

ANSWER

Begin naturally.
```

## 7. Sample answer demonstrating the new behaviour

This is illustrative -- what I'd *expect* a competent instruction-following
8B model to produce given the prompt above (I did not run this against a
live model):

> Differential privacy is a way of sharing useful information about a
> dataset -- like averages or counts -- without revealing whether any single
> person's data was included in it [1]. The core trick is adding a small,
> carefully calibrated amount of random noise to the result before it's
> released, so an individual record's presence or absence barely changes
> what comes out.
>
> The key knob here is the privacy budget, usually written as epsilon (ε)
> [2]. Think of it like a dial: turn epsilon down and you get stronger
> privacy guarantees, but the results get noisier and less precise; turn it
> up and you get more accurate numbers at the cost of weaker privacy. This
> is a genuine tradeoff, not a bug -- there's no free lunch between privacy
> and accuracy in this framework.
>
> It's worth contrasting this with an older idea, k-anonymity, which tries
> to protect privacy by making sure every individual "blends in" with at
> least k-1 other similar records, rather than by adding noise -- k-anonymity
> is easier to reason about but has known weaknesses (e.g. it doesn't
> protect against an attacker who has outside knowledge about the dataset)
> that differential privacy was specifically designed to avoid. This
> comparison isn't in your workspace material, so take it as general
> background rather than something cited from your notes.

Note the shape: it explains and connects ideas rather than quoting [1]/[2]
verbatim, and it clearly marks the one piece of outside knowledge (the
k-anonymity comparison) as not coming from the workspace -- exactly the
behavior Part 7 asked for.
