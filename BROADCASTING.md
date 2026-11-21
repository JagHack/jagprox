# Live Game Broadcasting

Let a friend watch your Hypixel game live, through your proxy, with Hypixel only
ever seeing you. Your JagProx acts as a tiny Minecraft server built from the game
state it already tracks; your friend's JagProx connects to it and swaps their
client over from Hypixel to your feed.

## Using it

**Broadcast your game** (while connected to Hypixel through JagProx):

- `/bc on` — start broadcasting. Tell your friend your Minecraft name.
- When they ask to join you'll get a clickable **§a[Accept]** / **§c[Deny]** line
  in chat (or use `/bc accept <name>` / `/bc deny <name>`).
- `/bc status` — how many people are watching.
- `/bc off` — stop broadcasting and disconnect spectators.

Set `auto_broadcast: true` in `config.yml` to start broadcasting automatically on
login, so a friend can `/watch` you without you running `/bc on` each game. It's
off by default; while off, `/bc on`/`/bc off` remind you it exists.

**Watch a friend:**

- `/watch <their name>` — JagProx disconnects you with a message; **reconnect** and
  you'll spawn into their game as an invisible, flying spectator.
- Fly around freely; you can't interact with anything.
- `/unwatch` then reconnect to go back to playing on Hypixel normally.

Nothing about your Hypixel session changes for the broadcaster — Hypixel sees one
player (you). Spectators never touch Hypixel at all.

## How it works

```
 Hypixel ── broadcaster's client ── broadcaster's JagProx ──┐
                                      (tracks the world)     │  relay (compass)
                                                             │
          spectator's client ── spectator's JagProx ─────────┘
```

1. **Rendezvous (compass).** Both proxies connect to the JagProx rendezvous server.
   The broadcaster registers by name; a spectator asks to join by that name; the
   broadcaster approves. The server then hands both sides a one-time token and pipes
   their two data connections together. (This is a relay, like iroh's relay fallback;
   direct peer-to-peer hole-punching is a planned enhancement.)

2. **World tracking.** While broadcasting, the proxy feeds every packet Hypixel sends
   into a `GameState` tracker (`crates/jagprox-core/src/proxy/broadcast/state.rs`):
   chunks + a per-column block-change overlay, every entity's position/metadata/gear,
   the tab list, the scoreboard, time, world border and block entities. Packets that
   are only ever replayed keep their raw bytes; only the key/position fields are
   decoded.

3. **Host side.** For each spectator the broadcaster's proxy acts as a Minecraft
   server over the relay: logs them in (adventure mode, flying), sends the world
   snapshot and spawns the broadcaster as a player model, then streams live packets.
   Packets about the broadcaster *as a player* (health, inventory, position
   corrections, abilities) are dropped so they can't corrupt the spectator's client;
   the broadcaster is animated from their own outgoing movement instead.

4. **Spectator side.** The spectator's proxy dials the broadcaster over the relay,
   logs in, and forwards frames both ways between its own Minecraft client and the
   feed.

## Running the rendezvous server

The server is the `jagprox-compass` crate — a small standalone TCP server.

```
COMPASS_ADDR=0.0.0.0:7676 cargo run -p jagprox-compass --release
```

Proxies default to the JagProx broker at `77.42.31.200:7676`. It's a raw IP, not
`jagprox.jaghack.com`, because the broker speaks plain TCP and that hostname is
Cloudflare-proxied (HTTP/HTTPS only). Point proxies at your own broker with a
`compass:` line in `config.yml`:

```yaml
compass: my-server.example.com:7676
```

The server never sees Minecraft credentials — it only brokers names and pipes bytes.

## Current limitations (v1)

- Transport is a relay through the rendezvous server, not direct peer-to-peer.
- Spectating starts on a reconnect (JagProx kicks you with a prompt), rather than an
  instant in-place swap.
- Entities are placed by absolute teleport each tick (robust, slightly chattier than
  relative-move deltas).
- Maps/particles reach spectators live but aren't replayed in a late-join snapshot.
