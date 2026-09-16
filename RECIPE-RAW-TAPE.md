# The raw tape has to travel with the detail

The Recipes page can now draw a raw version's tape: the `v1 raw` tab renders every event in the
same fixed-height scrolling table the other tabs use, one row per event, reading

```
#   Event     Details                    At
1   down      (640, 60) button 1         +0.00 s
2   up        (640, 60) button 1         +0.12 s
3   move      (700, 120)                 +0.38 s
4   wheel     (640, 400) by (0, -120)    +1.20 s
5   keydown   "e"                        +2.05 s
6   keyup     Return                     +2.31 s
```

It cannot show any of that yet, because the detail route strips the tape to a count on the way
out. Until the server change below lands, a raw tab falls back to what it said before — how many
events were taped, when, and the sentence about a tape being kept as it was taken. Nothing else
is needed on the app side: `RecipeVersionBody::events` (`src/opengrok/client.rs`) already reads
either shape.

## The change

**File:** `crates/opengrok-server/src/recipes.rs`
**Function:** `detail_body` — the `"body"` of each version in the `"versions"` array.

Today a raw version's body becomes its size:

```rust
"body": if version.kind == "raw" {
    json!({ "events": version.body.as_array().map(|events| events.len()).unwrap_or(0) })
} else {
    version.body.clone()
},
```

It should become the tape itself, up to a cap, and its size above that cap:

```rust
/// The most tape events one detail carries. A tape is capped at 5 MB on the way in, which is
/// still tens of thousands of pointer moves; a longer one travels as its count, the way every
/// tape did before.
const RAW_EVENTS_SENT: usize = 2_000;

// …in detail_body:
"body": if version.kind == "raw" {
    let events = version.body.as_array().cloned().unwrap_or_default();
    if events.len() <= RAW_EVENTS_SENT {
        json!({ "events": events })
    } else {
        json!({ "events": events.len() })
    }
} else {
    version.body.clone()
},
```

So `events` is either an array of `TapeEvent` (`crates/opengrok-recipes/src/lib.rs`) or a number.
The app takes both: `RecipeTape::Events` and `RecipeTape::Count`, with any third shape parsed as
`RecipeTape::Other` so one odd body cannot fail a whole detail.

## Why the cap matters

The detail route is what the page loads on every open, every rename, every grant and after every
run — the app re-reads the whole detail each time. A tape is capped at 5 MB when it is taught, and
most of that is `move` events: a minute of teaching is easily tens of thousands of them. Sending
all of them would mean a multi-megabyte JSON body on a request the page makes constantly, and a
table the app would have to lay out tens of thousands of rows for.

2,000 events is a long but readable tape and a body of a few hundred kilobytes. Above it, the count
alone is the honest answer: the page says how much was taped and does not pretend to list it. The
whole tape stays in the store either way — the cap is about what travels, not about what is kept —
so a `filter` on the server still sees every event.

## Alternative already in a server checkout

`/Volumes/goldcoders/OSS/opengrok-server` currently sends a third shape from this same place:

```rust
json!({ "events": total, "tape": shown, "truncated": total > RAW_EVENTS_SENT })
```

That keeps `events` a count and puts the tape under a separate `tape` key. The app does **not**
read `tape`; it reads the tape under `events`. If that shape is the one that ships, either move the
events onto `events` as above, or `RecipeVersionBody` needs a `tape: Vec<RecipeTapeEvent>` field
and `RecipeVersion::tape_events` needs to prefer it.
