"""§ fixture.generator := synthetic-only + actual.Apocv4.canonical.serializer."""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apocv4-src", type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.apocv4_src.resolve()))
    from apocv4.context import canonical_json_bytes, digest_json
    from apocv4.history_proof_codec import build_history_proof_bundle

    conversation = "11111111-1111-4111-8111-111111111111"
    request = "22222222-2222-4222-8222-222222222222"
    text = "A verified G12 response: café, 中文, 🧠, e\u0301, \u2028, \u2029, \\ and \"quoted\"."
    usage = {"prompt_tokens": 12, "completion_tokens": 7}
    model = {
        "evidence_lane": "model_reported_not_observed_fact",
        "model_id": "fixture/g12", "model_revision": "fixture-revision",
        "model_family": "fixture-family", "serving_profile_digest": "1" * 64,
        "response_id": "response-" + request, "prompt_digest": "2" * 64,
        "response_digest": "", "rationale_present": False, "rationale_digest": None,
        "token_admission_digest": None, "token_admission": None, "usage": usage,
    }
    response = {
        "schema_version": "apocv4.chat-response.v2", "text": text,
        "model_reported": model,
        "observed": {"evidence_lane": "observed_runtime_transport", "latency_ms": 4.25,
                     "transport_kind": "fixture", "transport_receipt_digest": None},
        "authority": {"effect_authority": "NONE", "tool_authority": "READ_ONLY_CONTEXT",
                      "memory_scope": "owner_partitioned_retrieval",
                      "conversation_history": "session_bounded", "training_consent": False},
        "identity": {"schema_version": "apocv4.identity.v1", "system_id": "apocrypha",
                     "architecture": "governed_hybrid_digital_intelligence",
                     "compiler_version": "g12-fixture", "identity_digest": "3" * 64,
                     "learned_model_role": "replaceable_faculty_not_system_identity",
                     "lineage": "g12-fixture-lineage"},
        "context": {"frame_id": "acf-g12-fixture", "frame_digest": "4" * 64,
                    "provenance_spine_digest": "5" * 64,
                    "retrieval": {"status": "EMPTY", "count": 0, "refs": []},
                    "memory": {"provider": "owner", "status": "EMPTY", "records_used": 0,
                               "receipt_digest": None, "refs": []}, "capabilities": []},
        "conversation_id": conversation, "request_id": request,
        "privacy_partition_ref": digest_json("owner:apocky"), "outcome": "completed",
        "learned_faculty_used": True,
        "duplicate_effect_protection": "not_applicable_no_effect_authority",
    }

    def history_body(chat):
        model = chat["model_reported"]
        response_core = {key: model[key] for key in (
            "model_id", "model_revision", "model_family", "serving_profile_digest",
            "response_id", "prompt_digest", "token_admission_digest", "rationale_digest", "usage",
        )}
        model["response_digest"] = digest_json({**response_core, "text": chat["text"]})
        core = {
            "schema_version": "apocv4.chat-history-visible-turn.v3", "state": "COMPLETED",
            "request_id": request, "conversation_id": conversation,
            "user_message": "Continue this worldline.", "assistant_message": chat["text"],
            "response": chat, "error_class": None, "failure_digest": None, "public_error": None,
            "token_admission_digest": None, "token_admission": None,
            "recorded_at": "2026-09-04T20:00:00.000Z", "response_digest": model["response_digest"],
            "terminal_receipt_digest": "6" * 64,
        }
        turn = {**core, "turn_digest": digest_json(core)}
        page_core = {
            "schema_version": "apocv4.chat-history-page.v1", "conversation_id": conversation,
            "turns": [turn], "next_cursor": None, "has_more": False,
            "persistence": "DURABLE_PRINCIPAL_BOUND", "effect_authority": "NONE",
        }
        page = {**page_core, "page_digest": digest_json(page_core)}
        envelope = {"schema_version": "apocv4.runtime-service.v1", "result": page}
        # § wire.order ≠ digest.order ; Python float tokens preserved @ actual.serializer.
        return json.dumps(envelope, ensure_ascii=False, allow_nan=False, separators=(",", ":"))

    fixtures = []
    for name, latency in (
        ("integral-float", 100.0), ("small-exponent", 1e-7), ("large-exponent", 1e16),
        ("negative-zero", -0.0), ("fixed-boundary", 0.0001), ("exponent-boundary", 1e-5),
        ("large-fixed", 1e15), ("subnormal", 5e-324), ("max-finite", 1.7976931348623157e308),
    ):
        chat = copy.deepcopy(response)
        chat["observed"]["latency_ms"] = latency
        fixtures.append({"name": name, "accepted": True, "body": history_body(chat)})

    nested = copy.deepcopy(response)
    nested["context"]["retrieval"] = {"status": "READY", "count": 1, "refs": [{
        "\U00010000": [100.0, -0.0, 1e-7, {"10": 1e16, "2": 4.25}],
        "\ue000": {"z": False, "a": "café\n🧠\t\u0000"},
        "__proto__": {"constructor": 0.0}, "01": None,
    }]}
    fixtures.append({"name": "nested-unicode-and-key-order", "accepted": True,
                     "body": history_body(nested)})
    for name, count, accepted in (
        ("bounded-integer-min", 0, True), ("bounded-integer-max", 1_000_000, True),
        ("bounded-integer-negative", -1, False), ("bounded-integer-overflow", 1_000_001, False),
        ("unsafe-integer", 9_007_199_254_740_993, False),
    ):
        chat = copy.deepcopy(response)
        chat["model_reported"]["usage"]["prompt_tokens"] = count
        fixtures.append({"name": name, "accepted": accepted, "body": history_body(chat)})

    for fixture in fixtures:
        if fixture["accepted"]:
            page = json.loads(fixture["body"])["result"]
            fixture["proof_body"] = canonical_json_bytes({
                "schema_version": "apocv4.runtime-service.v1",
                "result": build_history_proof_bundle(page),
            }).decode("utf-8")

    output = {
        "schema_version": "apocky.g12-python-history-fixtures.v1",
        "source": "apocv4.context.canonical_json_bytes + digest_json",
        "source_sha256": hashlib.sha256((args.apocv4_src / "apocv4/context.py").read_bytes()).hexdigest(),
        "proof_source_sha256": hashlib.sha256((args.apocv4_src / "apocv4/history_proof_codec.py").read_bytes()).hexdigest(),
        "fixtures": fixtures,
    }
    destination = Path(__file__).with_name("g12-python-history.json")
    destination.write_bytes(canonical_json_bytes(output) + b"\n")
    print(f"{len(fixtures)} synthetic fixtures written: {destination}")


if __name__ == "__main__":
    main()
