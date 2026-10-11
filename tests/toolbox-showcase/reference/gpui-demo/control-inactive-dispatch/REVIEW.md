# Current-state system-control dispatch

Dialog and window buttons, checkboxes and radio buttons now use weak Demo callbacks that revalidate host activity and current guest identity/state when invoked. Dialog item identity is checked; document controls require a visible active owner. Notification and Standard File ownership exclude these actions. Pointer delivery retains the existing guest event path.

Eight themed frontend regressions pass on the final source (20.85s). Direct helper checks exercise three dialog kinds and three window-control definitions without rerendering, rejecting inactive hosts, stale generations and disabled items; dialog checks also reject changed control identity, while window checks reject hidden controls. Existing pointer tests verify exactly one guest press/release. Active-host focus tests explicitly establish simulated host activity.

This establishes shared Demo dispatch contracts and GPUI test-window behavior, not native accessibility callback delivery, physical activation, comprehensive lifecycle coverage or production readiness. No appearance capture was generated. All eight broad release gates remain open.
