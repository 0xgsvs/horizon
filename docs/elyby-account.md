# Ely.by accounts and offline names

This fork adds a second account provider beside Microsoft's
([microsoft-account.md](microsoft-account.md)): the game binary and the launcher
sign in with an [Ely.by](https://ely.by) account through its
Yggdrasil-compatible authserver. The game binary can also play under a plain
offline name.

Code: `crates/horizon-client-render/src/elyby.rs` (the authserver client and the
kept account), `auth.rs` (`Provider`, the session join, the server's check), and
`crates/horizon-client/src/game/launch_options.rs` (`Options::read`).

## The launcher

On first start the launcher asks **Microsoft account** or **Ely.by account**.
Ely.by needs no Microsoft account and does not go through Microsoft's owner gate.
Settings > Account lists the Ely.by accounts with Play as and Sign out, and
**Add account** opens the same sign-in.

Server access is unchanged: a Microsoft account is still needed to download
Minecraft from Mojang or to play a Java instance. Ely.by plays an installed
Horizon build prepared from the player's own files.

## Who plays as whom

Decided at start, first match wins:

1. `--session-file` (the launcher's Microsoft account, `user_type` `msa`, or
   `elyby`).
2. `--elyby-login LOGIN`: sign in now and keep the account.
3. The account kept by an earlier `--elyby-login`, checked again at start.
4. Offline: `--name`, `HORIZON_PLAYER_NAME`, else the name kept in
   `horizon-player.txt`.

`--offline` skips 2 and 3. The dedicated server (`--dedicated`) never signs in.

```sh
horizon-client --elyby-login you@example.com     # prompts for the password (and the 2FA code)
horizon-client --server host:25565               # later starts: signed in as the kept account
horizon-client --offline --name Steve --server host:25565
horizon-client --elyby-logout                    # invalidates the token and forgets the account
```

`HORIZON_ELYBY_PASSWORD` and `HORIZON_ELYBY_CODE` answer the prompts (scripts).
The prompt shows what you type; pipe the password in if that matters.

An offline name gets vanilla's offline UUID (`UUID.nameUUIDFromBytes("OfflinePlayer:<name>")`),
so the same name is the same player (and inventory) on every offline-mode
server.

## The kept account

`horizon-elyby.json` in the player's folder (the game folder, or `--game-dir`
when given): name, UUID, access token and client token, never the password. It is
written to a new file only its owner can read (mode 0600 on Unix) and moved into
place, and `.gitignore` lists it. The launcher keeps its Microsoft tokens in the
OS keychain; the game binary has no keychain dependency, so this one is a file.

At start the token goes through `POST /auth/validate`; a refused one is renewed
with `POST /auth/refresh` and the file is rewritten. No network keeps the token
as it is (joining then says the authentication servers are unreachable). A token
that can't be renewed leaves the game offline with a note on stderr;
`--elyby-login` signs in again.

## Joining servers

Online-mode servers send an encryption request. A client signed in with Ely.by
joins through `POST {authserver}/session/join` (`accessToken`, `selectedProfile`
as the undashed UUID, `serverId` the server hash), then encrypts the connection
as for Microsoft.

A Horizon server in online mode (`online-mode=true`) asks the services listed in
`HORIZON_AUTH_PROVIDERS`, a comma list tried in order: `microsoft` (default) and
`elyby`, e.g. `HORIZON_AUTH_PROVIDERS=microsoft,elyby`. The first that knows the
join (`GET {authserver}/session/hasJoined?username=&serverId=`) gives the
profile. Ely.by and Microsoft names are separate namespaces, so with both on,
two players can hold one name with different UUIDs: keep whitelists and ops on
UUIDs. `online-mode=false` stays offline-mode for everyone.

`HORIZON_ELYBY_URL` replaces `https://authserver.ely.by` (a mock in tests).

## Endpoints used

`/auth/authenticate` (the 2FA code goes after the password, `password:code`),
`/auth/refresh`, `/auth/validate`, `/auth/invalidate`, `/session/join`,
`/session/hasJoined`. Reference: https://docs.ely.by/en/minecraft-auth.html

## Not done

- No title-screen UI: in the game, sign-in and the offline name are command-line only.
- Ely.by skins: `skins.rs` fetches only from `textures.minecraft.net`, so Ely.by
  players show the default skin.
- The "online mode" refusal text still names only the Microsoft account.
- `server.toml` has no provider setting; use the environment variable.
