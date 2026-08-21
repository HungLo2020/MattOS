import json
import glob
from collections import defaultdict

files = glob.glob(
    "/home/matt/.codex/sessions/**/rollout-*.jsonl",
    recursive=True
)

totals = defaultdict(lambda: {
    "sessions": 0,
    "input": 0,
    "cached": 0,
    "output": 0,
    "reasoning": 0,
    "total": 0,
})

unknown = 0

for path in files:
    latest = None
    model = None
    cwd = None

    try:
        with open(path, "r", encoding="utf-8") as f:
            for line in f:
                try:
                    obj = json.loads(line)
                except Exception:
                    continue

                # Search session metadata / turn context for cwd and model.
                text = line

                if cwd is None:
                    # Cheap but robust extraction from arbitrary JSON nesting.
                    if '"cwd"' in text:
                        def find_value(x, key):
                            if isinstance(x, dict):
                                if key in x and isinstance(x[key], str):
                                    return x[key]
                                for v in x.values():
                                    r = find_value(v, key)
                                    if r is not None:
                                        return r
                            elif isinstance(x, list):
                                for v in x:
                                    r = find_value(v, key)
                                    if r is not None:
                                        return r
                            return None
                        cwd = find_value(obj, "cwd")

                # Models can change within a rollout; this captures the
                # model associated with the session when available.
                def find_model(x):
                    if isinstance(x, dict):
                        for k, v in x.items():
                            if k == "model" and isinstance(v, str) and v.startswith("gpt-"):
                                return v
                            r = find_model(v)
                            if r:
                                return r
                    elif isinstance(x, list):
                        for v in x:
                            r = find_model(v)
                            if r:
                                return r
                    return None

                m = find_model(obj)
                if m:
                    model = m

                # Exact Codex token_count event.
                try:
                    payload = obj.get("payload", {})
                    if payload.get("type") != "token_count":
                        continue

                    info = payload.get("info")
                    if not info:
                        continue

                    usage = info.get("total_token_usage")
                    if not usage:
                        continue

                    # total_token_usage is cumulative for this rollout/session,
                    # so retain the greatest snapshot rather than summing events.
                    if latest is None or usage.get("total_tokens", 0) >= latest.get("total_tokens", 0):
                        latest = usage
                except Exception:
                    pass

    except Exception as e:
        print(f"Could not read {path}: {e}")
        continue

    if latest is None:
        continue

    if cwd and "/Documents/Repos/" in cwd:
        project = cwd.split("/Documents/Repos/", 1)[1].split("/", 1)[0]
    elif cwd:
        project = cwd
    else:
        project = "<unknown>"
        unknown += 1

    key = (project, model or "<unknown>")

    x = totals[key]
    x["sessions"] += 1
    x["input"] += latest.get("input_tokens", 0)
    x["cached"] += latest.get("cached_input_tokens", 0)
    x["output"] += latest.get("output_tokens", 0)
    x["reasoning"] += latest.get("reasoning_output_tokens", 0)
    x["total"] += latest.get("total_tokens", 0)

print()
print(
    f"{'Project':<20} {'Model':<18} {'Sess':>5} "
    f"{'Input':>15} {'Cached':>15} {'Output':>12} "
    f"{'Reasoning':>12} {'Total':>15}"
)
print("-" * 121)

for (project, model), x in sorted(
    totals.items(),
    key=lambda kv: kv[1]["total"],
    reverse=True
):
    print(
        f"{project:<20} {model:<18} {x['sessions']:>5,} "
        f"{x['input']:>15,} {x['cached']:>15,} "
        f"{x['output']:>12,} {x['reasoning']:>12,} "
        f"{x['total']:>15,}"
    )

print()
print(f"Rollout files scanned: {len(files):,}")
print(f"Token-bearing sessions without cwd: {unknown:,}")
PY
