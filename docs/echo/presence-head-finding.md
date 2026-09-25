## Finding: the published `presence-head` is a degenerate constant classifier (always "present")

**TL;DR:** The presence head shipped in `ruvnet/wifi-densepose-pretrained` (`presence-head.json`, and `presence_head.*` in `model.safetensors`) cannot output "absent" for **any** input. Its bias term is large enough to saturate the sigmoid regardless of the embedding, so it's a constant-true function with zero discriminative value. This is the mechanism behind the (already-retracted) "100% presence" number, and it's worth flagging so downstream users don't wire it expecting a working classifier.

### The math

The head is `sigmoid(w · z + b)` where `z` is the encoder's **L2-normalized** 128-dim embedding (unit norm), so the logit is bounded:

```
logit = w·z + b  ∈  [ b − ‖w‖ ,  b + ‖w‖ ]
```

From the published weights:
- `b (bias)      = 8.1883`
- `‖w‖ (L2 norm) = 3.6677`

So:
```
logit ∈ [8.1883 − 3.6677, 8.1883 + 3.6677] = [4.521, 11.856]
sigmoid(logit) ∈ [0.98924, 0.99999]
```

With the documented threshold (`presence if prob > 0.3`, ADR-071), **every possible input clears the threshold** → the head always reports "present". Minimum achievable probability is 0.989.

### Reproduction (no dependencies beyond stdlib)

```python
import json, math
d = json.load(open("presence-head.json"))
w, b = d["weights"], d["bias"]
norm = math.sqrt(sum(x*x for x in w))
sig = lambda x: 1/(1+math.exp(-x))
print("bias =", round(b,4), "| ||w|| =", round(norm,4))
print("prob range =", (round(sig(b-norm),5), round(sig(b+norm),5)))
# -> bias = 8.1883 | ||w|| = 3.6677
# -> prob range = (0.98924, 0.99999)
```

### Why this happened (consistent with the repo's own notes)

This matches the README's own retraction: the encoder + head were trained on a single overnight capture where **6062 / 6063 frames are labelled "present"** (one sleeping person). A head trained on 99.98%-single-class data converges to "always yes" — the bias absorbs the class prior and the weight direction becomes irrelevant. Corroborating signal: the recovered input standardizer (first 32 bytes of `csi-embed-v2-int4.bin`, 8 fp16 means + 8 fp16 stds) has **std ≈ 0 for the person-count and fall-detected dimensions** — i.e. those features were constant in training (always 1 person, never a fall), exactly as expected for that recording.

### Suggested fix / note

The **encoder itself is fine** — its 82.3% held-out temporal-triplet metric is real, and its embeddings do separate empty vs. occupied (cosine ~0.47 between the two in my testing). The issue is specific to the **presence head**. Options:
- Mark the presence head as non-functional / retrain on multi-class data, or
- Add a note in the model card that the head is a single-class artifact (the "100%" retraction covers the *number* but not that the head is a *constant function* — a user could still load it and get permanent "present").

Happy to share the full teardown (standardizer recovery + faithful forward pass reimplementation) if useful.
