import { strict as assert } from "node:assert";
import { test } from "node:test";
import * as providerModel from "../src/analytics/provider-model.mjs";

import {
  availableModels,
  canEnableAi,
  endpointClassLabel,
  isLoopbackEndpoint,
  providerErrorKind,
  shouldOfferSaveAsNote,
  visibleProviderRows,
} from "../src/analytics/provider-model.mjs";

const providers = [
  { id: "anthropic", verified: false },
  { id: "openai", verified: true },
];

test("provider picker hides untested providers by default and exposes them only on request", () => {
  assert.deepEqual(visibleProviderRows(providers, false).map((row) => row.id), ["openai"]);
  assert.deepEqual(visibleProviderRows(providers, true).map((row) => row.id), ["anthropic", "openai"]);
});

test("loopback disclosure appears only for a loopback provider endpoint", () => {
  const local = { loopback: true, endpoint: "http://127.0.0.1:11434" };
  const external = { loopback: false, endpoint: "https://api.example.test" };
  const inconsistent = { loopback: true, endpoint: "https://api.example.test" };
  assert.equal(isLoopbackEndpoint(local), true);
  assert.equal(endpointClassLabel(local, "on this computer"), "on this computer");
  assert.equal(endpointClassLabel(external, "on this computer"), "");
  assert.equal(endpointClassLabel(inconsistent, "on this computer"), "");
  assert.equal(endpointClassLabel({ loopback: true, endpoint: "http://localhost.evil.test" }, "local"), "");
});

test("changing a provider endpoint is detected before chat history can transfer", () => {
  assert.equal(providerModel.shouldWarnForEndpointChange?.("http://127.0.0.1:11434/v1", "http://127.0.0.1:11434/v1"), false);
  assert.equal(providerModel.shouldWarnForEndpointChange?.("http://127.0.0.1:11434/v1", "http://127.0.0.1:4000/v1"), true);
});

test("Anthropic keeps a closed list while other providers accept known and fetched IDs", () => {
  assert.deepEqual(availableModels("anthropic", ["unexpected"], ["typed"]), [
    "claude-opus-5-5",
    "claude-sonnet-5-5",
    "claude-haiku-5-5",
  ]);
  assert.deepEqual(availableModels("openai", ["gpt-a"], ["gpt-b", "gpt-a"]), ["gpt-a", "gpt-b"]);
});

test("AI cannot be enabled until model, consent and any required key are present", () => {
  assert.equal(canEnableAi({ model: "gpt-a", consent: true, requiresKey: true, keySet: true }), true);
  assert.equal(canEnableAi({ model: "gpt-a", consent: true, requiresKey: true, keySet: false }), false);
  assert.equal(canEnableAi({ model: "gpt-a", consent: true, requiresKey: false, keySet: false }), true);
  assert.equal(canEnableAi({ model: "", consent: true, requiresKey: false, keySet: false }), false);
  assert.equal(canEnableAi({ model: "gpt-a", consent: false, requiresKey: false, keySet: false }), false);
});

test("active match has a distinct blocked state and save-as-note needs a user action", () => {
  assert.equal(providerErrorKind("match_active"), "match_active");
  assert.equal(providerErrorKind("consent_required"), "consent_required");
  assert.equal(providerErrorKind("secret key_required"), "key_required");
  assert.equal(shouldOfferSaveAsNote({ answer: "reply", savedNoteId: null, noteFallback: true }), true);
  assert.equal(shouldOfferSaveAsNote({ answer: "reply", savedNoteId: null, noteFallback: false }), false);
  assert.equal(shouldOfferSaveAsNote({ answer: "reply", savedNoteId: "note.md", noteFallback: true }), false);
});
