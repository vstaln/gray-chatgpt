# gray-chatgpt

> **Sign in with ChatGPT for the [gray](https://github.com/vstaln/gray) agent harness.** Run gray on your ChatGPT plan instead of an API key, through OpenAI's official open-source flow.

`gray-chatgpt` is a protocol-1.2 provider sidecar. It handles the sign-in only. Gray sends every request itself to OpenAI's public Responses API (`https://api.openai.com/v1`) with the OAuth access token, so gray's own tools, approvals and compaction stay in charge. Nothing goes through `chatgpt.com/backend-api` and nothing goes through the Codex CLI.

It follows OpenAI's [Sign in with ChatGPT for open-source apps](https://developers.openai.com/siwc/token-sharing-open-source) contract:

- **Dynamic client registration.** The first sign-in registers Gray with `client_id=dynamic_agent_client`. Later sign-ins reuse the issued `oaiapp_…` client, so there's no client secret and no partner API key.
- **Sign-in security.** PKCE (S256) with a fresh `state` and `nonce` per attempt, and a loopback callback on `127.0.0.1`.
- **ID-token check.** The RS256 signature is verified against OpenAI's JWKS, along with issuer, audience (the issued client), expiry and nonce.
- **Plan usage must be granted.** A sign-in that grants only identity, without the `chatgpt.tokens.use.direct` scope, is refused.
- **Refresh, revocation and models.** Rotating refresh tokens, revocation on sign-out, and the account's own model catalog (`GET /v1/models`).

## Requirements

- A ChatGPT **Plus or Pro** account.
- gray with the host fixes for short-lived tokens ([vstaln/gray#198](https://github.com/vstaln/gray/pull/198)). Without them, gray asks you to sign in again whenever the 1-hour access token has expired, instead of refreshing it.

## Build and install

From a checkout of this repository:

```sh
cargo build --release
gray plugin install ./target/release/gray-chatgpt
```

Run the install from an interactive terminal and approve the `provider.credentials` capability.

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

## License

MIT
