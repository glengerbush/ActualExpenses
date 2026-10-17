---
status: accepted
---

# Connect directly to Actual's synchronization endpoints

ActualExpenses will connect directly to the existing Actual server from Android. Its official Node.js library provides budget operations locally, but using it would require either a companion server service or an embedded Node runtime and Android-compatible native dependencies. The user chose direct integration for now, accepting responsibility for compatibility with Actual's synchronization protocol and relevant budget schema.

Validate the approach first against a disposable budget, covering account reads and expense creation, editing, and deletion with verification in Actual Desktop. Revisit this decision if the prototype demonstrates impractical compatibility costs. The narrow user interface does not imply that the server offers equally narrow transaction endpoints.
