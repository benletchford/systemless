# Control definition freshness

Changing a registered control definition now discards retained background
provenance from the previous painter. An unchanged definition preserves it.
Once discarded, nonuniform pixels cannot be claimed as a fresh backdrop;
a genuine unambiguous draw must establish new ownership evidence.

The public no-default-features library check passes (11.38s), before the added
lifecycle test source. Source hashes here include that later test; its result
is pending, not implied by the preceding build check. No composed lifecycle
or native Macintosh reference qualification is claimed.

The subsequent working change invalidates PPC replacement presentation before
ActivateControl/DeactivateControl requests its immediate genuine guest redraw.
This protects deferred or failed drawing from stale active-state presentation.
The existing actual PPC activation/hit-testing regression is pending. Earlier
source hashes here describe only the definition-change checkpoint.

The registry lifecycle regression now passes: one test, 0.01s after1m29
compilation. It checks unchanged definition retention, changed definition
rejection and inability to recapture ambiguous leftover painter ink. The later
PPC activation invalidation is outside that test's scope.

The actual PPC activation/hit-testing regression passes: one test,0.07s.
It verifies inactive controls cannot be hit, reactivation restores hit-testing,
and the guest highlight field is unchanged. This does not establish composed
inactive appearance; activation-source hashes pin this later checkpoint.
