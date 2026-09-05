"""Offline diagnosis of a trained policy head, against held-out self-play.

    scripts/venv/bin/python scripts/diagnose_policy_head.py [games] [model] [corpus_dir]

Answers what the training logs cannot. `validation policy loss` and
`best_epoch` are both computed against the *unmasked* objective the model is
trained on, so they can look healthy while the ranking the search actually
consumes is worthless — which is exactly what happened across three volume
runs (ROADMAP Phase 2 section 5). This asks the two questions that matter
instead: how much of the model's softmax mass lands on legal slots at all,
and whether its ranking of the legal ones agrees with the search it was
distilled from.

**Point it at a corpus the model never trained on.** The default is
iteration 9, generated after `rejected_iter_008.onnx` was fitted and outside
its four-iteration window. Stalled games are dropped exactly as the trainer
drops them.

The legal set is taken as the support of the recorded `policy_target`. That
target is the *root* visit distribution and PUCT expands every legal root
edge, so with 128 simulations over a mean ~7 legal actions the support is
the legal set; it is a lower bound in any case, which makes the reported
legal mass an upper bound.

No `onnxruntime` needed, and deliberately so: the exported net is a plain
MLP (Gemm / LayerNorm / Relu, two heads), so the weights are read straight
off the ONNX initializers with `onnx` — already a training dependency — and
the forward pass is reproduced here in numpy. One less thing to install on a
machine that only wants to read a checkpoint.
"""
import json, sys, glob
import numpy as np
import onnx
from onnx import numpy_helper

N_GAMES = int(sys.argv[1]) if len(sys.argv) > 1 else 200
MODEL = sys.argv[2] if len(sys.argv) > 2 else "checkpoints/rejected_iter_008.onnx"
CORPUS = sys.argv[3] if len(sys.argv) > 3 else "data/selfplay/iter_009"
GAMES = sorted(glob.glob(f"{CORPUS}/game_*.jsonl"))[:N_GAMES]
if not GAMES:
    sys.exit(f"no game_*.jsonl under {CORPUS}")

g = onnx.load(MODEL).graph
W = {i.name: numpy_helper.to_array(i) for i in g.initializer}
EPS = {}
for n in g.node:
    if n.op_type == "LayerNormalization":
        EPS[n.output[0]] = next((a.f for a in n.attribute if a.name == "epsilon"), 1e-5)
eps = list(EPS.values())[0] if EPS else 1e-5

def gemm(x, w, b):          # torch Linear export: Y = X @ W.T + b
    return x @ w.T + b

def layernorm(x, w, b):
    mu = x.mean(-1, keepdims=True)
    var = x.var(-1, keepdims=True)
    return (x - mu) / np.sqrt(var + eps) * w + b

def forward(obs):
    h = layernorm(gemm(obs, W["trunk.0.weight"], W["trunk.0.bias"]), W["trunk.1.weight"], W["trunk.1.bias"])
    h = np.maximum(h, 0)
    h = layernorm(gemm(h, W["trunk.3.weight"], W["trunk.3.bias"]), W["trunk.4.weight"], W["trunk.4.bias"])
    h = np.maximum(h, 0)
    p = np.maximum(gemm(h, W["policy_head.0.weight"], W["policy_head.0.bias"]), 0)
    logits = gemm(p, W["policy_head.2.weight"], W["policy_head.2.bias"])
    v = np.maximum(gemm(h, W["value_head.0.weight"], W["value_head.0.bias"]), 0)
    value = np.tanh(gemm(v, W["value_head.2.weight"], W["value_head.2.bias"]))
    return logits, value[:, 0]

# `ActionSpace`'s segment layout (`action_mask.rs`), whose consts are
# private. Derived here and checked against the pinned `SIZE` below: if the
# lengths stop summing to 1646 the layout moved and this table is stale, so
# the assertion is the guard rather than a comment asking to be trusted.
_H, _I, _REMOTE, _ABIL, _SUBS = 16, 32, 10, 4, 8
_DECK, _ACCESS, _COST, _PENDING, _TRACE = 50, 32, 2, 4, 30
_ZONE = 3 + _REMOTE
_SEGMENT_LENGTHS = [
    ("basic action", 9), ("draw", 2), ("gain credit", 2), ("pass priority", 2),
    ("install card", _H * _ZONE * 2), ("rez ice", _I), ("initiate run", _ZONE),
    ("play event", _H), ("install hardware", _H), ("install program", _H),
    ("play operation", _H), ("discard", _H),
    ("activate ability (corp)", _I * _ABIL), ("activate ability (runner)", _I * _ABIL),
    ("ADVANCE CARD", _I), ("SCORE AGENDA", _I), ("trash resource", _I),
    ("select card to access", _ACCESS), ("steal agenda", 1), ("trash accessed", 1),
    ("pass accessed", 1), ("pay access trigger", 1), ("decline trigger", 1),
    ("corp trace bid", _TRACE + 1), ("runner trace bid", _TRACE + 1),
    ("accept paid choice", 1 + _COST), ("resolve pending choice", _PENDING),
    ("toggle card selection", _DECK), ("confirm card selection", 1),
    ("choose server", _ZONE), ("install resource", _H),
    ("install program on ice", _H * _I), ("break subroutine (click)", _SUBS),
    ("choose trigger", _I),
]
SEGMENTS, _off = [], 0
for _name, _len in _SEGMENT_LENGTHS:
    SEGMENTS.append((_name, _off, _off + _len))
    _off += _len

OBS_SIZE, SIZE = 990, 1646
assert _off == SIZE, f"segment table sums to {_off}, not ActionSpace::SIZE ({SIZE}) -- the layout moved"
rows_obs, rows_sup, rows_tgt, rows_side = [], [], [], []
skipped_stall = 0
for path in GAMES:
    with open(path) as f:
        game = json.load(f)
    if str(game.get("end_reason", "")).startswith("stall_"):
        skipped_stall += 1
        continue
    for st in game["steps"]:
        o = np.zeros(OBS_SIZE, dtype=np.float32)
        for i, v in st["observation"]:
            o[i] = v
        sup = np.array([i for i, _ in st["policy_target"]], dtype=np.int32)
        tgt = np.array([v for _, v in st["policy_target"]], dtype=np.float32)
        if len(sup) < 2:
            continue                      # no decision to rank
        rows_obs.append(o); rows_sup.append(sup); rows_tgt.append(tgt); rows_side.append(st["active_side"])

obs = np.stack(rows_obs)
print(f"{MODEL} over {CORPUS}")
print(f"games {len(GAMES)} ({skipped_stall} stalled, dropped)   steps {len(obs)}")
logits = np.empty((len(obs), SIZE), dtype=np.float32)
for a in range(0, len(obs), 4096):
    logits[a:a+4096], _ = forward(obs[a:a+4096])

full = np.exp(logits - logits.max(1, keepdims=True))
full /= full.sum(1, keepdims=True)

legal_mass, top1, top1_masked_uniform, argmax_legal, n_legal = [], [], [], [], []
peak, tgt_peak, on_best, ent_ratio = [], [], [], []
# Per side, per segment: [steps where the segment had a legal slot,
# summed search mass, summed model mass].
seg_stats = {0: np.zeros((len(SEGMENTS), 3)), 1: np.zeros((len(SEGMENTS), 3))}
side_hit = {0: [], 1: []}
for k in range(len(obs)):
    sup, tgt = rows_sup[k], rows_tgt[k]
    legal_mass.append(full[k, sup].sum())
    argmax_legal.append(bool(logits[k].argmax() in set(sup.tolist())))
    sub = logits[k, sup]
    pred = sup[sub.argmax()]
    best = sup[tgt.argmax()]
    hit = bool(pred == best)
    top1.append(hit)
    side_hit[rows_side[k]].append(hit)
    top1_masked_uniform.append(1.0 / len(sup))
    n_legal.append(len(sup))
    # The prior the search actually consumes: full softmax conditioned on
    # the legal set, which is what `masked_softmax` computes.
    e = np.exp(sub - sub.max()); q = e / e.sum()
    peak.append(q.max())
    tgt_peak.append(tgt.max())
    on_best.append(q[tgt.argmax()])
    h = -(q * np.log(q + 1e-12)).sum()
    ent_ratio.append(h / np.log(len(sup)))
    acc = seg_stats[rows_side[k]]
    for si, (_name, lo, hi) in enumerate(SEGMENTS):
        inside = (sup >= lo) & (sup < hi)
        if inside.any():
            acc[si, 0] += 1
            acc[si, 1] += tgt[inside].sum()
            acc[si, 2] += q[inside].sum()

lm = np.array(legal_mass); t1 = np.array(top1, dtype=float)
print()
print(f"mean legal actions/step         {np.mean(n_legal):.2f}")
print(f"MASS ON LEGAL SLOTS  mean       {lm.mean()*100:.3f}%   median {np.median(lm)*100:.3f}%")
print(f"                     p10/p90    {np.percentile(lm,10)*100:.3f}% / {np.percentile(lm,90)*100:.3f}%")
print(f"unmasked argmax is a legal slot {np.mean(argmax_legal)*100:.1f}%")
print()
print(f"TOP-1 AGREEMENT with search     {t1.mean()*100:.1f}%   (+-{1.96*t1.std()/np.sqrt(len(t1))*100:.1f})")
print(f"  chance (uniform over legal)   {np.mean(top1_masked_uniform)*100:.1f}%")
print(f"  Corp steps                    {np.mean(side_hit[0])*100:.1f}%  (n={len(side_hit[0])})")
print(f"  Runner steps                  {np.mean(side_hit[1])*100:.1f}%  (n={len(side_hit[1])})")

peak=np.array(peak); tgt_peak=np.array(tgt_peak); on_best=np.array(on_best); ent_ratio=np.array(ent_ratio)
unif=np.array(top1_masked_uniform)
print()
print("PEAKEDNESS of the prior the search consumes")
print(f"  model  max prior   mean {peak.mean():.3f}   median {np.median(peak):.3f}")
print(f"  search max visits  mean {tgt_peak.mean():.3f}   median {np.median(tgt_peak):.3f}")
print(f"  uniform would be        {unif.mean():.3f}")
print(f"  entropy / log(n_legal): model {ent_ratio.mean():.3f}  (1.000 = uniform)")
print()
print(f"  prior mass on the search's BEST action  {on_best.mean():.3f}")
print(f"  ...uniform would give                   {unif.mean():.3f}")
print()
print("STARVATION: prior the model gives the search's best action")
for thr in (0.02, 0.05, 0.10):
    print(f"  below {thr:.2f}: {np.mean(on_best<thr)*100:5.1f}% of steps   (uniform is never below {unif.mean():.2f} on average)")
for q in (10,25,50,75,90):
    print(f"  p{q:<2d} {np.percentile(on_best,q):.3f}", end="")
print()
wrong = ~np.array(top1)
print(f"  when the model is WRONG (n={wrong.sum()}): mass on the correct action  mean {on_best[wrong].mean():.3f}  median {np.median(on_best[wrong]):.3f}")
print(f"      and on its own (wrong) pick:                                mean {peak[wrong].mean():.3f}")
print()
print()
print("WHERE THE MASS GOES, per chair, over steps where that segment was legal at all.")
print("`search` is the visit distribution the model was trained to imitate; `model` is")
print("the prior the search actually consumed. `ratio` below 1.0 is under-weighting.")
for side, label in ((0, "CORP"), (1, "RUNNER")):
    acc = seg_stats[side]
    print(f"\n  --- {label} ---")
    print(f"  {'segment':26s} {'slots':>6s} {'steps':>7s} {'search':>8s} {'model':>8s} {'ratio':>7s}")
    order = np.argsort(-acc[:, 1])
    for si in order:
        steps, search_m, model_m = acc[si]
        # Everything the search actually puts mass on. Not a top-N: hiding
        # a segment is how "the Corp never scores" would stay invisible.
        if steps == 0 or search_m / max(steps, 1) < 0.005:
            continue
        name, lo, hi = SEGMENTS[si]
        s_mean, m_mean = search_m / steps, model_m / steps
        ratio = m_mean / s_mean if s_mean > 0 else float("nan")
        print(f"  {name:26s} {hi - lo:6d} {int(steps):7d} {s_mean:8.3f} {m_mean:8.3f} {ratio:7.2f}")
print()
print("AGREEMENT vs how much mass the model puts on legal slots")
edges=np.percentile(lm,[0,20,40,60,80,100])
for i in range(5):
    m=(lm>=edges[i])&(lm<=edges[i+1] if i==4 else lm<edges[i+1])
    print(f"  legal mass {edges[i]*100:5.1f}-{edges[i+1]*100:5.1f}%  n={m.sum():6d}  top-1 {t1[m].mean()*100:5.1f}%   chance {unif[m].mean()*100:4.1f}%   peak {peak[m].mean():.3f}")
