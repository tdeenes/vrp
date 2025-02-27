# Summary

[Introduction](index.md)

* [Getting Started](getting-started/index.md)
  * [Features](getting-started/features.md)
  * [Installation](getting-started/installation.md)
  * [Defining problem](getting-started/import.md)
  * [Acquiring routing info](getting-started/routing.md)
  * [Running solver](getting-started/solver.md)
  * [Analyzing results](getting-started/analysis.md)
  * [Evaluating performance](getting-started/performance.md)

* [Concepts](concepts/index.md)
  * [Pragmatic format](concepts/pragmatic/index.md)
    * [Modeling a problem](concepts/pragmatic/problem/index.md)
      * [Jobs](concepts/pragmatic/problem/jobs.md)
      * [Vehicles](concepts/pragmatic/problem/vehicles.md)
      * [Relations](concepts/pragmatic/problem/relations.md)
      * [Clustering](concepts/pragmatic/problem/clustering.md)
      * [Objectives](concepts/pragmatic/problem/objectives.md)
    * [Routing data](concepts/pragmatic/routing/index.md)
        * [Routing matrix](concepts/pragmatic/routing/format.md)
        * [Profiles](concepts/pragmatic/routing/profile.md)
    * [Solution model](concepts/pragmatic/solution/index.md)
      * [Tour list](concepts/pragmatic/solution/tour-list.md)
      * [Statistic](concepts/pragmatic/solution/statistic.md)
      * [Unassigned jobs](concepts/pragmatic/solution/unassigned-jobs.md)
      * [Violations](concepts/pragmatic/solution/violations.md)
    * [Error index](concepts/pragmatic/errors/index.md)
  * [Scientific formats](concepts/scientific/index.md)
    * [Solomon benchmark](concepts/scientific/solomon.md)
    * [Li&Lim benchmark](concepts/scientific/lilim.md)
    * [TSPLIB format](concepts/scientific/tsplib.md)

* [Examples](examples/index.md)
  * [Pragmatic format](examples/pragmatic/index.md)
    * [Basic feature usage](examples/pragmatic/basics/index.md)
        * [Basic job usage](examples/pragmatic/basics/job-types.md)
        * [Job priorities](examples/pragmatic/basics/job-priorities.md)
        * [Multi day plan](examples/pragmatic/basics/multi-day.md)
        * [Vehicle dispatch](examples/pragmatic/basics/dispatch.md)
        * [Vehicle break](examples/pragmatic/basics/break.md)
        * [Multiple trips](examples/pragmatic/basics/reload.md)
        * [Relations](examples/pragmatic/basics/relations.md)
        * [Skills](examples/pragmatic/basics/skills.md)
        * [Area order](examples/pragmatic/basics/area-order.md)
        * [Multiple profiles](examples/pragmatic/basics/profiles.md)
        * [Unassigned job](examples/pragmatic/basics/unassigned.md)
    * [Clustering](examples/pragmatic/clustering/index.md)
      * [Vicinity continuation](examples/pragmatic/clustering/vicinity-continue.md)
      * [Vicinity return](examples/pragmatic/clustering/vicinity-return.md)
    * [Objective usage](examples/pragmatic/objectives/index.md)
        * [Default behavior](examples/pragmatic/objectives/objective-default.md)
        * [Balance max load](examples/pragmatic/objectives/objective-balance-max-load.md)
        * [Balance activities](examples/pragmatic/objectives/objective-balance-activities.md)
        * [Balance distance](examples/pragmatic/objectives/objective-balance-distance.md)
  * [Language interop](examples/interop/index.md)
    * [Java](examples/interop/java.md)
    * [Kotlin](examples/interop/kotlin.md)
    * [Javascript](examples/interop/javascript.md)
    * [Python](examples/interop/python.md)

* [Internals](internals/index.md)
  * [Overview](internals/overview.md)
  * [Development](internals/development/index.md)
    * [Code style](internals/development/code-style.md)
    * [Testing](internals/development/testing.md)
  * [Algorithms](internals/algorithms/index.md)
    * [Rosomaxa](internals/algorithms/rosomaxa.md)
    * [Hyper-heuristic](internals/algorithms/hyper.md)
