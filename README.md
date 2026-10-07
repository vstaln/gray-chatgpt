# gray-chatgpt

> **A [gray](https://github.com/vstaln/gray) plugin: Sign in with ChatGPT.** Run gray on your ChatGPT plan instead of an API key, through OpenAI's official open-source flow.

[gray](https://github.com/vstaln/gray) is a minimal, always-on AI agent harness by Vstalin Grady ([@vstaln](https://github.com/vstaln)), with its home at [gray.alignment.id](https://gray.alignment.id). This is a community plugin for it. It adds **ChatGPT plan** as a provider in gray's `/connect`, so the agent runs on the usage included in your ChatGPT subscription.

## How it works

`gray-chatgpt` is a gray provider sidecar (plugin protocol 1.2). It handles the sign-in only. Gray sends every request itself to OpenAI's public Responses API (`https://api.openai.com/v1`) with the OAuth access token, so gray's own tools, approvals and compaction stay in charge. Nothing goes through `chatgpt.com/backend-api` and nothing goes through the Codex CLI.

It follows OpenAI's [Sign in with ChatGPT for open-source apps](https://developers.openai.com/siwc/token-sharing-open-source) contract:

- **Dynamic client registration.** The first sign-in registers Gray with `client_id=dynamic_agent_client`. Later sign-ins reuse the issued `oaiapp_…` client, so there's no client secret and no partner API key.
- **Sign-in security.** PKCE (S256) with a fresh `state` and `nonce` per attempt, and a loopback callback on `127.0.0.1`.
- **ID-token check.** The RS256 signature is verified against OpenAI's JWKS, along with issuer, audience (the issued client), expiry and nonce.
- **Plan usage must be granted.** A sign-in that grants only identity, without the `chatgpt.tokens.use.direct` scope, is refused.
- **Refresh, revocation and models.** Rotating refresh tokens, revocation on sign-out, and the account's own model catalog (`GET /v1/models`).

After you approve, the browser lands on a small Gray-branded page served by the plugin itself. It's self-contained and loads nothing from the network.

## Requirements

- [gray](https://github.com/vstaln/gray) with the host fixes for short-lived tokens ([vstaln/gray#198](https://github.com/vstaln/gray/pull/198)). Without them, gray asks you to sign in again whenever the 1-hour access token has expired, instead of refreshing it.
- A ChatGPT **Plus or Pro** account.

## Install

Prebuilt binaries for macOS (Apple Silicon and Intel) and Linux (x86-64 and ARM64, static) are attached to every [release](https://github.com/DanielOu1208/gray-chatgpt/releases), each with a `.sha256`:

```sh
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64)  target=x86_64-unknown-linux-musl ;;
  Linux-aarch64) target=aarch64-unknown-linux-musl ;;
esac
curl -fsSLo ~/.local/bin/gray-chatgpt \
  "https://github.com/DanielOu1208/gray-chatgpt/releases/latest/download/gray-chatgpt-$target"
chmod +x ~/.local/bin/gray-chatgpt
gray plugin install chatgpt   # finds gray-chatgpt on PATH
```

Run the install from an interactive terminal and approve the `provider.credentials` capability. That's how gray lets a plugin offer a provider and hold its login.

To build from source instead: `cargo build --release`, then `gray plugin install ./target/release/gray-chatgpt`.

## Connect

In gray, run `/connect`, pick **ChatGPT plan**, and finish the browser sign-in. Approve both the app registration ("Gray") and plan usage. Then pick a model with `/model`.

## Usage limits

Requests count against your ChatGPT plan, under any per-app limit you set in [ChatGPT Settings → Usage](https://chatgpt.com/settings/usage). On Plus, the five-hour limit is shared across every app that uses your plan. Once a limit is hit, gray stops the turn instead of retrying.

## Accounts

The plugin keeps this install's host id and the issued client under `$GRAY_HOME/chatgpt/state.json` (owner-only). The tokens live in gray's own credential store.

```sh
gray-chatgpt status   # registration and account for this install
gray-chatgpt forget   # drop the registration; the next /connect registers again
```

To cut a registration off entirely, remove Gray under ChatGPT Settings → Apps.

## Develop

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

The tests mock OpenAI's endpoints, including a full sign-in with a signed ID token. `tests/sidecar_wire.rs` drives the real binary through gray's own sidecar client. `testdata/id-token-test-key.pem` is a throwaway key used only to sign test tokens.

## Credits

- **[gray](https://github.com/vstaln/gray)** by Vstalin Grady ([@vstaln](https://github.com/vstaln)): the agent harness, its plugin protocol, and the `gray-plugin` and `gray-core` crates this plugin builds on.
- The login plumbing (loopback callback, login operation manager, sidecar request loop) is adapted from his [gray-codex-sub](https://github.com/vstaln/gray-codex-sub) (MIT). His copyright notice is kept in [LICENSE](LICENSE).
- The Gray name and the mark on the sign-in page belong to the gray project.

This is a community plugin. It isn't affiliated with or endorsed by OpenAI, and ChatGPT is a trademark of OpenAI.

## License

MIT. See [LICENSE](LICENSE).
