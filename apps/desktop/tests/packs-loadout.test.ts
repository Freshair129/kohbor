import assert from "node:assert/strict";
import { test } from "node:test";
import { loadoutRequest, moveEquipped, parsePackLoadout, toggleEquipped } from "../src/packs/loadoutModel.ts";

const pack = (slot:number, packageId=`voice.pack${slot}`) => ({
  install_id: `ins_${String(slot).padStart(10,"0")}`,
  package_id: packageId, revision: 1, container_sha256: String(slot).repeat(64),
});
const loadout = { generation: 4, equipped: [pack(1),pack(2)], event_locks: { hero_death:"voice.pack2" }, match_consistency:false };

test("parses the host loadout without inventing pending state", () => {
  const parsed = parsePackLoadout({ schema_version:"1.0.0", loadout, pending:null, pending_request_id:null, max_equipped:4 });
  assert.deepEqual(parsed?.loadout, loadout);
  assert.equal(parsed?.pending, null);
  assert.equal(parsePackLoadout({ schema_version:"1.0.0", loadout:{...loadout,equipped:[pack(1),pack(2,"voice.pack1")]}, pending:null, pending_request_id:null, max_equipped:4 }), null);
});

test("equip toggles append in priority order and reject a fifth or duplicate package", () => {
  assert.deepEqual(toggleEquipped(loadout, { installId:pack(3).install_id, packageId:"voice.pack3" }), [pack(1).install_id,pack(2).install_id,pack(3).install_id]);
  assert.deepEqual(toggleEquipped(loadout, { installId:pack(2).install_id, packageId:"voice.pack2" }), [pack(1).install_id]);
  const four = { ...loadout, equipped:[pack(1),pack(2),pack(3),pack(4)] };
  assert.equal(toggleEquipped(four, { installId:pack(5).install_id, packageId:"voice.pack5" }), null);
  assert.equal(toggleEquipped(loadout, { installId:pack(3,"voice.pack2").install_id, packageId:"voice.pack2" }), null);
});

test("priority moves preserve other loadout preferences and requests use the accepted schema", () => {
  assert.deepEqual(moveEquipped(loadout,"ins_0000000002",-1),["ins_0000000002","ins_0000000001"]);
  assert.equal(moveEquipped(loadout,"ins_0000000001",-1),null);
  assert.deepEqual(loadoutRequest("req_12345678",["ins_0000000002"],loadout), {
    schema_version:"1.0.0", request_id:"req_12345678", install_ids:["ins_0000000002"],
    event_locks:{hero_death:"voice.pack2"}, match_consistency:false, policy:"after_match",
  });
});
